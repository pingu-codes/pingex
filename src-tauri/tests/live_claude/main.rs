//! Live end-to-end tests against a real `claude` CLI on a cheap model.
//! Run with `bun run test:e2e:claude`; skipped unless `PINGEX_LIVE_E2E=1`.

mod harness;

use serde_json::Value;

#[test]
fn resumes_an_unapproved_plan_and_implements_with_bypass() {
    let setup = live!();
    let claude = harness::spawn(setup, "plan");
    claude.send_prompt("Plan to create restart-check.txt in the current directory containing exactly PERIWINKLE-384. Do not implement yet. Finish by calling ExitPlanMode to request approval.");
    let (_, _, request) = expect_plan_request(&claude);
    assert_eq!(request["tool_name"], "ExitPlanMode");
    let session = claude.session_id.clone();
    // Like closing Pingex while the plan is awaiting approval: no result and
    // no permission answer, but Claude's transcript must remain resumable.
    drop(claude);
    let claude = harness::resume_in_mode(setup, session.clone(), "plan");
    claude.send_prompt("Continue planning the same file and exact content from our previous conversation. Keep the plan minimal. Call ExitPlanMode again to request approval; do not implement before approval.");
    let (_, init) = claude.expect_frame(0, "system", "resumed init");
    assert_eq!(init["session_id"], session);
    assert_eq!(init["permissionMode"], "plan");
    let (next, request_id, request) = expect_plan_request(&claude);
    assert_eq!(request["tool_name"], "ExitPlanMode");
    claude.respond_permission(&request_id, "plan_mode_bypassPermissions", &request);
    let (next, _) = claude.expect_success(next, "implementing the resumed plan");
    assert_eq!(
        std::fs::read_to_string(setup.work.join("restart-check.txt"))
            .unwrap()
            .trim(),
        "PERIWINKLE-384"
    );
    // The CLI reports its effective mode at the start of the following turn.
    claude.send_prompt("Reply with exactly OK, without using any tools.");
    let (_, init) = claude
        .wait_for(next, harness::TURN_TIMEOUT, |frame| {
            frame["type"] == "system"
                && frame["subtype"] == "init"
                && frame["permissionMode"] == "bypassPermissions"
        })
        .expect("CLI must retain Bypass after plan approval");
    assert_eq!(init["session_id"], session);
    claude.expect_success(next, "the next turn after implementation");
}

fn expect_plan_request(claude: &harness::ClaudeProcess) -> (usize, String, Value) {
    let (next, frame) = claude
        .wait_for(0, harness::TURN_TIMEOUT, |frame| {
            frame["type"] == "result"
                || (frame["type"] == "control_request"
                    && frame["request"]["subtype"] == "can_use_tool")
        })
        .expect("plan approval request");
    assert_ne!(
        frame["type"], "result",
        "Claude ended before requesting plan approval: {}",
        frame["result"]
    );
    (
        next,
        frame["request_id"]
            .as_str()
            .expect("request id")
            .to_string(),
        frame["request"].clone(),
    )
}

fn side_reply(
    claude: &harness::ClaudeProcess,
    from: usize,
    id: &str,
    question: &str,
    history: Value,
) -> (usize, Value) {
    claude.write_frame(&serde_json::json!({
        "type": "control_request", "request_id": id,
        "request": {"subtype": "side_question", "question": question, "history": history},
    }));
    claude
        .wait_for(from, harness::TURN_TIMEOUT, |frame| {
            frame["type"] == "control_response"
                && frame["response"]["request_id"] == id
                && matches!(
                    frame["response"]["subtype"].as_str(),
                    Some("success" | "error")
                )
        })
        .expect("terminal side-question control response")
}

#[test]
fn native_side_questions_recall_context_follow_up_and_resume() {
    let setup = live!();
    let claude = harness::spawn(setup, "default");
    claude.send_prompt("Remember the project code CEDAR-481. Reply exactly OK.");
    let (next, _) = claude.expect_success(0, "seed context");
    let (next, first) = side_reply(
        &claude,
        next,
        "btw-one",
        "What is the project code? Reply with only the code.",
        serde_json::json!([]),
    );
    assert_eq!(first["response"]["subtype"], "success", "{first}");
    assert!(first["response"]["response"]["response"]
        .as_str()
        .unwrap()
        .contains("CEDAR-481"));
    let (_, follow) = side_reply(
        &claude,
        next,
        "btw-two",
        "What colour did I mention in the side conversation? Reply with only the colour.",
        serde_json::json!([{"question": "The side-only colour is vermilion.", "response": "Noted."}]),
    );
    assert!(
        follow["response"]["response"]["response"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("vermilion"),
        "{follow}"
    );
    let session = claude.session_id.clone();
    drop(claude);
    let claude = harness::resume(setup, session);
    claude.write_frame(&serde_json::json!({"type": "control_request", "request_id": "initialize", "request": {"subtype": "initialize"}}));
    let (next, _) = claude
        .wait_for(0, harness::TURN_TIMEOUT, |f| {
            f["type"] == "control_response" && f["response"]["request_id"] == "initialize"
        })
        .expect("resumed session initializes without sending a main prompt");
    let (_, resumed) = side_reply(
        &claude,
        next,
        "btw-resume",
        "What is the project code? Reply with only the code.",
        serde_json::json!([]),
    );
    assert!(
        resumed["response"]["response"]["response"]
            .as_str()
            .unwrap()
            .contains("CEDAR-481"),
        "{resumed}"
    );
}

#[test]
fn native_side_question_cancel_leaves_main_approval_alive() {
    let setup = live!();
    let claude = harness::spawn(setup, "default");
    claude.send_prompt("Reply exactly OK.");
    let (next, _) = claude.expect_success(0, "side-question authentication check");
    claude.send_prompt("Use the Write tool to create side-test.txt containing CEDAR. Then stop.");
    let (next, approval_id, permission) = claude.expect_can_use_tool(next);
    claude.write_frame(&serde_json::json!({"type": "control_request", "request_id": "btw-cancel",
        "request": {"subtype": "side_question", "question": "Explain the context in detail.", "history": []}}));
    claude.write_frame(
        &serde_json::json!({"type": "control_cancel_request", "request_id": "btw-cancel"}),
    );
    let (next, cancelled) = claude
        .wait_for(next, harness::TURN_TIMEOUT, |f| {
            f["type"] == "control_response"
                && f["response"]["request_id"] == "btw-cancel"
                && f["response"]["subtype"] == "error"
        })
        .expect("side question acknowledges cancellation");
    assert!(cancelled["response"]["error"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("cancel"));
    claude.respond_permission(&approval_id, "deny", &permission);
    claude.expect_success(
        next,
        "main turn completes after its still-live approval is answered",
    );
    assert!(!setup.work.join("side-test.txt").exists());
}

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

/// Changing effort on an idle thread restarts the CLI under the same session
/// (`ClaudeDriver::ensure_process`), since `--effort` is fixed at spawn. The
/// process resumed at the new effort must still have the conversation.
#[test]
fn a_session_resumed_at_another_effort_keeps_its_context() {
    let setup = live!();
    let claude = harness::spawn(setup, "default");
    claude.send_prompt("Remember the codeword OTTER-5521. Reply with exactly OK.");
    claude.expect_success(0, "the first turn");
    let session = claude.session_id.clone();
    drop(claude);

    let claude = harness::resume_with_effort(setup, session.clone(), "high");
    claude.send_prompt("What was the codeword? Reply with the codeword only.");
    let (_, init) = claude.expect_frame(0, "system", "resumed init");
    assert_eq!(init["session_id"], session);
    let (_, result) = claude.expect_success(0, "the turn at the new effort");
    let text = result["result"].as_str().unwrap_or_default();
    assert!(
        text.contains("OTTER-5521"),
        "context lost on restart: {result}"
    );
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
