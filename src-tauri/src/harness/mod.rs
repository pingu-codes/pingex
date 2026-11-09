//! The harness-neutral event model (`features/13-harnesses.md`).
//!
//! A driver translates its wire protocol into [`HarnessEvent`]s and
//! [`HarnessRequest`]s. Today the Codex driver still speaks its own
//! notifications end to end; the Claude driver emits these and `project`
//! turns them into Codex-shaped notifications so the journal, the cache and
//! the transcript need no second code path. When the frontend reducer moves
//! onto `kind`-shaped items, the projection goes away and `harness:event`
//! becomes the only channel.

pub(crate) mod driver;
pub(crate) mod project;
pub(crate) mod sink;

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;

use crate::util::json::Json;

/// Which agent CLI a thread runs on.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum HarnessKind {
    Codex,
    Claude,
    Maki,
}

impl HarnessKind {
    pub(crate) const ALL: [HarnessKind; 3] =
        [HarnessKind::Codex, HarnessKind::Claude, HarnessKind::Maki];

    /// The tag stored in `harness_threads.harness` and sent over IPC.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            HarnessKind::Codex => "codex",
            HarnessKind::Claude => "claude",
            HarnessKind::Maki => "maki",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    /// The short name the harness goes by in messages.
    pub(crate) fn label(self) -> &'static str {
        match self {
            HarnessKind::Codex => "Codex",
            HarnessKind::Claude => "Claude",
            HarnessKind::Maki => "maki",
        }
    }

    /// The ACP agent behind this harness, for one that speaks ACP.
    pub(crate) fn acp_spec(self) -> Option<&'static dyn crate::acp::spec::AcpSpec> {
        match self {
            HarnessKind::Maki => Some(&crate::acp::maki::Maki),
            HarnessKind::Codex | HarnessKind::Claude => None,
        }
    }

    /// Every Host gets a Home of this harness when a project is added. The
    /// others are registered by hand, when the user wants them.
    pub(crate) fn required_per_host(self) -> bool {
        matches!(self, HarnessKind::Codex | HarnessKind::Claude)
    }
}

/// What the settings page and the harness picker need to know: whether a
/// harness's binary resolves, where, what it reports as its version, and
/// whether it is logged in.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HarnessStatus {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub config_dir: String,
    /// Whether the CLI is logged in under `config_dir`. `None` when the
    /// probe could not tell (no binary, or no way to check).
    pub logged_in: Option<bool>,
    /// The oldest CLI the driver can drive.
    pub protocol_floor: String,
    pub message: Option<String>,
}

/// Whether dotted `version` is older than `floor`, compared numerically.
pub(crate) fn version_below(version: &str, floor: &str) -> bool {
    let parse = |text: &str| -> Vec<u64> {
        text.split('.')
            .map(|part| part.trim().parse::<u64>().unwrap_or(0))
            .collect()
    };
    parse(version) < parse(floor)
}

/// Something a harness can do beyond the required core (`GLOSSARY.md`). This
/// is the declared half, fixed per harness; the frontend reads it (as the
/// generated `HARNESS_CAPABILITIES`) instead of branching on the kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Capability {
    /// A thread-level objective the harness pursues across turns (`/goal`).
    Goals,
    /// Editing a sent message in place: the thread is reverted to before it.
    Revert,
    /// Named permission modes the harness switches between itself, plan
    /// among them (Claude's default/acceptEdits/plan/bypassPermissions).
    PermissionModes,
    /// Sandbox and approval presets plus a per-turn collaboration mode
    /// (default or plan) sent with full model settings (Codex).
    CollaborationModes,
    /// Tool calls report completion while the turn continues, so finished
    /// work can fold before the turn ends.
    LiveToolResults,
    /// Summarising the thread to free context (`/compact`).
    Compact,
    /// Branching a thread into a new one, also behind "Move to worktree".
    Fork,
    /// A review turn over the working changes (`/review`).
    Review,
    /// Asking aside without disturbing the thread (`/btw`).
    SideQuestions,
    /// A turn sent with the read-only sandbox (`sandboxMode: "read-only"`)
    /// cannot change files without asking: Codex's sandbox, Claude's Default
    /// mode. Agents at No checkout need it, since they run in the user's clone.
    ReadOnlySandbox,
    /// Driving the user's own Chrome through the `pingex_browser` tools
    /// (Codex dynamic tools, `src-tauri/src/browser`).
    BrowserUse,
}

impl HarnessKind {
    pub(crate) fn capabilities(self) -> &'static [Capability] {
        use Capability::*;
        match self {
            HarnessKind::Codex => &[
                Goals,
                Revert,
                CollaborationModes,
                Compact,
                Fork,
                Review,
                SideQuestions,
                ReadOnlySandbox,
                BrowserUse,
            ],
            HarnessKind::Claude => &[
                PermissionModes,
                LiveToolResults,
                Compact,
                SideQuestions,
                ReadOnlySandbox,
            ],
            HarnessKind::Maki => &[PermissionModes, LiveToolResults],
        }
    }
}

/// What the frontend shows for a harness without asking it anything.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HarnessInfo {
    /// The product name, for menus and Home rows.
    pub label: &'static str,
    /// The short name, for prompts like "Claude wants to…".
    pub name: &'static str,
    pub default_binary: &'static str,
    /// Where the config lives, relative to the user's home directory.
    pub config_dir: &'static str,
    /// Whether a Home can point at another config directory. maki has no
    /// override short of the XDG variables, which its tools would inherit.
    pub config_dir_editable: bool,
    /// Registered by hand rather than on every Host.
    pub optional: bool,
    /// `read_home_status` can probe its install and login.
    pub probes_status: bool,
}

impl HarnessKind {
    pub(crate) fn info(self) -> HarnessInfo {
        let (label, binary, config_dir) = match self {
            HarnessKind::Codex => ("Codex", "codex", ".codex"),
            HarnessKind::Claude => ("Claude Code", "claude", ".claude"),
            HarnessKind::Maki => ("maki", "maki", ".config/maki"),
        };
        HarnessInfo {
            label,
            name: self.label(),
            default_binary: binary,
            config_dir,
            config_dir_editable: self.acp_spec().is_none(),
            optional: !self.required_per_host(),
            probes_status: self != HarnessKind::Codex,
        }
    }

    /// The modes the composer's permissions menu offers, default first.
    pub(crate) fn modes(self) -> Vec<ModeInfo> {
        if let Some(spec) = self.acp_spec() {
            return spec
                .modes()
                .iter()
                .map(|mode| ModeInfo {
                    id: mode.id,
                    label: mode.label,
                    description: mode.description,
                    plan: mode.plan,
                })
                .collect();
        }
        match self {
            HarnessKind::Claude => crate::claude::modes::mode_infos(),
            _ => Vec::new(),
        }
    }
}

/// One entry in a harness's permissions menu (`HARNESS_MODES`).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModeInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// The mode plans instead of acting.
    pub plan: bool,
}

/// [`HarnessKind::info`] for every harness, exported as `HARNESS_INFO`.
pub(crate) fn harness_info() -> BTreeMap<HarnessKind, HarnessInfo> {
    HarnessKind::ALL
        .into_iter()
        .map(|kind| (kind, kind.info()))
        .collect()
}

/// [`HarnessKind::modes`] for every harness that has any, exported as
/// `HARNESS_MODES`.
pub(crate) fn harness_modes() -> BTreeMap<HarnessKind, Vec<ModeInfo>> {
    HarnessKind::ALL
        .into_iter()
        .map(|kind| (kind, kind.modes()))
        .filter(|(_, modes)| !modes.is_empty())
        .collect()
}

/// [`HarnessKind::capabilities`] for every harness, exported as the
/// `HARNESS_CAPABILITIES` constant.
pub(crate) fn declared_capabilities() -> BTreeMap<HarnessKind, Vec<Capability>> {
    HarnessKind::ALL
        .into_iter()
        .map(|kind| (kind, kind.capabilities().to_vec()))
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SideRefusal {
    pub original_model: String,
    pub fallback_model: String,
    pub content: String,
}

pub(crate) struct SideAnswer {
    pub response: Option<String>,
    pub synthetic: bool,
    pub refusal: Option<SideRefusal>,
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
    /// Prompt tokens, cache hits included.
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    /// Prompt tokens written to the cache this turn (0 when the harness does
    /// not say).
    pub cache_write_input_tokens: u64,
    pub output_tokens: u64,
    /// The part of `output_tokens` that was thinking.
    pub reasoning_tokens: u64,
    pub context_window: Option<u64>,
    /// Harness-reported cost, cumulative for the session when the harness
    /// reports it that way (Claude does).
    pub cost_usd: Option<f64>,
    pub model: Option<String>,
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
    /// The harness switched collaboration mode on its own (a Claude plan
    /// approval moves the session out of plan mode). `mode` is `plan` or
    /// `default`, the same vocabulary `TurnOptions::collaboration_mode` uses.
    ModeChanged {
        mode: String,
    },
    PermissionModeChanged {
        mode: String,
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
