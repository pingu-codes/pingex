//! Live end-to-end tests against a real `claude` CLI on a cheap model.
//! Run with `deno task test:e2e:claude`; skipped unless `PINGEX_LIVE_E2E=1`.

mod harness;

use serde_json::Value;

/// The `system/init` frame reports a session we recognise and a real model —
/// the auth canary: a logged-out config dir fails here with the CLI's
/// "please run /login" result instead.
#[test]
fn init_reports_a_logged_in_session() {
    let setup = live!();
    let claude = harness::spawn(setup, "default");
    claude.send_prompt("Reply with exactly OK");
    let (_, init) = claude
        .wait_for(0, harness::TURN_TIMEOUT, |frame| {
            frame.get("type").and_then(Value::as_str) == Some("system")
                && frame.get("subtype").and_then(Value::as_str) == Some("init")
        })
        .unwrap_or_else(|| panic!("no system/init\n{}", claude.diagnostics(0)));
    assert_eq!(
        init["session_id"].as_str(),
        Some(claude.session_id.as_str()),
        "init: {init}"
    );
    let model = init["model"].as_str().unwrap_or_default();
    assert!(model.contains("haiku"), "unexpected model in init: {init}");
    claude.expect_success(0, "the canary turn");
}

/// A plain prompt round-trips: the turn ends in a successful `result`
/// carrying the model's text.
#[test]
fn simple_echo_turn() {
    let setup = live!();
    let claude = harness::spawn(setup, "default");
    claude.send_prompt("Reply with exactly PINGEX-OK and nothing else");
    let (_, result) = claude.expect_success(0, "the echo turn");
    let text = result["result"].as_str().unwrap_or_default();
    assert!(
        text.contains("PINGEX-OK"),
        "unexpected result text: {result}"
    );
}

/// After a turn, `get_context_usage` reports the live context by category,
/// and the categories that are in use add up to its own total — what
/// `read_context_breakdown` relies on (`features/15-usage.md`).
#[test]
fn context_usage_reports_categories_that_sum_to_the_total() {
    let setup = live!();
    let claude = harness::spawn(setup, "default");
    claude.send_prompt("Reply with exactly PINGEX-OK and nothing else");
    let (after, _) = claude.expect_success(0, "the echo turn");
    claude.write_frame(&serde_json::json!({
        "type": "control_request",
        "request_id": "pingex-ctx",
        "request": {"subtype": "get_context_usage"},
    }));
    let (_, frame) = claude
        .wait_for(after, std::time::Duration::from_secs(30), |frame| {
            frame["type"] == "control_response" && frame["response"]["request_id"] == "pingex-ctx"
        })
        .expect("a control_response for get_context_usage");
    let response = &frame["response"]["response"];
    let total = response["totalTokens"].as_u64().unwrap_or(0);
    assert!(total > 0, "no total in {response}");
    assert!(response["maxTokens"].as_u64().unwrap_or(0) >= total);
    let categories = response["categories"].as_array().expect("categories");
    let in_use: u64 = categories
        .iter()
        .filter(|c| {
            let name = c["name"].as_str().unwrap_or_default().to_ascii_lowercase();
            !c["isDeferred"].as_bool().unwrap_or(false)
                && !name.contains("free")
                && !name.contains("buffer")
        })
        .map(|c| c["tokens"].as_u64().unwrap_or(0))
        .sum();
    assert!(
        (in_use as i64 - total as i64).abs() <= 2,
        "categories in use sum to {in_use}, CLI says {total}: {response}"
    );
    assert!(
        categories.iter().any(|c| c["name"] == "System prompt"),
        "no system prompt category: {response}"
    );
}

/// A Write outside the auto-allowed set raises `can_use_tool`; answering it
/// through the driver's own permission mapping lets the tool run.
#[test]
fn tool_approval_flow() {
    let setup = live!();
    let claude = harness::spawn(setup, "default");
    claude.send_prompt(
        "Use the Write tool to create a file named note.txt containing exactly APPROVED. \
         Then stop.",
    );
    let (next, request_id, request) = claude.expect_can_use_tool(0);
    assert_eq!(
        request["tool_name"].as_str(),
        Some("Write"),
        "unexpected tool request: {request}"
    );
    claude.respond_permission(&request_id, "allow", &request);
    claude.expect_success(next, "the approved write turn");
    let note = setup.work.join("note.txt");
    let content = std::fs::read_to_string(&note)
        .unwrap_or_else(|error| panic!("note.txt missing after approval: {error}"));
    assert!(
        content.contains("APPROVED"),
        "note.txt content: {content:?}"
    );
    let _ = std::fs::remove_file(note);
}

/// Denying the prompt still ends the turn cleanly — no hang, no dead process.
#[test]
fn tool_denial_ends_the_turn() {
    let setup = live!();
    let claude = harness::spawn(setup, "default");
    claude.send_prompt(
        "Use the Write tool to create a file named denied.txt containing NO. Then stop.",
    );
    let (next, request_id, request) = claude.expect_can_use_tool(0);
    claude.respond_permission(&request_id, "deny", &request);
    let (_, result) = claude.expect_frame(next, "result", "the denied turn's result");
    assert!(
        result.get("subtype").is_some(),
        "no result after denial: {result}"
    );
    assert!(
        !setup.work.join("denied.txt").exists(),
        "denied.txt was written despite the deny"
    );
}
