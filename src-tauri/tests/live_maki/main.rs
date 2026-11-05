//! The wire facts the ACP driver relies on, checked against a real maki
//! (`bun run test:e2e:maki`). Each test documents one assumption in
//! `src-tauri/src/acp/`; when maki changes, the failing test names it.

mod harness;

use harness::{kinds, reply_text, updates, Incoming};
use pingex_app_lib::e2e::{acp, acp_cancelled, acp_selected};
use serde_json::Value;
use std::time::{Duration, Instant};

/// Answer every permission request with `option`.
fn answer(option: &'static str) -> impl FnMut(&mut harness::Agent, &Value, &str, &Value) {
    move |agent, id, method, _| {
        assert_eq!(method, "session/request_permission");
        agent.respond(id, acp_selected(option));
    }
}

fn final_update(updates: &[Value], status: &str) -> Value {
    updates
        .iter()
        .rev()
        .find(|update| update["sessionUpdate"] == "tool_call_update" && update["status"] == status)
        .cloned()
        .unwrap_or_else(|| panic!("no {status} tool update in {updates:#?}"))
}

#[test]
fn initialize_offers_load_and_images_on_protocol_one() {
    let setup = live!();
    let mut agent = setup.spawn(&setup.work("init"), false);
    let info = acp::agent_info(&agent.initialize());
    assert_eq!(info.protocol_version, acp::PROTOCOL_VERSION);
    assert!(info.load_session);
    assert!(info.images);
}

#[test]
fn a_reply_streams_and_reports_usage_before_the_response() {
    let setup = live!();
    let mut agent = setup.spawn(&setup.work("reply"), false);
    let session = agent.new_session();
    let (result, seen) = agent.prompt(
        &session,
        "Reply with exactly the word: pong",
        |_, _, _, _| panic!("a plain reply asks nothing"),
    );
    assert_eq!(result.unwrap()["stopReason"], "end_turn");
    let updates = updates(&seen);
    assert!(reply_text(&updates).to_lowercase().contains("pong"));
    let usage = updates
        .iter()
        .find(|update| update["sessionUpdate"] == "usage_update")
        .expect("a usage update");
    assert!(usage["used"].as_u64().unwrap() > 0);
    assert!(usage["size"].as_u64().unwrap() > 0);
}

#[test]
fn an_allowed_command_runs_and_its_output_arrives_unfenced_in_raw_output() {
    let setup = live!();
    let mut agent = setup.spawn(&setup.work("allow"), false);
    let session = agent.new_session();
    let mut asked = 0;
    let (result, seen) = agent.prompt(
        &session,
        "Use the bash tool to run exactly: echo pingex-live. Then reply done.",
        |agent, id, method, params| {
            assert_eq!(method, "session/request_permission");
            assert_eq!(params["toolCall"]["kind"], "execute");
            assert!(params["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option["optionId"] == "allow_once"));
            asked += 1;
            agent.respond(id, acp_selected("allow_once"));
        },
    );
    assert_eq!(result.unwrap()["stopReason"], "end_turn");
    assert_eq!(asked, 1);
    let updates = updates(&seen);
    // A call opens pending, then names its input on `in_progress`.
    let opened = updates
        .iter()
        .find(|update| update["sessionUpdate"] == "tool_call")
        .unwrap();
    assert_eq!(opened["title"], "bash");
    assert!(updates
        .iter()
        .any(|update| update["status"] == "in_progress"
            && update["rawInput"]["command"].as_str().is_some()));
    let done = final_update(&updates, "completed");
    assert!(done["rawOutput"].as_str().unwrap().contains("pingex-live"));
}

#[test]
fn a_denied_command_fails_with_a_reason() {
    let setup = live!();
    let mut agent = setup.spawn(&setup.work("deny"), false);
    let session = agent.new_session();
    let (result, seen) = agent.prompt(
        &session,
        "Use the bash tool to run exactly: echo should-not-run. If it is denied, reply denied.",
        answer("reject_once"),
    );
    assert_eq!(result.unwrap()["stopReason"], "end_turn");
    let failed = final_update(&updates(&seen), "failed");
    assert!(failed["rawOutput"]
        .as_str()
        .unwrap()
        .contains("Permission denied"));
}

#[test]
fn a_write_inside_the_project_is_not_asked_about_and_names_its_input() {
    let setup = live!();
    let work = setup.work("write");
    let mut agent = setup.spawn(&work, false);
    let session = agent.new_session();
    let (result, seen) = agent.prompt(
        &session,
        "Use the write tool to create note.txt in the current directory containing the single line: hi. Do not use bash. Then reply done.",
        |_, _, _, _| panic!("writes inside the project are pre-allowed"),
    );
    assert_eq!(result.unwrap()["stopReason"], "end_turn");
    let updates = updates(&seen);
    let input = updates
        .iter()
        .find(|update| {
            update["status"] == "in_progress" && update["rawInput"]["content"].is_string()
        })
        .expect("the write's input");
    assert!(input["rawInput"]["path"]
        .as_str()
        .unwrap()
        .ends_with("note.txt"));
    assert!(work.join("note.txt").is_file());
}

#[test]
fn cancelling_with_a_permission_open_fails_the_call_and_ends_cancelled() {
    let setup = live!();
    let mut agent = setup.spawn(&setup.work("cancel"), false);
    let session = agent.new_session();
    let cancel_session = session.clone();
    let (result, seen) = agent.prompt(
        &session,
        "Use the bash tool to run exactly: sleep 20 && echo slept. Then reply done.",
        move |agent, id, _, _| {
            agent.notify("session/cancel", acp::cancel(&cancel_session));
            agent.respond(id, acp_cancelled());
        },
    );
    assert_eq!(result.unwrap()["stopReason"], "cancelled");
    final_update(&updates(&seen), "failed");
}

#[test]
fn load_replays_history_before_answering_and_locks_out_a_second_process() {
    let setup = live!();
    let work = setup.work("load");
    let session = {
        let mut first = setup.spawn(&work, false);
        let session = first.new_session();
        first
            .prompt(
                &session,
                "Reply with exactly the word: pong",
                |_, _, _, _| {},
            )
            .0
            .expect("first prompt");
        // Keep the session for the second process; the first must not delete it.
        first.sessions.clear();
        session
    };
    let mut second = setup.spawn(&work, false);
    second.sessions.push(session.clone());
    second.initialize();
    let cwd = work.to_string_lossy().into_owned();
    let (loaded, seen) = second.call(
        "session/load",
        acp::load_session(&session, &cwd),
        |_, _, _, _| {},
    );
    assert!(loaded.is_ok(), "{loaded:?}");
    let replayed = kinds(&updates(&seen));
    assert!(
        replayed.contains(&"user_message_chunk".to_string()),
        "{replayed:?}"
    );
    assert!(
        replayed.contains(&"agent_message_chunk".to_string()),
        "{replayed:?}"
    );

    let mut third = setup.spawn(&work, false);
    third.initialize();
    let (locked, _) = third.call(
        "session/load",
        acp::load_session(&session, &cwd),
        |_, _, _, _| {},
    );
    let error = locked.unwrap_err();
    assert_eq!(error["code"], -32000, "{error}");
    assert!(error["message"]
        .as_str()
        .unwrap()
        .contains("another maki process"));
}

#[test]
fn switching_mode_and_model_answer_in_place() {
    let setup = live!();
    let mut agent = setup.spawn(&setup.work("modes"), false);
    let session = agent.new_session();
    let (switched, seen) = agent.call(
        "session/set_mode",
        acp::set_mode(&session, "plan"),
        |_, _, _, _| {},
    );
    switched.unwrap();
    assert!(updates(&seen)
        .iter()
        .any(|update| update["sessionUpdate"] == "current_mode_update"
            && update["currentModeId"] == "plan"));
    if let Some(model) = setup.model.as_deref() {
        let (set, _) = agent.call(
            "session/set_config_option",
            acp::set_model(&session, model),
            |_, _, _, _| {},
        );
        set.unwrap();
    }
}

#[test]
fn closing_stdin_ends_the_process_promptly() {
    let setup = live!();
    let mut agent = setup.spawn(&setup.work("eof"), false);
    agent.initialize();
    let started = Instant::now();
    assert!(agent.close(Duration::from_secs(2)));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn without_prompts_a_command_runs_unasked() {
    let setup = live!();
    let mut agent = setup.spawn(&setup.work("yolo"), true);
    let session = agent.new_session();
    let (result, seen) = agent.prompt(
        &session,
        "Use the bash tool to run exactly: echo pingex-yolo. Then reply done.",
        |_, _, method, _| panic!("{method}: a process started without prompts asked"),
    );
    assert_eq!(result.unwrap()["stopReason"], "end_turn");
    let done = final_update(&updates(&seen), "completed");
    assert!(done["rawOutput"].as_str().unwrap().contains("pingex-yolo"));
    assert!(seen
        .iter()
        .all(|message| !matches!(message, Incoming::Request(..))));
}
