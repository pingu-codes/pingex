//! Feature flows that must survive the app being quit and relaunched: goals,
//! plan mode → implement, side questions and temporary worktrees. Each test
//! drives a real `codex app-server` with the app's own payloads, restarts it
//! mid-scenario ([`Server::restart`]) and reopens the Pingex database
//! ([`Server::open_db`]) to check what was recorded.

use crate::harness::{self, block_on, git, Server, TurnOutcome, TURN_TIMEOUT};
use crate::{assert_reply_contains, low_effort, text_input};
use pingex_app_lib::e2e::requests::{self, TurnOptions};
use pingex_app_lib::e2e::{
    add_side_question, delete_side_question, is_temp_worktree_path_under, read_side_questions,
    read_temp_worktrees, record_temp_worktree, temp_worktrees_root, worktree_parent_project,
    SideQuestion, MAX_TITLE_CHARS,
};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

/// How long a goal thread gets to start a continuation turn before we call it quiet.
const GOAL_SETTLE: Duration = Duration::from_secs(5);

fn goal(server: &Server, thread_id: &str) -> Value {
    server
        .call(requests::thread_goal_get(thread_id))
        .get("goal")
        .cloned()
        .unwrap_or(Value::Null)
}

fn goal_status(goal: &Value) -> &str {
    goal.get("status").and_then(Value::as_str).unwrap_or("")
}

fn goal_objective(goal: &Value) -> &str {
    goal.get("objective").and_then(Value::as_str).unwrap_or("")
}

/// The turns `thread/read` reports, so a restart provably lost nothing.
fn read_turns(server: &Server, thread_id: &str) -> Vec<Value> {
    server
        .call(requests::thread_read(thread_id))
        .pointer("/thread/turns")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn user_messages(turns: &[Value]) -> Vec<String> {
    turns
        .iter()
        .flat_map(|turn| turn["items"].as_array().cloned().unwrap_or_default())
        .filter(|item| item["type"] == "userMessage")
        .filter_map(|item| {
            item.pointer("/content")
                .and_then(Value::as_array)
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| part["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("")
                })
        })
        .collect()
}

fn mode_options(server: &Server, mode: &str) -> Option<TurnOptions> {
    let mut options = low_effort(server).unwrap_or_default();
    options.collaboration_mode = Some(json!({
        "mode": mode,
        "settings": {"model": server.model, "reasoning_effort": "low", "developer_instructions": null},
    }).into());
    Some(options)
}

fn canonical(path: &str) -> String {
    Path::new(path)
        .canonicalize()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.to_string())
}

// ---------------------------------------------------------------- goals

/// The `/goal` a user sets on a thread is the server's to keep: objective and
/// status (including a pause) must read back identically after a relaunch,
/// and clearing it must stick too.
#[test]
fn goal_survives_restart_pause_and_clear() {
    let server = live!();
    let thread_id = server.start_thread();
    let objective = "E2E goal: reply GOAL-OK whenever asked, nothing else";

    // Paused from the start so the goal never drives turns on its own.
    let set = server.call(requests::thread_goal_set(
        &thread_id,
        Some(objective),
        Some("paused"),
    ));
    assert_eq!(goal_objective(&set["goal"]), objective, "{set}");
    assert_eq!(goal_status(&set["goal"]), "paused", "{set}");

    let read = goal(server, &thread_id);
    assert_eq!(goal_objective(&read), objective);
    assert_eq!(goal_status(&read), "paused");

    // A normal turn on a goal thread still works and lands on this thread.
    let (outcome, _) = server.run_turn_observed(requests::turn_start(
        &thread_id,
        text_input("Reply with exactly GOAL-OK"),
        low_effort(server),
    ));
    assert_reply_contains(&outcome, "GOAL-OK", "turn on a paused-goal thread");
    let turns_before = read_turns(server, &thread_id).len();
    assert_eq!(turns_before, 1, "one turn before restart");

    // Relaunch: the goal must be there once the thread is re-attached.
    server.restart();
    server.call(requests::thread_resume(&thread_id));
    let after = goal(server, &thread_id);
    assert_eq!(
        goal_objective(&after),
        objective,
        "objective lost on restart: {after}"
    );
    assert_eq!(
        goal_status(&after),
        "paused",
        "pause lost on restart: {after}"
    );
    // A paused goal must not start running just because the app came back.
    let from = server.cursor();
    assert!(
        server
            .wait_notification(from, "turn/started", GOAL_SETTLE, |_| true)
            .is_none(),
        "paused goal started a turn on resume"
    );
    assert_eq!(
        read_turns(server, &thread_id).len(),
        turns_before,
        "turns lost on restart"
    );

    // Only the given field changes: pausing/unpausing keeps the objective.
    let resumed = server.call(requests::thread_goal_set(&thread_id, None, Some("active")));
    assert_eq!(goal_status(&resumed["goal"]), "active", "{resumed}");
    assert_eq!(goal_objective(&resumed["goal"]), objective, "{resumed}");
    let paused = server.call(requests::thread_goal_set(&thread_id, None, Some("paused")));
    assert_eq!(goal_status(&paused["goal"]), "paused", "{paused}");
    server.drain_turns(from, GOAL_SETTLE);

    // Clearing sticks across a relaunch too.
    server.call(requests::thread_goal_clear(&thread_id));
    assert!(goal(server, &thread_id).is_null(), "goal not cleared");
    server.restart();
    server.call(requests::thread_resume(&thread_id));
    assert!(
        goal(server, &thread_id).is_null(),
        "cleared goal came back after restart: {}",
        goal(server, &thread_id)
    );
}

/// "Start with goal": `/goal <objective>` on a brand-new thread sets the goal
/// before any turn ran (Codex materialises the thread from the goal), the
/// first turn then runs under whatever id Codex chooses, and pausing right
/// after leaves an active-then-paused goal that a relaunch reads back.
#[test]
fn start_with_goal_on_fresh_thread_then_pause_survives_restart() {
    let server = live!();
    let thread_id = server.start_thread();
    let objective = "E2E goal: when asked for the token reply GOAL-FRESH-OK; the goal is complete once you have replied";

    let set = server.call(requests::thread_goal_set(&thread_id, Some(objective), None));
    assert_eq!(goal_status(&set["goal"]), "active", "{set}");
    // Goal set on a fresh thread must not have started a turn by itself.
    let from = server.cursor();

    let (outcome, requested_id) = server.run_turn_observed(requests::turn_start(
        &thread_id,
        text_input("Reply with exactly GOAL-FRESH-OK"),
        low_effort(server),
    ));
    assert_reply_contains(
        &outcome,
        "GOAL-FRESH-OK",
        "first turn on a goal-first thread",
    );
    // Stop the goal before it can drive more turns; drain anything in flight.
    server.call(requests::thread_goal_set(&thread_id, None, Some("paused")));
    let turn_ids = server.drain_turns(from, GOAL_SETTLE);
    assert!(
        turn_ids.contains(&outcome.turn_id),
        "observed turn {} not among started turns {turn_ids:?} (turn/start returned {requested_id})",
        outcome.turn_id
    );

    let before = goal(server, &thread_id);
    assert_eq!(goal_status(&before), "paused", "{before}");
    let turns_before = read_turns(server, &thread_id);
    assert!(
        !turns_before.is_empty(),
        "goal-first thread has no turns in thread/read"
    );
    let messages_before = user_messages(&turns_before);
    assert_eq!(
        messages_before
            .iter()
            .filter(|m| m.contains("GOAL-FRESH-OK"))
            .count(),
        1,
        "user message duplicated or missing: {messages_before:?}"
    );

    server.restart();
    server.call(requests::thread_resume(&thread_id));
    let after = goal(server, &thread_id);
    assert_eq!(goal_objective(&after), objective, "{after}");
    assert_eq!(goal_status(&after), "paused", "{after}");
    let turns_after = read_turns(server, &thread_id);
    assert_eq!(
        user_messages(&turns_after),
        messages_before,
        "transcript changed across restart"
    );
    let from = server.cursor();
    assert!(
        server
            .wait_notification(from, "turn/started", GOAL_SETTLE, |_| true)
            .is_none(),
        "paused goal-first thread started a turn on resume"
    );
    server.call(requests::thread_goal_clear(&thread_id));
}

// ---------------------------------------------------------------- plans

const PLAN_FILE: &str = "PLAN_DONE.txt";
const PLAN_TOKEN: &str = "PLAN-IMPLEMENTED-7";

fn plan_request(file: &str) -> String {
    format!(
        "Plan (do not do it yet) how to create a file named {file} in the current directory whose only content is the line {PLAN_TOKEN}. Reply with the plan as a short numbered list."
    )
}

fn plan_text(outcome: &TurnOutcome) -> String {
    // Prefer a plan item when the server emitted one; the reply otherwise.
    outcome
        .items
        .iter()
        .rev()
        .find(|item| item["type"] == "plan")
        .and_then(|item| item["text"].as_str())
        .map(str::to_string)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| outcome.reply())
}

/// Plan mode, app quit, relaunch, "Implement the plan." — the composer's
/// exact handoff — must implement in the same thread with both turns on
/// record and no duplicates.
#[test]
fn plan_then_restart_then_implement_in_same_thread() {
    let server = live!();
    let file = server.cwd.join(PLAN_FILE);
    let _ = std::fs::remove_file(&file);
    let thread_id = server.start_thread();

    let planned = server.run_turn(requests::turn_start(
        &thread_id,
        text_input(&plan_request(PLAN_FILE)),
        mode_options(server, "plan"),
    ));
    assert_eq!(planned.status, "completed", "{:?}", planned.item_types());
    assert!(
        !plan_text(&planned).is_empty(),
        "no plan came back: {planned:?}"
    );
    assert!(
        !file.exists(),
        "plan mode wrote the file instead of planning: {:?}",
        planned.item_types()
    );

    server.restart();
    server.call(requests::thread_resume(&thread_id));
    let turns = read_turns(server, &thread_id);
    assert_eq!(turns.len(), 1, "planning turn lost on restart");

    // Composer.implementPlan(): "Implement the plan." with mode "default".
    let implemented = server.run_turn(requests::turn_start(
        &thread_id,
        text_input("Implement the plan."),
        mode_options(server, "default"),
    ));
    assert_eq!(
        implemented.status,
        "completed",
        "{:?}",
        implemented.item_types()
    );
    let content = std::fs::read_to_string(&file).unwrap_or_else(|error| {
        panic!(
            "{PLAN_FILE} not created by the implementation turn: {error}\nitems: {:?}\nreply: {}",
            implemented.item_types(),
            implemented.reply()
        )
    });
    assert!(content.contains(PLAN_TOKEN), "wrong content: {content:?}");

    let messages = user_messages(&read_turns(server, &thread_id));
    assert_eq!(
        messages.len(),
        2,
        "expected exactly two user turns: {messages:?}"
    );
    assert!(
        messages[0].contains("Plan (do not do it yet)"),
        "{messages:?}"
    );
    assert_eq!(messages[1], "Implement the plan.", "{messages:?}");

    // Another relaunch: the whole exchange is still on record.
    server.restart();
    server.call(requests::thread_resume(&thread_id));
    assert_eq!(user_messages(&read_turns(server, &thread_id)), messages);
    let _ = std::fs::remove_file(&file);
}

/// "Clear context & implement": the plan is carried into a brand-new thread
/// with the composer's fresh-plan prompt (golden copy in
/// `tests/fixtures/protocol/plan-handoff.json`) and implemented there; the
/// planning thread is left untouched — also after a relaunch.
#[test]
fn plan_then_implement_in_fresh_thread_after_restart() {
    let server = live!();
    let file_name = "PLAN_FRESH.txt";
    let file = server.cwd.join(file_name);
    let _ = std::fs::remove_file(&file);
    let planning_thread = server.start_thread();

    let planned = server.run_turn(requests::turn_start(
        &planning_thread,
        text_input(&plan_request(file_name)),
        mode_options(server, "plan"),
    ));
    let plan = plan_text(&planned);
    assert!(!plan.is_empty(), "no plan came back: {planned:?}");
    assert!(!file.exists(), "plan mode wrote the file");
    let planning_messages = user_messages(&read_turns(server, &planning_thread));

    server.restart();

    let prompt = fresh_plan_prompt(&plan);
    let fresh_thread = server.start_thread();
    assert_ne!(fresh_thread, planning_thread);
    let implemented = server.run_turn(requests::turn_start(
        &fresh_thread,
        text_input(&prompt),
        mode_options(server, "default"),
    ));
    let content = std::fs::read_to_string(&file).unwrap_or_else(|error| {
        panic!(
            "{file_name} not created in the fresh thread: {error}\nitems: {:?}\nreply: {}",
            implemented.item_types(),
            implemented.reply()
        )
    });
    assert!(content.contains(PLAN_TOKEN), "wrong content: {content:?}");

    // The fresh thread holds exactly the handoff prompt; the planning thread
    // gained nothing.
    let fresh_messages = user_messages(&read_turns(server, &fresh_thread));
    assert_eq!(fresh_messages, vec![prompt.clone()], "{fresh_messages:?}");
    server.call(requests::thread_resume(&planning_thread));
    assert_eq!(
        user_messages(&read_turns(server, &planning_thread)),
        planning_messages
    );
    let _ = std::fs::remove_file(&file);
}

fn fresh_plan_prompt(plan: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("protocol")
        .join("plan-handoff.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{} unreadable ({error}); run `deno task test src/lib/thread/planHandoff.test.ts` to generate it",
            path.display()
        )
    });
    let fixture: Value = serde_json::from_str(&text).expect("plan-handoff.json parses");
    fixture["freshPlanPrompt"]
        .as_str()
        .expect("freshPlanPrompt")
        .replace("${PLAN}", plan.trim())
}

// ---------------------------------------------------------- side questions

/// A side question is a real Codex thread plus a Pingex-side link to its
/// parent. The link (and the trimmed title) must be there when the database
/// is reopened after a relaunch, re-adding updates in place, and removing
/// the link leaves the thread itself intact.
#[test]
fn side_question_link_survives_restart() {
    let server = live!();
    let parent = server.start_thread();
    let side = server.start_thread();
    let long_title = "Side question about restarts ".repeat(6); // > MAX_TITLE_CHARS
    assert!(long_title.chars().count() > MAX_TITLE_CHARS);
    let title: String = long_title.trim().chars().take(MAX_TITLE_CHARS).collect();

    // What `threads::side_questions::add_side_question` stores.
    let db = server.open_db();
    block_on(add_side_question(
        &db,
        &SideQuestion {
            side_thread_id: side.clone(),
            parent_thread_id: parent.clone(),
            title: title.clone(),
            created_at: 1_700_000_000,
        },
    ))
    .expect("add side question");
    drop(db);

    // Both threads need a turn to exist on the Codex side at all: a thread
    // with no turns has no rollout and cannot be resumed after a relaunch.
    let outcome = server.run_turn(requests::turn_start(
        &parent,
        text_input("Reply with exactly PARENT-OK"),
        low_effort(server),
    ));
    assert_reply_contains(&outcome, "PARENT-OK", "turn on the parent thread");
    let outcome = server.run_turn(requests::turn_start(
        &side,
        text_input("Reply with exactly SIDE-OK"),
        low_effort(server),
    ));
    assert_reply_contains(&outcome, "SIDE-OK", "turn on the side thread");

    server.restart();
    let db = server.open_db();
    let recorded = block_on(read_side_questions(&db)).expect("read side questions");
    let mine: Vec<_> = recorded
        .iter()
        .filter(|q| q.side_thread_id == side)
        .collect();
    assert_eq!(
        mine.len(),
        1,
        "side question rows after restart: {recorded:?}"
    );
    assert_eq!(mine[0].parent_thread_id, parent);
    assert_eq!(mine[0].title, title);
    assert_eq!(mine[0].title.chars().count(), MAX_TITLE_CHARS);
    assert_eq!(mine[0].created_at, 1_700_000_000);
    // Both threads are still Codex threads with their history.
    server.call(requests::thread_resume(&side));
    assert_eq!(
        read_turns(server, &side).len(),
        1,
        "side thread lost its turn"
    );
    server.call(requests::thread_resume(&parent));
    assert_eq!(
        read_turns(server, &parent).len(),
        1,
        "parent thread lost its turn"
    );

    // Re-adding (rename) updates in place; created_at is kept.
    block_on(add_side_question(
        &db,
        &SideQuestion {
            side_thread_id: side.clone(),
            parent_thread_id: parent.clone(),
            title: "Renamed".into(),
            created_at: 1_800_000_000,
        },
    ))
    .expect("re-add side question");
    drop(db);
    server.restart();
    let db = server.open_db();
    let recorded = block_on(read_side_questions(&db)).expect("read side questions");
    let mine: Vec<_> = recorded
        .iter()
        .filter(|q| q.side_thread_id == side)
        .collect();
    assert_eq!(mine.len(), 1, "{recorded:?}");
    assert_eq!(mine[0].title, "Renamed");
    assert_eq!(
        mine[0].created_at, 1_700_000_000,
        "upsert must keep created_at"
    );

    // Removing the link only untracks it — the thread stays.
    block_on(delete_side_question(&db, &side)).expect("delete side question");
    drop(db);
    server.restart();
    let db = server.open_db();
    let recorded = block_on(read_side_questions(&db)).expect("read side questions");
    assert!(
        recorded.iter().all(|q| q.side_thread_id != side),
        "side question still linked after delete: {recorded:?}"
    );
    server.call(requests::thread_resume(&side));
    assert_eq!(
        read_turns(server, &side).len(),
        1,
        "deleting the link deleted the thread"
    );
}

// -------------------------------------------------------- temp worktrees

/// A temporary worktree lives under `<codex_home>/worktrees-tmp/<repo>/<name>`
/// and its link to the parent repository is recorded in the Pingex database,
/// so threads started in it stay under the repository across relaunches —
/// even after the worktree directory itself is gone. Turns in the worktree
/// run against the worktree, and `thread/read` keeps its real cwd (what
/// "Open in…" uses).
#[test]
fn temp_worktree_link_survives_restart_and_removal() {
    let server = live!();
    let repo = server.git_repo("e2e-repo");
    let repo_path = repo.display().to_string();
    let worktree = temp_worktrees_root(&server.codex_home)
        .join("e2e-repo")
        .join("e2e-wt");
    std::fs::create_dir_all(worktree.parent().unwrap()).expect("worktrees-tmp");
    let worktree_path = worktree.display().to_string();
    git(
        &repo,
        &["worktree", "add", "-q", "-b", "e2e-wt", &worktree_path],
    );
    assert!(is_temp_worktree_path_under(
        &server.codex_home,
        &worktree_path
    ));
    assert!(!is_temp_worktree_path_under(&server.codex_home, &repo_path));
    assert_eq!(
        worktree_parent_project(&worktree_path).map(|p| canonical(&p)),
        Some(canonical(&repo_path)),
        "git does not report the repository as the worktree's main tree"
    );

    // What `git_worktree_add` records for a temp worktree.
    let db = server.open_db();
    block_on(record_temp_worktree(&db, &worktree_path, &repo_path)).expect("record");
    drop(db);

    // A thread whose cwd is the worktree; its turn edits the worktree only.
    let started = server.call(requests::thread_start(&worktree_path, None, None, None));
    let thread_id = harness::thread_id_of(&started);
    let outcome = server.run_turn(requests::turn_start(
        &thread_id,
        text_input("Create a file named WT.txt in the current directory containing exactly WT-OK, then reply DONE"),
        low_effort(server),
    ));
    assert_eq!(outcome.status, "completed", "{:?}", outcome.item_types());
    assert!(
        worktree.join("WT.txt").is_file(),
        "turn did not write into the worktree; reply: {}",
        outcome.reply()
    );
    assert!(
        !repo.join("WT.txt").exists(),
        "turn wrote into the parent repo"
    );

    server.restart();
    let db = server.open_db();
    let links = block_on(read_temp_worktrees(&db)).expect("read temp worktrees");
    assert!(
        links.contains(&(worktree_path.clone(), repo_path.clone())),
        "temp worktree link lost on restart: {links:?}"
    );
    drop(db);
    server.call(requests::thread_resume(&thread_id));
    let read = server.call(requests::thread_read(&thread_id));
    assert_eq!(
        read.pointer("/thread/cwd")
            .and_then(Value::as_str)
            .map(canonical),
        Some(canonical(&worktree_path)),
        "thread cwd must stay the worktree, not the parent: {read}"
    );
    assert_eq!(read_turns(server, &thread_id).len(), 1);
    // The thread is found when listing by the worktree cwd.
    let listed = server.call(requests::thread_list(20, None, Some(&worktree_path), false));
    let ids: Vec<&str> = listed["data"]
        .as_array()
        .map(|d| d.iter().filter_map(|t| t["id"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        ids.contains(&thread_id.as_str()),
        "thread not listed for its worktree cwd: {listed}"
    );

    // Re-recording with a wrong parent then the right one corrects in place.
    let db = server.open_db();
    block_on(record_temp_worktree(&db, &worktree_path, "/nowhere")).expect("record");
    block_on(record_temp_worktree(&db, &worktree_path, &repo_path)).expect("record");
    let links = block_on(read_temp_worktrees(&db)).expect("read temp worktrees");
    let mine: Vec<_> = links.iter().filter(|(p, _)| p == &worktree_path).collect();
    assert_eq!(
        mine,
        vec![&(worktree_path.clone(), repo_path.clone())],
        "{links:?}"
    );
    drop(db);

    // Throw the worktree away: the link outlives it and the thread still opens.
    git(&repo, &["worktree", "remove", "--force", &worktree_path]);
    assert!(!worktree.exists());
    server.restart();
    let db = server.open_db();
    let links = block_on(read_temp_worktrees(&db)).expect("read temp worktrees");
    assert!(
        links.contains(&(worktree_path.clone(), repo_path.clone())),
        "link dropped with the directory: {links:?}"
    );
    assert!(is_temp_worktree_path_under(
        &server.codex_home,
        &worktree_path
    ));
    server.call(requests::thread_resume(&thread_id));
    assert_eq!(
        read_turns(server, &thread_id).len(),
        1,
        "thread lost with its worktree"
    );
    let _ = TURN_TIMEOUT;
}
