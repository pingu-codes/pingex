//! The harness-neutral event model (`features/13-harnesses.md`).
//!
//! A driver translates its wire protocol into [`HarnessEvent`]s and
//! [`HarnessRequest`]s. Today the Codex driver still speaks its own
//! notifications end to end; the Claude driver emits these and `project`
//! turns them into Codex-shaped notifications so the journal, the cache and
//! the transcript need no second code path. When the frontend reducer moves
//! onto `kind`-shaped items, the projection goes away and `harness:event`
//! becomes the only channel.

pub(crate) mod project;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::util::json::Json;

/// Which agent CLI a thread runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub(crate) enum HarnessKind {
    Codex,
    Claude,
}

/// ACP's tool kinds, used to pick a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    SwitchMode,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StopReason {
    EndTurn,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Cancelled,
    Error,
}

/// One piece of what a tool call shows: text, a diff, or terminal output.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(crate) enum ToolCallContent {
    Content {
        text: String,
    },
    Diff {
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    Terminal {
        text: String,
        exit_code: Option<i64>,
        cwd: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlanEntry {
    pub content: String,
    pub priority: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub context_window: Option<u64>,
    pub cost_usd: Option<f64>,
}

/// Everything a driver can say about a thread. Codex-only detail rides in
/// `ext` under the driver's own key and never on the root.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(crate) enum HarnessEvent {
    TurnStarted {
        turn_id: String,
        model: Option<String>,
    },
    TurnEnded {
        turn_id: String,
        stop_reason: StopReason,
        error: Option<String>,
        duration_ms: Option<u64>,
        usage: Option<TurnUsage>,
    },
    UserMessage {
        item_id: String,
        text: String,
    },
    AgentMessageChunk {
        item_id: String,
        text: String,
        /// The chunk closes the item; `text` is the final full text, or empty
        /// when everything already streamed.
        done: bool,
    },
    AgentThoughtChunk {
        item_id: String,
        text: String,
        done: bool,
    },
    ToolCall {
        item_id: String,
        title: String,
        kind: ToolKind,
        status: ToolCallStatus,
        name: String,
        content: Vec<ToolCallContent>,
        #[specta(type = Json)]
        raw_input: Json,
    },
    ToolCallUpdate {
        item_id: String,
        status: Option<ToolCallStatus>,
        content: Option<Vec<ToolCallContent>>,
        output_delta: Option<String>,
    },
    Plan {
        entries: Vec<PlanEntry>,
    },
    Compaction {
        item_id: String,
        trigger: String,
    },
    Notice {
        level: String,
        text: String,
    },
    RequestCancelled {
        request_id: i64,
    },
}

/// One option on a permission request, ACP-shaped.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PermissionOption {
    pub option_id: String,
    pub name: String,
    /// `allow_once` | `allow_always` | `reject_once` | `reject_always`
    pub kind: String,
}

/// A question the harness needs answered before it can go on.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(crate) enum HarnessRequest {
    Permission {
        title: String,
        description: Option<String>,
        kind: ToolKind,
        name: String,
        content: Vec<ToolCallContent>,
        options: Vec<PermissionOption>,
        /// Why the harness is asking (Claude `decision_reason`), ANSI stripped.
        reason: Option<String>,
        default_to_reject: bool,
        /// The command line, for an `execute` request.
        command: Option<String>,
        cwd: Option<String>,
        /// Codex-shaped `FileUpdateChange[]` for an `edit` request, so the
        /// existing diff card draws it.
        #[specta(type = Json)]
        changes: Json,
    },
    UserInput {
        #[specta(type = Json)]
        questions: Json,
    },
}

/// `harness:event` — one neutral event, tagged with the home and the thread.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "harness:event")]
#[serde(rename_all = "camelCase")]
pub(crate) struct HarnessEventEnvelope {
    pub codex_home: String,
    pub harness: HarnessKind,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub seq: u64,
    pub event: HarnessEvent,
}

/// `harness:request` — a request awaiting the user's answer. Answered through
/// the same `respond_*` commands as Codex requests; the id says who owns it.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "harness:request")]
#[serde(rename_all = "camelCase")]
pub(crate) struct HarnessRequestEnvelope {
    pub codex_home: String,
    pub harness: HarnessKind,
    pub request_id: i64,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub request: HarnessRequest,
}
