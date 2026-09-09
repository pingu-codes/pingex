//! Turning the usage notifications into `turn_usage` rows.
//!
//! Both harnesses report a thread's running total (`total`) and the last
//! request (`last`) on `thread/tokenUsage/updated`; Codex sends one per
//! response inside a turn and replays the total when a thread is resumed.
//! The ledger keeps, per thread, the last total it saw and the total the open
//! turn started from, so a turn's row is `total − turn_base` however many
//! requests it took, and anything the total exceeds the rows by outside a
//! turn is booked to the thread's opening row.
//!
//! Attribution needs the items in stream order, and the journal writes those
//! asynchronously, so the ledger takes its own copy of every completed item
//! as it passes and applies everything through one consumer per home. That
//! makes the order of items, usage and turn completions the order they
//! streamed in.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};
use tokio::sync::{mpsc, oneshot};
use turso::Database;

use super::estimate::{sketch_item, tokens_for_chars, Composition, Sketch};
use crate::harness::HarnessKind;
use crate::storage::{
    self, CategoryTokens, TurnUsageRow, UsageScope, UsageTokens, OPENING_TURN_ID,
};
use crate::util::host::Host;
use crate::util::time::unix_secs;

/// The composition of a thread's context as the ledger knows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ContextSnapshot {
    pub(crate) categories: CategoryTokens,
    pub(crate) context_tokens: Option<u64>,
    pub(crate) context_window: Option<u64>,
}

pub(crate) enum LedgerEvent {
    /// An `item/completed` payload (the `item` object).
    Item {
        thread_id: String,
        payload: Value,
    },
    /// A `thread/tokenUsage/updated`: its `tokenUsage` object, and whether the
    /// journal has a turn open on the thread.
    Usage {
        thread_id: String,
        turn_id: Option<String>,
        turn_open: bool,
        harness: HarnessKind,
        token_usage: Value,
    },
    TurnCompleted {
        thread_id: String,
        turn_id: String,
    },
    /// Forget a thread's in-memory state; the next event rebuilds it from
    /// what is stored (after a rollback dropped turns, say).
    Reset {
        thread_id: String,
    },
    /// Ask for the thread's context composition, loading it if needed.
    Snapshot {
        thread_id: String,
        reply: oneshot::Sender<Option<ContextSnapshot>>,
    },
}

/// Per-thread bookkeeping.
#[derive(Default)]
struct ThreadLedger {
    harness: String,
    /// The last running total seen (tokens, cost), or the stored sum before
    /// this process saw anything.
    seen: (UsageTokens, f64),
    /// The turn a row is being accumulated for, and the total it began at.
    open: Option<OpenTurn>,
    comp: Composition,
    context_tokens: Option<u64>,
    context_window: Option<u64>,
}

struct OpenTurn {
    turn_id: String,
    base: (UsageTokens, f64),
    /// The turn's own output so far (excluding reasoning), added to the
    /// composition once the turn ends.
    output: u64,
    /// The last row written, re-attributed when the turn completes.
    row: Option<TurnUsageRow>,
}

fn read_tokens(value: Option<&Value>) -> UsageTokens {
    let read = |key: &str| {
        value
            .and_then(|v| v.get(key))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    UsageTokens {
        input: read("inputTokens"),
        cached_input: read("cachedInputTokens"),
        cache_write_input: read("cacheWriteInputTokens"),
        output: read("outputTokens"),
        reasoning_output: read("reasoningOutputTokens"),
    }
}

fn read_cost(value: Option<&Value>) -> Option<f64> {
    value.and_then(|v| v.get("costUsd")).and_then(Value::as_f64)
}

/// Size a skill file in tokens, by its length on disk. A skill that cannot
/// be read counts for nothing; it must never stall a write.
fn skill_tokens(host: &Host, path: &str) -> u64 {
    std::fs::metadata(host.to_local(path))
        .map(|meta| tokens_for_chars(meta.len()))
        .unwrap_or(0)
}

/// The ledger's state and rules, driven by `apply`. Kept apart from the task
/// that runs it so the rules are testable against a database alone.
#[derive(Default)]
pub(crate) struct LedgerCore {
    threads: HashMap<String, ThreadLedger>,
}

impl LedgerCore {
    /// The thread's state, loaded from storage on first sight: its stored
    /// totals, and a composition replayed from its journaled items.
    async fn thread(
        &mut self,
        database: &Database,
        host: &Host,
        thread_id: &str,
    ) -> &mut ThreadLedger {
        if !self.threads.contains_key(thread_id) {
            let loaded = load_thread(database, host, thread_id).await;
            self.threads.insert(thread_id.to_string(), loaded);
        }
        self.threads
            .get_mut(thread_id)
            .expect("inserted immediately above")
    }

    pub(crate) async fn apply(&mut self, database: &Database, host: &Host, event: LedgerEvent) {
        match event {
            LedgerEvent::Item { thread_id, payload } => {
                let Some(sketch) = sketch_item(&payload) else {
                    return;
                };
                let thread = self.thread(database, host, &thread_id).await;
                thread.comp.absorb(&sketch, |path| skill_tokens(host, path));
            }
            LedgerEvent::Usage {
                thread_id,
                turn_id,
                turn_open,
                harness,
                token_usage,
            } => {
                let thread = self.thread(database, host, &thread_id).await;
                thread.harness = harness_name(harness).to_string();
                let total = read_tokens(token_usage.get("total"));
                let total_cost = read_cost(token_usage.get("total")).unwrap_or(0.0);
                let last = read_tokens(token_usage.get("last"));
                thread.context_tokens = Some(last.total())
                    .filter(|n| *n > 0)
                    .or(thread.context_tokens);
                if let Some(window) = token_usage
                    .get("modelContextWindow")
                    .and_then(Value::as_u64)
                {
                    thread.context_window = Some(window);
                }
                let model = token_usage
                    .get("model")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let turn_id = turn_id.filter(|_| turn_open);
                let Some(turn_id) = turn_id else {
                    // Outside a turn: what the total exceeds what is recorded
                    // by happened where this process could not see it.
                    let increment = total.saturating_sub(&thread.seen.0);
                    let cost_increment = (total_cost - thread.seen.1).max(0.0);
                    thread.seen = (total, total_cost.max(thread.seen.1));
                    if increment.is_zero() {
                        return;
                    }
                    let existing = storage::read_turn_usage(database, &thread_id, OPENING_TURN_ID)
                        .await
                        .ok()
                        .flatten();
                    let mut tokens = existing.as_ref().map(|row| row.tokens).unwrap_or_default();
                    tokens.add(&increment);
                    let cost_usd = match (
                        existing.as_ref().and_then(|row| row.cost_usd),
                        cost_increment > 0.0,
                    ) {
                        (Some(cost), _) => Some(cost + cost_increment),
                        (None, true) => Some(cost_increment),
                        (None, false) => None,
                    };
                    let row = TurnUsageRow {
                        thread_id: thread_id.clone(),
                        turn_id: OPENING_TURN_ID.to_string(),
                        harness: thread.harness.clone(),
                        model,
                        tokens,
                        cost_usd,
                        context_tokens: thread.context_tokens,
                        context_window: thread.context_window,
                        attribution: CategoryTokens {
                            unattributed: tokens.total(),
                            ..Default::default()
                        },
                        recorded_at: unix_secs(),
                    };
                    let _ = storage::record_turn_usage(database, &row).await;
                    return;
                };
                // A turn that never completed gets closed by the next one.
                if thread
                    .open
                    .as_ref()
                    .is_some_and(|open| open.turn_id != turn_id)
                {
                    finish_turn(thread);
                }
                if thread.open.is_none() {
                    // A total that went backwards means the harness process
                    // restarted its count; the turn then started from nothing.
                    let base = if total.input < thread.seen.0.input {
                        (UsageTokens::default(), 0.0)
                    } else {
                        thread.seen
                    };
                    thread.open = Some(OpenTurn {
                        turn_id: turn_id.clone(),
                        base,
                        output: 0,
                        row: None,
                    });
                }
                let base = thread
                    .open
                    .as_ref()
                    .map(|open| open.base)
                    .expect("opened above");
                let tokens = total.saturating_sub(&base.0);
                let cost_usd =
                    read_cost(token_usage.get("total")).map(|cost| (cost - base.1).max(0.0));
                thread.seen = (total, total_cost.max(thread.seen.1));
                if !thread.comp.is_seeded() {
                    if let Some(context) = thread.context_tokens {
                        thread.comp.seed(context.saturating_sub(tokens.output));
                    }
                }
                if let Some(context) = thread.context_tokens {
                    thread.comp.fit(context.saturating_sub(tokens.output));
                }
                let model = match model {
                    Some(model) => Some(model),
                    None => storage::read_turn_settings(database, &thread_id)
                        .await
                        .ok()
                        .and_then(|settings| {
                            settings
                                .into_iter()
                                .find(|s| s.turn_id == turn_id)
                                .and_then(|s| s.model)
                        }),
                };
                let row = TurnUsageRow {
                    thread_id: thread_id.clone(),
                    turn_id: turn_id.clone(),
                    harness: thread.harness.clone(),
                    model,
                    tokens,
                    cost_usd,
                    context_tokens: thread.context_tokens,
                    context_window: thread.context_window,
                    attribution: thread.comp.attribute(&tokens),
                    recorded_at: unix_secs(),
                };
                let _ = storage::record_turn_usage(database, &row).await;
                if let Some(open) = thread.open.as_mut() {
                    open.output = tokens.output.saturating_sub(tokens.reasoning_output);
                    open.row = Some(row);
                }
            }
            LedgerEvent::TurnCompleted { thread_id, turn_id } => {
                let Some(thread) = self.threads.get_mut(&thread_id) else {
                    return;
                };
                if thread
                    .open
                    .as_ref()
                    .is_none_or(|open| open.turn_id != turn_id)
                {
                    return;
                }
                // Items that completed after the last usage report (the final
                // reply, a trailing tool result) are in the composition now.
                if let Some(mut row) = thread.open.as_ref().and_then(|open| open.row.clone()) {
                    row.attribution = thread.comp.attribute(&row.tokens);
                    let _ = storage::record_turn_usage(database, &row).await;
                }
                finish_turn(thread);
            }
            LedgerEvent::Reset { thread_id } => {
                self.threads.remove(&thread_id);
            }
            LedgerEvent::Snapshot { thread_id, reply } => {
                let thread = self.thread(database, host, &thread_id).await;
                let snapshot =
                    (thread.context_tokens.is_some() || thread.comp.is_seeded()).then(|| {
                        ContextSnapshot {
                            categories: thread.comp.snapshot(thread.context_tokens),
                            context_tokens: thread.context_tokens,
                            context_window: thread.context_window,
                        }
                    });
                let _ = reply.send(snapshot);
            }
        }
    }
}

fn harness_name(kind: HarnessKind) -> &'static str {
    match kind {
        HarnessKind::Codex => "codex",
        HarnessKind::Claude => "claude",
    }
}

/// Close the open turn: its reply is now part of the context.
fn finish_turn(thread: &mut ThreadLedger) {
    if let Some(open) = thread.open.take() {
        thread.comp.add_output(open.output);
    }
}

/// Rebuild a thread's state from storage: the stored totals are what was
/// seen, the journaled items give the composition, and the latest row says
/// how big the context was, which seeds the system prompt.
async fn load_thread(database: &Database, host: &Host, thread_id: &str) -> ThreadLedger {
    let mut thread = ThreadLedger::default();
    let totals =
        storage::read_usage_totals(database, &UsageScope::Thread(thread_id.to_string()), None)
            .await
            .unwrap_or_default();
    thread.seen = (totals.tokens, totals.cost_usd.unwrap_or(0.0));
    let items = storage::read_thread_items(database, thread_id)
        .await
        .unwrap_or_default();
    for item in &items {
        if let Some(sketch) = sketch_item(&item.payload) {
            if matches!(sketch, Sketch::Compaction) {
                thread.comp.absorb(&sketch, |_| 0);
            } else {
                thread.comp.absorb(&sketch, |path| skill_tokens(host, path));
            }
        }
    }
    thread.comp.add_output(totals.attribution.output);
    if let Ok(Some(latest)) = storage::read_latest_turn_usage(database, thread_id).await {
        thread.harness = latest.harness;
        thread.context_tokens = latest.context_tokens;
        thread.context_window = latest.context_window;
        if let Some(context) = latest.context_tokens {
            thread.comp.seed(context);
            thread.comp.fit(context);
        }
    }
    thread
}

/// One consumer per home. Started on first use, since a home's context is
/// built before there is an app handle to resolve its database through.
#[derive(Default)]
pub(crate) struct UsageLedger {
    sender: OnceLock<mpsc::UnboundedSender<LedgerEvent>>,
}

impl UsageLedger {
    fn sender(&self, app: &AppHandle, home_key: &str) -> &mpsc::UnboundedSender<LedgerEvent> {
        self.sender.get_or_init(|| {
            let (sender, mut receiver) = mpsc::unbounded_channel::<LedgerEvent>();
            let (app, home_key) = (app.clone(), home_key.to_string());
            tauri::async_runtime::spawn(async move {
                let mut core = LedgerCore::default();
                while let Some(event) = receiver.recv().await {
                    let Some(context) = app
                        .try_state::<crate::AppState>()
                        .and_then(|state| state.context_for_home(&home_key))
                    else {
                        continue;
                    };
                    let (database, host) = (context.database(), context.host());
                    core.apply(&database, &host, event).await;
                }
            });
            sender
        })
    }

    pub(crate) fn send(&self, app: &AppHandle, home_key: &str, event: LedgerEvent) {
        let _ = self.sender(app, home_key).send(event);
    }

    pub(crate) async fn snapshot(
        &self,
        app: &AppHandle,
        home_key: &str,
        thread_id: &str,
    ) -> Option<ContextSnapshot> {
        let (reply, receiver) = oneshot::channel();
        self.send(
            app,
            home_key,
            LedgerEvent::Snapshot {
                thread_id: thread_id.to_string(),
                reply,
            },
        );
        receiver.await.ok().flatten()
    }
}

/// Hand an event to the ledger of `home_key`, from wherever a notification
/// is observed.
pub(crate) fn send(app: &AppHandle, home_key: &str, event: LedgerEvent) {
    if let Some(context) = app
        .try_state::<crate::AppState>()
        .and_then(|state| state.context_for_home(home_key))
    {
        context.usage.send(app, home_key, event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn database() -> Database {
        let directory = tempfile::tempdir().unwrap();
        let database = crate::storage::open(directory.path()).await.unwrap();
        std::mem::forget(directory);
        database
    }

    fn usage(
        turn: Option<&str>,
        open: bool,
        total: (u64, u64, u64),
        last_total: u64,
    ) -> LedgerEvent {
        LedgerEvent::Usage {
            thread_id: "t1".into(),
            turn_id: turn.map(str::to_string),
            turn_open: open,
            harness: HarnessKind::Codex,
            token_usage: json!({
                "total": {
                    "totalTokens": total.0 + total.1,
                    "inputTokens": total.0,
                    "cachedInputTokens": 0,
                    "outputTokens": total.1,
                    "reasoningOutputTokens": total.2,
                },
                "last": {"totalTokens": last_total, "inputTokens": last_total, "outputTokens": 0},
                "modelContextWindow": 200000,
            }),
        }
    }

    fn item(payload: Value) -> LedgerEvent {
        LedgerEvent::Item {
            thread_id: "t1".into(),
            payload,
        }
    }

    fn completed(turn: &str) -> LedgerEvent {
        LedgerEvent::TurnCompleted {
            thread_id: "t1".into(),
            turn_id: turn.into(),
        }
    }

    async fn rows(database: &Database) -> Vec<TurnUsageRow> {
        let mut out = Vec::new();
        for turn in [OPENING_TURN_ID, "turn-1", "turn-2"] {
            if let Some(row) = storage::read_turn_usage(database, "t1", turn)
                .await
                .unwrap()
            {
                out.push(row);
            }
        }
        out
    }

    #[tokio::test]
    async fn a_turn_of_several_requests_ends_as_one_row_of_the_turn_sum() {
        let database = database().await;
        let mut core = LedgerCore::default();
        let host = Host::Native;
        core.apply(&database, &host, item(json!({"type": "userMessage", "id": "u", "content": [{"type": "text", "text": "x".repeat(400)}]}))).await;
        core.apply(
            &database,
            &host,
            usage(Some("turn-1"), true, (1000, 50, 10), 1050),
        )
        .await;
        core.apply(&database, &host, item(json!({"type": "commandExecution", "id": "c", "command": "ls", "aggregatedOutput": "y".repeat(800)}))).await;
        core.apply(
            &database,
            &host,
            usage(Some("turn-1"), true, (2500, 120, 30), 1620),
        )
        .await;
        core.apply(&database, &host, completed("turn-1")).await;
        let stored = rows(&database).await;
        assert_eq!(stored.len(), 1);
        let row = &stored[0];
        assert_eq!(row.turn_id, "turn-1");
        assert_eq!(row.tokens.input, 2500);
        assert_eq!(row.tokens.output, 120);
        assert_eq!(row.tokens.reasoning_output, 30);
        assert_eq!(row.harness, "codex");
        assert_eq!(row.context_tokens, Some(1620));
        assert_eq!(row.attribution.total(), 2620);
        assert_eq!(row.attribution.reasoning, 30);
        assert_eq!(row.attribution.output, 90);
        assert!(
            row.attribution.system > 0,
            "system prompt derived from the first context"
        );
        assert!(
            row.attribution.tool > 0,
            "the command completed before the turn did"
        );
        assert_eq!(row.attribution.unattributed, 0);
    }

    #[tokio::test]
    async fn a_second_turn_books_only_its_own_delta() {
        let database = database().await;
        let mut core = LedgerCore::default();
        let host = Host::Native;
        core.apply(
            &database,
            &host,
            usage(Some("turn-1"), true, (1000, 50, 0), 1050),
        )
        .await;
        core.apply(&database, &host, completed("turn-1")).await;
        core.apply(
            &database,
            &host,
            usage(Some("turn-2"), true, (1800, 90, 0), 1890),
        )
        .await;
        core.apply(&database, &host, completed("turn-2")).await;
        let stored = rows(&database).await;
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[1].turn_id, "turn-2");
        assert_eq!(stored[1].tokens.input, 800);
        assert_eq!(stored[1].tokens.output, 40);
        // The first reply is in context for the second turn.
        assert!(stored[1].attribution.output >= 40);
    }

    #[tokio::test]
    async fn a_replayed_total_outside_a_turn_becomes_the_opening_row() {
        let database = database().await;
        let mut core = LedgerCore::default();
        let host = Host::Native;
        core.apply(&database, &host, usage(None, false, (5000, 400, 100), 5400))
            .await;
        let stored = rows(&database).await;
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].turn_id, OPENING_TURN_ID);
        assert_eq!(stored[0].tokens.input, 5000);
        assert_eq!(stored[0].attribution.unattributed, 5400);
        // Replaying the same total again adds nothing.
        core.apply(&database, &host, usage(None, false, (5000, 400, 100), 5400))
            .await;
        assert_eq!(rows(&database).await[0].tokens.input, 5000);
        // A later untracked increase grows the same row.
        core.apply(&database, &host, usage(None, false, (6000, 400, 100), 5400))
            .await;
        assert_eq!(rows(&database).await[0].tokens.input, 6000);
    }

    #[tokio::test]
    async fn a_replay_equal_to_the_stored_sum_writes_nothing_after_a_restart() {
        let database = database().await;
        let host = Host::Native;
        let mut first = LedgerCore::default();
        first
            .apply(
                &database,
                &host,
                usage(Some("turn-1"), true, (1000, 50, 0), 1050),
            )
            .await;
        first.apply(&database, &host, completed("turn-1")).await;
        // A new process resumes the thread and Codex replays the total.
        let mut second = LedgerCore::default();
        second
            .apply(&database, &host, usage(None, false, (1000, 50, 0), 1050))
            .await;
        assert_eq!(rows(&database).await.len(), 1);
        // The next turn is a delta from the stored sum.
        second
            .apply(
                &database,
                &host,
                usage(Some("turn-2"), true, (1300, 70, 0), 1370),
            )
            .await;
        second.apply(&database, &host, completed("turn-2")).await;
        let stored = rows(&database).await;
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[1].tokens.input, 300);
    }

    #[tokio::test]
    async fn a_total_that_went_backwards_starts_the_turn_from_nothing() {
        let database = database().await;
        let mut core = LedgerCore::default();
        let host = Host::Native;
        core.apply(
            &database,
            &host,
            usage(Some("turn-1"), true, (5000, 200, 0), 5200),
        )
        .await;
        core.apply(&database, &host, completed("turn-1")).await;
        // The Claude process restarted: its projector counts from zero again.
        core.apply(
            &database,
            &host,
            usage(Some("turn-2"), true, (700, 30, 0), 730),
        )
        .await;
        core.apply(&database, &host, completed("turn-2")).await;
        let stored = rows(&database).await;
        assert_eq!(stored[1].tokens.input, 700);
        assert_eq!(stored[1].tokens.output, 30);
    }

    #[tokio::test]
    async fn a_cold_ledger_rebuilds_the_composition_from_the_journal() {
        let database = database().await;
        let host = Host::Native;
        storage::record_thread_item(
            &database,
            "t1",
            "turn-1",
            "u",
            &json!({"type": "userMessage", "id": "u", "content": [{"type": "text", "text": "q".repeat(4000)}]}),
            None,
        )
        .await
        .unwrap();
        let mut first = LedgerCore::default();
        first
            .apply(
                &database,
                &host,
                usage(Some("turn-1"), true, (13000, 100, 0), 13100),
            )
            .await;
        first.apply(&database, &host, completed("turn-1")).await;

        let mut second = LedgerCore::default();
        let (reply, receiver) = oneshot::channel();
        second
            .apply(
                &database,
                &host,
                LedgerEvent::Snapshot {
                    thread_id: "t1".into(),
                    reply,
                },
            )
            .await;
        let snapshot = receiver
            .await
            .unwrap()
            .expect("a thread with rows has a snapshot");
        assert_eq!(snapshot.context_tokens, Some(13100));
        assert_eq!(snapshot.context_window, Some(200000));
        assert_eq!(snapshot.categories.user, 1000);
        assert_eq!(snapshot.categories.output, 100);
        assert_eq!(snapshot.categories.system, 12000);
        assert_eq!(snapshot.categories.unattributed, 0);
    }

    #[tokio::test]
    async fn an_unknown_thread_has_no_snapshot() {
        let database = database().await;
        let mut core = LedgerCore::default();
        let (reply, receiver) = oneshot::channel();
        core.apply(
            &database,
            &Host::Native,
            LedgerEvent::Snapshot {
                thread_id: "nope".into(),
                reply,
            },
        )
        .await;
        assert_eq!(receiver.await.unwrap(), None);
    }
}
