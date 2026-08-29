//! Live end-to-end suite: the app's real request payloads against a real
//! `codex app-server` and a real (cheap) model.
//!
//! Why this exists: a `{type:"skill", name}` input once shipped without the
//! `path` the protocol requires. Unit tests could not catch that — only the
//! server knows what it accepts — so this suite replays exactly what the app
//! sends (`pingex_app_lib::e2e::requests`, plus the composer's golden inputs
//! in `tests/fixtures/protocol/turn-inputs.json`) and checks the server takes
//! it and the model does what the input asked.
//!
//! Run:  `deno task test:e2e:codex`   (PINGEX_LIVE_E2E=1, needs a Codex login)
//! Skips silently otherwise so `cargo test` stays offline.

#[macro_use]
mod harness;
mod features;
mod fixtures;

use harness::{Server, TurnOutcome, MCP_SERVER, MCP_TOOL, SKILL_NAME, TURN_TIMEOUT};
use pingex_app_lib::e2e::requests::{self, TurnOptions};
use pingex_app_lib::e2e::{
    agent_tool_specs, collect_model_ids, parse_skills, sandbox_tag, Feature, AGENT_PREAMBLE,
    DELEGATION_POLICY, NAMER_INSTRUCTIONS,
};
use serde_json::{json, Value};
use std::time::Duration;

/// A turn's reply must contain `token`; failures show the whole outcome.
fn assert_reply_contains(outcome: &TurnOutcome, token: &str, context: &str) {
    let reply = outcome.reply();
    assert!(
        reply.contains(token),
        "{context}: expected reply to contain {token:?}, got {reply:?}\nstatus: {}\nitems: {:?}",
        outcome.status,
        outcome.item_types()
    );
}

fn low_effort(server: &Server) -> Option<TurnOptions> {
    Some(TurnOptions {
        model: Some(server.model.clone()),
        effort: Some("low".into()),
        ..TurnOptions::default()
    })
}

fn text_input(text: &str) -> Vec<Value> {
    vec![json!({"type": "text", "text": text})]
}

// ── discovery ─────────────────────────────────────────────────────────────

#[test]
fn model_list_offers_the_configured_model() {
    let server = live!();
    let response = server.call(requests::model_list(100, true));
    let ids = collect_model_ids(&response);
    assert!(
        ids.iter().any(|id| id == &server.model),
        "{} not in model/list: {ids:?}",
        server.model
    );
    // The frontend's `dynamicTools` enum is built from this list.
    let specs = agent_tool_specs(&ids);
    assert!(specs.is_array(), "agent tool specs are an array: {specs}");
}

#[test]
fn skills_list_reports_our_skill_with_a_path() {
    let server = live!();
    let response = server.call(requests::skills_list(&[server.cwd.display().to_string()]));
    let skills = parse_skills(&response);
    let skill = skills
        .iter()
        .find(|skill| skill.name == SKILL_NAME)
        .unwrap_or_else(|| panic!("{SKILL_NAME} missing from {skills:?}"));
    assert!(
        skill.path.ends_with("SKILL.md"),
        "path is the SKILL.md: {}",
        skill.path
    );
    assert!(skill.enabled);
    assert!(skill.description.as_deref().unwrap_or("").contains("e2e"));
}

#[test]
fn skill_config_write_toggles_enabled() {
    let server = live!();
    let cwds = [server.cwd.display().to_string()];
    let enabled_of = |server: &Server| {
        parse_skills(&server.call(requests::skills_list(&cwds)))
            .into_iter()
            .find(|skill| skill.name == SKILL_NAME)
            .map(|skill| skill.enabled)
    };
    server.call(requests::skill_config_write(SKILL_NAME, false));
    assert_eq!(enabled_of(server), Some(false), "disabled after write");
    server.call(requests::skill_config_write(SKILL_NAME, true));
    assert_eq!(enabled_of(server), Some(true), "re-enabled after write");
}

#[test]
fn mcp_server_status_lists_the_echo_server_and_its_tool() {
    let server = live!();
    let response = server.call(requests::mcp_server_status_list());
    let servers = response["data"].as_array().expect("data array");
    let echo = servers
        .iter()
        .find(|s| s["name"] == MCP_SERVER)
        .unwrap_or_else(|| panic!("{MCP_SERVER} not in mcpServerStatus/list: {response}"));
    // The Integrations view keys tools by name off this map.
    assert!(
        echo["tools"].get(MCP_TOOL).is_some(),
        "tool map has {MCP_TOOL}: {}",
        echo["tools"]
    );
    assert!(echo["authStatus"].is_string(), "authStatus present: {echo}");
    // 0.150 added the live connection state the Integrations view labels.
    if server.at_least(0, 150) {
        assert!(
            echo.get("runtimeStatus").is_some(),
            "runtimeStatus present on Codex ≥0.150: {echo}"
        );
    }
    // Reload is what every config.toml mutation calls afterwards.
    server.call(requests::mcp_config_reload());
}

// ── turns ─────────────────────────────────────────────────────────────────

#[test]
fn a_plain_text_turn_round_trips_and_is_readable() {
    let server = live!();
    let thread_id = server.start_thread();
    let outcome = server.run_turn(requests::turn_start(
        &thread_id,
        text_input("Reply with exactly PONG"),
        low_effort(server),
    ));
    assert_eq!(outcome.status, "completed");
    assert_reply_contains(&outcome, "PONG", "text turn");

    let read = server.call(requests::thread_read(&thread_id));
    let turns = read
        .pointer("/thread/turns")
        .and_then(Value::as_array)
        .expect("turns");
    assert_eq!(turns.len(), 1, "one turn read back: {read}");
    let items = turns[0]["items"].as_array().expect("items");
    let types: Vec<_> = items.iter().filter_map(|i| i["type"].as_str()).collect();
    assert!(
        types.contains(&"userMessage") && types.contains(&"agentMessage"),
        "{types:?}"
    );

    let listed = server.call(requests::thread_list(
        20,
        None,
        Some(&server.cwd.display().to_string()),
        false,
    ));
    let ids: Vec<_> = listed["data"]
        .as_array()
        .expect("data")
        .iter()
        .filter_map(|t| t["id"].as_str())
        .collect();
    assert!(
        ids.contains(&thread_id.as_str()),
        "thread/list has {thread_id}: {ids:?}"
    );
}

/// The bug this suite was written for, in both directions.
#[test]
fn every_composer_input_fixture_is_accepted_and_understood() {
    let server = live!();
    let skill_path = server.skill_path();
    let cwd = server.cwd.display().to_string();
    let image = server.image_path.display().to_string();
    let vars = [
        ("CWD", cwd.as_str()),
        ("SKILL_PATH", skill_path.as_str()),
        ("IMAGE_PATH", image.as_str()),
    ];
    let fixtures = fixtures::load();
    assert!(!fixtures.is_empty());
    let thread_id = server.start_thread();
    let mut failures = Vec::new();
    for fixture in &fixtures {
        let input = fixture
            .input
            .iter()
            .map(|item| fixtures::substitute(item, &vars))
            .collect();
        let request = requests::turn_start(&thread_id, input, low_effort(server));
        let outcome = server.run_turn(request);
        if outcome.status != "completed" {
            failures.push(format!("{}: status {}", fixture.name, outcome.status));
            continue;
        }
        if let Some(token) = &fixture.expect_reply {
            let reply = outcome.reply();
            if !reply.contains(token) {
                failures.push(format!(
                    "{} (via {}): wanted {token:?}, got {reply:?}",
                    fixture.name, fixture.via
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "fixture failures:\n{}",
        failures.join("\n")
    );
}

/// Negative control: the exact payload that shipped broken must still be
/// rejected — otherwise this suite could not have caught it.
#[test]
fn a_skill_item_without_a_path_is_rejected_by_the_server() {
    let server = live!();
    let thread_id = server.start_thread();
    let error = server
        .request(requests::turn_start(
            &thread_id,
            vec![json!({"type": "skill", "name": SKILL_NAME})],
            None,
        ))
        .expect_err("skill without path must be rejected");
    assert!(
        error.message.contains("path"),
        "rejection names the missing field: {error}"
    );
}

#[test]
fn every_permission_preset_and_option_the_composer_can_send_is_accepted() {
    let server = live!();
    let thread_id = server.start_thread();
    let model = server.model.clone();
    let cases: Vec<(&str, TurnOptions)> = vec![
        (
            "read-only preset",
            TurnOptions {
                approval_policy: Some("on-request".into()),
                sandbox_mode: Some("read-only".into()),
                ..TurnOptions::default()
            },
        ),
        (
            "auto preset",
            TurnOptions {
                approval_policy: Some("on-request".into()),
                sandbox_mode: Some("workspace-write".into()),
                ..TurnOptions::default()
            },
        ),
        (
            "full-access preset",
            TurnOptions {
                approval_policy: Some("never".into()),
                sandbox_mode: Some("danger-full-access".into()),
                ..TurnOptions::default()
            },
        ),
        (
            "plan mode + effort",
            TurnOptions {
                model: Some(model.clone()),
                effort: Some("low".into()),
                collaboration_mode: Some(json!({
                    "mode": "plan",
                    "settings": {"model": model, "reasoning_effort": "low", "developer_instructions": null},
                }).into()),
                ..TurnOptions::default()
            },
        ),
        (
            "default mode + subagent policies",
            TurnOptions {
                model: Some(model.clone()),
                collaboration_mode: Some(json!({
                    "mode": "default",
                    "settings": {"model": model, "reasoning_effort": null, "developer_instructions": null},
                }).into()),
                subagent_model_policy: Some(json!({"allowed": [model]}).into()),
                subagent_reasoning_effort_policy: Some(json!({"excluded": ["xhigh"]}).into()),
                ..TurnOptions::default()
            },
        ),
    ];
    let mut failures = Vec::new();
    for (name, options) in cases {
        let request = requests::turn_start(
            &thread_id,
            text_input("Reply with exactly OK"),
            Some(options),
        );
        let params = request.params.clone();
        match server.request(request) {
            Ok(response) => {
                let turn_id = response
                    .pointer("/turn/id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let outcome = server.await_turn(server.cursor().saturating_sub(50), &turn_id);
                if outcome.status != "completed" {
                    failures.push(format!("{name}: turn status {}", outcome.status));
                }
            }
            Err(error) => failures.push(format!("{name}: {error}\n  params: {params}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_workspace_turn_with_additional_context_is_accepted() {
    let server = live!();
    let thread_id = server.start_thread();
    let cwd = server.cwd.display().to_string();
    let mut request = requests::turn_start(
        &thread_id,
        text_input("Reply with exactly WS-OK"),
        low_effort(server),
    );
    requests::apply_workspace_params(
        &mut request.params,
        &cwd,
        std::slice::from_ref(&cwd),
        "This thread belongs to a Pingex workspace named e2e.",
    );
    let outcome = server.run_turn(request);
    assert_reply_contains(&outcome, "WS-OK", "workspace turn");
}

#[test]
fn a_thread_started_with_dynamic_agent_tools_is_accepted() {
    let server = live!();
    let models = collect_model_ids(&server.call(requests::model_list(100, false)));
    let response = server.call(requests::thread_start(
        &server.cwd.display().to_string(),
        None,
        Some(DELEGATION_POLICY),
        Some(agent_tool_specs(&models)),
    ));
    let thread_id = harness::thread_id_of(&response);
    let outcome = server.run_turn(requests::turn_start(
        &thread_id,
        text_input("Do not use any tools. Reply with exactly TOOLS-OK"),
        low_effort(server),
    ));
    assert_reply_contains(&outcome, "TOOLS-OK", "dynamic tools thread");
}

#[test]
fn resume_then_follow_up_keeps_context() {
    let server = live!();
    let thread_id = server.start_thread();
    let first = server.run_turn(requests::turn_start(
        &thread_id,
        text_input("Remember the secret word ZEBRA-42. Reply with exactly SAVED"),
        low_effort(server),
    ));
    assert_reply_contains(&first, "SAVED", "first turn");
    let resumed = server.call(requests::thread_resume(&thread_id));
    assert_eq!(harness::thread_id_of(&resumed), thread_id);
    let second = server.run_turn(requests::turn_start(
        &thread_id,
        text_input("What was the secret word? Reply with only the word."),
        low_effort(server),
    ));
    assert_reply_contains(&second, "ZEBRA-42", "follow-up after resume");
}

/// An MCP tool call asks the client to approve it via an elicitation; the
/// suite answers with the payload `ElicitationCard` sends.
#[test]
fn an_mcp_tool_call_runs_through_our_server() {
    let server = live!();
    let thread_id = server.start_thread();
    let from = server.cursor();
    let response = server.call(requests::turn_start(
        &thread_id,
        text_input(&format!(
            "Call the MCP tool `{MCP_TOOL}` from server `{MCP_SERVER}` with text \"ZED-7\" and reply with only the tool's result."
        )),
        low_effort(server),
    ));
    let turn_id = response
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .expect("turn id")
        .to_string();
    // Answer every elicitation the tool call raises until the turn ends.
    let mut cursor = from;
    let outcome = loop {
        let elicitation = server.wait_for(cursor, Duration::from_secs(90), |message| {
            let method = message.get("method").and_then(Value::as_str);
            (message.get("id").is_some() && method == Some("mcpServer/elicitation/request"))
                || (method == Some("turn/completed")
                    && message.pointer("/params/turn/id").and_then(Value::as_str) == Some(&turn_id))
        });
        let Some((next, message)) = elicitation else {
            panic!(
                "neither an elicitation nor turn/completed arrived\n{}",
                server.stderr_tail()
            );
        };
        cursor = next;
        if message.get("method").and_then(Value::as_str) == Some("turn/completed") {
            break server.await_turn(from, &turn_id);
        }
        let request_id = message["id"].as_i64().expect("elicitation id");
        assert_eq!(message.pointer("/params/threadId"), Some(&json!(thread_id)));
        server.respond(
            request_id,
            requests::elicitation_result("accept", Some(json!({}))),
        );
    };
    assert_eq!(outcome.status, "completed");
    assert!(
        outcome.item_types().contains(&"mcpToolCall"),
        "an mcpToolCall item completed: {:?}",
        outcome.item_types()
    );
    assert_reply_contains(&outcome, "ZED-7", "mcp echo");
}

#[test]
fn interrupt_stops_a_running_turn() {
    let server = live!();
    let thread_id = server.start_thread();
    let from = server.cursor();
    let response = server.call(requests::turn_start(
        &thread_id,
        text_input("Count from 1 to 400, one number per line, with no other text."),
        low_effort(server),
    ));
    let turn_id = response
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .expect("turn id")
        .to_string();
    server
        .wait_notification(from, "item/agentMessage/delta", TURN_TIMEOUT, |params| {
            params["turnId"] == turn_id
        })
        .expect("model started streaming");
    server.call(requests::turn_interrupt(&thread_id, &turn_id));
    let outcome = server.await_turn(from, &turn_id);
    assert_eq!(
        outcome.status,
        "interrupted",
        "items: {:?}",
        outcome.item_types()
    );
}

#[test]
fn approval_requests_are_answered_with_our_decision_payloads() {
    let server = live!();
    let thread_id = server.start_thread();
    let escalate = |marker: &str| {
        Some(TurnOptions {
            model: Some(server.model.clone()),
            effort: Some("low".into()),
            approval_policy: Some("on-request".into()),
            sandbox_mode: Some("read-only".into()),
            ..TurnOptions::default()
        })
        .map(|options| {
            requests::turn_start(
                &thread_id,
                text_input(&format!(
                    "You must run this exact shell command (request approval if the sandbox blocks it, do not just describe it): `echo {marker} > {marker}.txt`. Then reply with exactly DONE."
                )),
                Some(options),
            )
        })
        .unwrap()
    };

    // Accept: the file appears.
    let from = server.cursor();
    let response = server.call(escalate("APPROVED"));
    let turn_id = response
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .expect("turn id")
        .to_string();
    let (next, request_id, params) = server
        .wait_server_request(from, "item/commandExecution/requestApproval", TURN_TIMEOUT)
        .expect("server asked for approval");
    assert_eq!(params["threadId"], thread_id, "{params}");
    // 0.150 tags approvals with a kind; a plain command is `command`, and the
    // approval card keys its title off that.
    if server.at_least(0, 150) {
        assert_eq!(params["kind"], "command", "{params}");
    } else {
        assert!(params.get("kind").is_none(), "{params}");
    }
    server.respond(request_id, requests::approval_result("accept"));
    let outcome = server.await_turn(next, &turn_id);
    assert_eq!(outcome.status, "completed", "{:?}", outcome.item_types());
    let approved = server.cwd.join("APPROVED.txt");
    assert!(
        approved.is_file(),
        "accepted command ran: {}",
        approved.display()
    );

    // Decline: the file does not appear, and the turn still ends.
    let from = server.cursor();
    let response = server.call(escalate("DENIED"));
    let turn_id = response
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .expect("turn id")
        .to_string();
    let (next, request_id, _) = server
        .wait_server_request(from, "item/commandExecution/requestApproval", TURN_TIMEOUT)
        .expect("server asked for approval");
    server.respond(request_id, requests::approval_result("decline"));
    let outcome = server.await_turn(next, &turn_id);
    assert!(
        matches!(outcome.status.as_str(), "completed" | "failed"),
        "turn ended: {}",
        outcome.status
    );
    assert!(
        !server.cwd.join("DENIED.txt").exists(),
        "declined command did not run"
    );
}

// ── the app's other request builders ──────────────────────────────────────

#[test]
fn autoname_thread_and_turn_produce_a_title() {
    let server = live!();
    let started = server.call(requests::namer_thread_start(NAMER_INSTRUCTIONS));
    let namer_id = harness::thread_id_of(&started);
    let outcome = server.run_turn(requests::naming_turn(
        &namer_id,
        "User: how do I rename a git branch?\nAssistant: use git branch -m",
        Some(&server.model),
    ));
    assert_eq!(outcome.status, "completed");
    assert!(
        !outcome.reply().trim().is_empty(),
        "namer replied with a title"
    );
    server.call(requests::thread_delete(&namer_id));
}

#[test]
fn subagent_thread_turn_and_follow_up_are_accepted() {
    let server = live!();
    let started = server.call(requests::agent_thread_start(
        &server.cwd.display().to_string(),
        AGENT_PREAMBLE,
    ));
    let agent_id = harness::thread_id_of(&started);
    let outcome = server.run_turn(requests::agent_turn(
        &agent_id,
        "Reply with exactly AGENT-OK",
        sandbox_tag("read-only"),
        Some(&server.model),
        Some("low"),
    ));
    assert_reply_contains(&outcome, "AGENT-OK", "agent first turn");
    let follow = server.run_turn(requests::agent_followup(
        &agent_id,
        "Reply with exactly AGENT-AGAIN",
    ));
    assert_reply_contains(&follow, "AGENT-AGAIN", "agent follow-up");
    server.call(requests::thread_delete(&agent_id));
}

// ── version-dependent APIs ────────────────────────────────────────────────
//
// The app supports the last stable (0.150.1), the current stable (0.151.0)
// and the unreleased mirror HEAD — see `docs/SUPPORTED_VERSIONS.md`. Each of
// these tests takes the modern branch
// where the API exists and, where it does not, checks that the refusal is
// one the app's classifier recognises — and that the Codex really is old
// enough for that to be the expected outcome.

#[test]
fn initialize_reports_a_parseable_cli_version() {
    let server = live!();
    // `<clientInfo.name>/<cli version> (<os>) …` — the originator we sent
    // leads, so the version is what identifies the CLI.
    assert!(
        server.user_agent.starts_with("pingex-e2e/"),
        "userAgent should lead with our client name: {:?}",
        server.user_agent
    );
    // A source build of the mirror reports the workspace's `0.0.0`, which
    // `version()` deliberately reads as "unreleased, newest".
    assert!(
        server.version().is_some() || server.user_agent.starts_with("pingex-e2e/0.0.0"),
        "could not read a version out of {:?}",
        server.user_agent
    );
}

/// `thread/queue/*` is a 0.149 API; 0.146 has no queue at all and the app
/// falls back to queueing in the window. A submission only stays queued while
/// a turn is running — on an idle thread the server promotes it at once — so
/// this queues behind a live turn, exactly as the app's composer does.
#[test]
fn server_side_queue_add_list_update_delete() {
    let server = live!();
    let thread_id = server.start_thread();
    let input = json!([{"type": "text", "text": "queued: reply with exactly QUEUED-OK"}]);

    // Probe support on the idle thread first, so 0.146 short-circuits before
    // we spend a turn.
    if let Err(error) = server.request(requests::queue_list(&thread_id, None)) {
        match error.unsupported(Feature::QUEUE) {
            Some(reason) => return server.expect_legacy(Feature::QUEUE, &reason),
            None => panic!("thread/queue/list rejected: {error}"),
        }
    }

    // Start a turn but do not wait for it: the queue holds submissions only
    // while one is in flight.
    let from = server.cursor();
    let running = server.call(requests::turn_start(
        &thread_id,
        text_input("Wait, then reply with exactly SLOW. First, think briefly."),
        low_effort(server),
    ));
    let running_id = running
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .expect("turn id")
        .to_string();
    server
        .wait_notification(from, "turn/started", TURN_TIMEOUT, |p| {
            p["threadId"] == thread_id
        })
        .expect("turn/started");

    server.call(requests::queue_add(
        &thread_id,
        input.clone(),
        "client-msg-1",
    ));
    let listed = server.call(requests::queue_list(&thread_id, None));
    let entries = listed["data"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        entries.len(),
        1,
        "one queued submission behind the running turn: {listed}"
    );
    let queued_id = entries[0]["id"].as_str().expect("queued id").to_string();
    server.call(requests::queue_update(&thread_id, &queued_id, input));
    server.call(requests::queue_reorder(
        &thread_id,
        std::slice::from_ref(&queued_id),
    ));
    server.call(requests::queue_delete(&thread_id, &queued_id));
    let listed = server.call(requests::queue_list(&thread_id, None));
    assert!(
        listed["data"]
            .as_array()
            .map(|d| d.is_empty())
            .unwrap_or(true),
        "queue empty after delete: {listed}"
    );

    server.call(requests::turn_interrupt(&thread_id, &running_id));
    server.call(requests::thread_delete(&thread_id));
}

/// `thread/revert` replaced `thread/rollback` in 0.149; the app tries revert
/// first and falls back to rollback only on the classified refusal.
#[test]
fn thread_revert_or_its_classified_absence() {
    let server = live!();
    let thread_id = server.start_thread();
    let first = server.run_turn(requests::turn_start(
        &thread_id,
        text_input("Reply with exactly ONE"),
        low_effort(server),
    ));
    assert_reply_contains(&first, "ONE", "first");
    let second = server.run_turn(requests::turn_start(
        &thread_id,
        text_input("Reply with exactly TWO"),
        low_effort(server),
    ));
    assert_reply_contains(&second, "TWO", "second");

    match server.request(requests::thread_revert(&thread_id, &second.turn_id)) {
        Ok(_) => {
            let read = server.call(requests::thread_read(&thread_id));
            let turns = read
                .pointer("/thread/turns")
                .and_then(Value::as_array)
                .map(Vec::len);
            assert_eq!(turns, Some(1), "one turn left after revert: {read}");
        }
        // 0.149 has the method but only serves threads in paginated history
        // mode; a legacy-history thread (the default) is refused with a
        // message the app also treats as "use rollback".
        Err(error) if error.message.contains(requests::REVERT_NEEDS_PAGINATED) => {
            assert!(
                server.at_least(0, 149),
                "{} refused revert for history mode but should not have it at all: {error}",
                server.user_agent
            );
            server.call(requests::thread_rollback(&thread_id, 1));
        }
        Err(error) => match error.unsupported(Feature::REVERT) {
            Some(reason) => {
                server.expect_legacy(Feature::REVERT, &reason);
                // The fallback the app takes on this Codex must still work.
                server.call(requests::thread_rollback(&thread_id, 1));
            }
            None => panic!("thread/revert rejected: {error}"),
        },
    }
    server.call(requests::thread_delete(&thread_id));
}

/// `project/*` (experimental, 0.149): the app imports each sidebar entry as
/// a server project and reads `projectId` back off `thread/list`.
#[test]
fn projects_import_assign_rename_delete() {
    let server = live!();
    let (thread_id, _) = server.persisted_thread();
    match server.request(requests::project_list(None, None)) {
        Ok(_) => {}
        Err(error) => match error.unsupported(Feature::PROJECTS) {
            Some(reason) => return server.expect_legacy(Feature::PROJECTS, &reason),
            None => panic!("project/list rejected: {error}"),
        },
    }
    let cwd = server.cwd.display().to_string();
    let imported = server.call(requests::project_import(
        "E2E project",
        std::slice::from_ref(&cwd),
        json!({"pingex.key": cwd}),
        std::slice::from_ref(&thread_id),
        &format!("e2e-{thread_id}"),
    ));
    let project_id = imported["project"]["id"]
        .as_str()
        .expect("project id")
        .to_string();
    assert_eq!(
        imported["project"]["metadata"]["pingex.key"], cwd,
        "metadata round-trips"
    );

    let listed = server.call(requests::thread_list(50, None, None, false));
    let thread = listed["data"]
        .as_array()
        .expect("data")
        .iter()
        .find(|thread| thread["id"] == thread_id)
        .cloned()
        .unwrap_or_else(|| panic!("{thread_id} not in thread/list: {listed}"));
    assert_eq!(
        thread["projectId"], project_id,
        "import filed the thread: {thread}"
    );

    // The assignment API the app uses for moves, both directions.
    server.call(requests::thread_set_project(&thread_id, None));
    server.call(requests::thread_set_project(&thread_id, Some(&project_id)));
    let read = server.call(requests::thread_read(&thread_id));
    assert_eq!(
        read["thread"]["projectId"], project_id,
        "re-assigned: {read}"
    );

    let renamed = server.call(requests::project_update(
        &project_id,
        Some("E2E renamed"),
        None,
    ));
    assert_eq!(renamed["project"]["name"], "E2E renamed");
    server.call(requests::project_delete(&project_id));
    // The sort keys are what the app always sends; releases ignore them so
    // far, and only the unreleased mirror reports `Project.recencyAt`.
    let projects = server.call(requests::project_list(None, Some(("recencyAt", "desc"))));
    if server.version().is_none() {
        assert!(
            projects["data"]
                .as_array()
                .expect("data")
                .iter()
                .all(|project| project.get("recencyAt").is_some()),
            "unreleased Codex lists Project.recencyAt: {projects}"
        );
    }
    assert!(
        !projects["data"]
            .as_array()
            .expect("data")
            .iter()
            .any(|project| project["id"] == project_id),
        "deleted project still listed: {projects}"
    );
    server.call(requests::thread_delete(&thread_id));
}

/// `threadSection/*` (stable in 0.149): the sidebar's section groups.
#[test]
fn thread_sections_create_move_update_delete() {
    let server = live!();
    let (thread_id, _) = server.persisted_thread();
    match server.request(requests::thread_section_list(None)) {
        Ok(_) => {}
        Err(error) => match error.unsupported(Feature::SECTIONS) {
            Some(reason) => return server.expect_legacy(Feature::SECTIONS, &reason),
            None => panic!("threadSection/list rejected: {error}"),
        },
    }
    let created = server.call(requests::thread_section_create(
        "E2E section",
        Some("#f59e0b"),
    ));
    let section_id = created["section"]["id"]
        .as_str()
        .expect("section id")
        .to_string();
    assert_eq!(created["section"]["appearance"]["color"], "#f59e0b");

    server.call(requests::thread_section_move(&thread_id, Some(&section_id)));
    let listed = server.call(requests::thread_list(50, None, None, false));
    let thread = listed["data"]
        .as_array()
        .expect("data")
        .iter()
        .find(|thread| thread["id"] == thread_id)
        .cloned()
        .unwrap_or_else(|| panic!("{thread_id} not in thread/list: {listed}"));
    assert_eq!(
        thread["section"]["id"], section_id,
        "thread moved into the section: {thread}"
    );

    let updated = server.call(requests::thread_section_update(
        &section_id,
        "E2E renamed",
        None,
    ));
    assert_eq!(updated["section"]["name"], "E2E renamed");
    assert!(
        updated["section"]["appearance"]["color"].is_null(),
        "colour cleared: {updated}"
    );
    server.call(requests::thread_section_move(&thread_id, None));
    let read = server.call(requests::thread_read(&thread_id));
    assert!(
        read["thread"]["section"].is_null(),
        "thread left the section: {read}"
    );

    server.call(requests::thread_section_delete(&section_id));
    let sections = server.call(requests::thread_section_list(None));
    assert!(
        !sections["data"]
            .as_array()
            .expect("data")
            .iter()
            .any(|section| section["id"] == section_id),
        "deleted section still listed: {sections}"
    );
    server.call(requests::thread_delete(&thread_id));
}

#[test]
fn thread_lifecycle_archive_unarchive_compact_rollback_delete() {
    let server = live!();
    let thread_id = server.start_thread();
    let outcome = server.run_turn(requests::turn_start(
        &thread_id,
        text_input("Reply with exactly ONE"),
        low_effort(server),
    ));
    assert_reply_contains(&outcome, "ONE", "first");
    let outcome = server.run_turn(requests::turn_start(
        &thread_id,
        text_input("Reply with exactly TWO"),
        low_effort(server),
    ));
    assert_reply_contains(&outcome, "TWO", "second");

    let rolled = server.call(requests::thread_rollback(&thread_id, 1));
    let turns = rolled
        .pointer("/thread/turns")
        .and_then(Value::as_array)
        .map(Vec::len);
    assert!(
        matches!(turns, None | Some(1)),
        "one turn left after rollback: {rolled}"
    );

    let from = server.cursor();
    server.call(requests::thread_compact(&thread_id));
    // Compaction runs as its own turn; wait for it to settle before archiving.
    let _ = server.wait_notification(from, "turn/completed", Duration::from_secs(90), |p| {
        p["threadId"] == thread_id
    });

    // The lifecycle notifications the sidebar refreshes on.
    let from = server.cursor();
    server.call(requests::thread_archive(&thread_id));
    let (from, _) = server
        .wait_notification(from, "thread/archived", Duration::from_secs(10), |p| {
            p["threadId"] == thread_id
        })
        .expect("thread/archived notification");
    let archived = server.call(requests::thread_list(50, None, None, true));
    let ids: Vec<_> = archived["data"]
        .as_array()
        .expect("data")
        .iter()
        .filter_map(|t| t["id"].as_str())
        .collect();
    assert!(
        ids.contains(&thread_id.as_str()),
        "archived list has it: {ids:?}"
    );
    server.call(requests::thread_unarchive(&thread_id));
    let (from, _) = server
        .wait_notification(from, "thread/unarchived", Duration::from_secs(10), |p| {
            p["threadId"] == thread_id
        })
        .expect("thread/unarchived notification");
    server.call(requests::thread_delete(&thread_id));
    server
        .wait_notification(from, "thread/deleted", Duration::from_secs(10), |p| {
            p["threadId"] == thread_id
        })
        .expect("thread/deleted notification");
}

/// `turn/settings/update` (unreleased): switch model or effort while a turn
/// runs. On a release build the refusal must be one the app classifies so the
/// composer falls back to "applies from the next turn".
#[test]
fn turn_settings_update_or_its_classified_absence() {
    let server = live!();
    let thread_id = server.start_thread();
    let from = server.cursor();
    let response = server.call(requests::turn_start(
        &thread_id,
        text_input("Count from 1 to 400, one number per line, with no other text."),
        low_effort(server),
    ));
    let turn_id = response
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .expect("turn id")
        .to_string();
    server
        .wait_notification(from, "item/agentMessage/delta", TURN_TIMEOUT, |params| {
            params["turnId"] == turn_id
        })
        .expect("model started streaming");
    let updated = server.request(requests::turn_settings_update(
        &thread_id,
        &turn_id,
        None,
        Some("medium"),
    ));
    server.call(requests::turn_interrupt(&thread_id, &turn_id));
    let _ = server.await_turn(from, &turn_id);
    match updated {
        Ok(response) => {
            let status = response["status"].as_str().unwrap_or("");
            assert!(
                matches!(status, "applied" | "targetUnavailable"),
                "turn/settings/update status: {response}"
            );
        }
        Err(error) => match error.unsupported(Feature::TURN_SETTINGS) {
            Some(reason) => server.expect_legacy(Feature::TURN_SETTINGS, &reason),
            None => panic!("turn/settings/update rejected: {error}"),
        },
    }
    server.call(requests::thread_delete(&thread_id));
}
