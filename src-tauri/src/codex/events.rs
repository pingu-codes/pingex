//! The typed shape of everything the app-server pushes at the frontend.
//!
//! Codex notifications and server requests arrive as `{ method, params }`
//! JSON-RPC lines. They are decoded here — once — into adjacently tagged
//! enums whose serde form is byte-identical to the wire, and exported by
//! tauri-specta as discriminated unions, so the frontend switches on a checked
//! `method` literal instead of re-parsing strings in every subscriber.
//!
//! Two rules keep this from being a liability against a protocol that moves
//! faster than the app:
//!
//! * Every field is optional. A missing field never fails a decode and is
//!   forwarded as `null` (specta's unified mode cannot express omission), which
//!   the frontend reads exactly as it read absence.
//! * A method the enum does not know — or a known one whose scalar has changed
//!   type — is forwarded as [`CodexNotification::Unknown`] rather than dropped,
//!   so the raw payload still reaches the frontend and the case is logged.
//!
//! Structured payloads (`item`, `turn`, `thread`, …) stay [`Json`]: the
//! frontend narrows them with its own hand-written types, and modelling them
//! here would just be a second copy of the protocol to keep in step.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;

use crate::util::json::Json;

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

/// A notification from the app-server, tagged by its JSON-RPC method.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "method", content = "params")]
pub(crate) enum CodexNotification {
    #[serde(rename = "thread/started")]
    ThreadStarted(ThreadStartedParams),
    #[serde(rename = "thread/status/changed")]
    ThreadStatusChanged(ThreadStatusChangedParams),
    #[serde(rename = "thread/name/updated")]
    ThreadNameUpdated(ThreadNameUpdatedParams),
    #[serde(rename = "thread/project/updated")]
    ThreadProjectUpdated(ThreadProjectUpdatedParams),
    #[serde(rename = "thread/goal/updated")]
    ThreadGoalUpdated(ThreadGoalUpdatedParams),
    #[serde(rename = "thread/tokenUsage/updated")]
    ThreadTokenUsageUpdated(ThreadTokenUsageUpdatedParams),
    #[serde(rename = "thread/settings/updated")]
    ThreadSettingsUpdated(ThreadSettingsUpdatedParams),
    #[serde(rename = "thread/queue/changed")]
    ThreadQueueChanged(ThreadParams),
    #[serde(rename = "thread/reverted")]
    ThreadReverted(ThreadParams),
    #[serde(rename = "thread/compacted")]
    ThreadCompacted(ThreadTurnParams),

    #[serde(rename = "turn/started")]
    TurnStarted(TurnParams),
    #[serde(rename = "turn/completed")]
    TurnCompleted(TurnParams),
    #[serde(rename = "turn/plan/updated")]
    TurnPlanUpdated(TurnPlanUpdatedParams),

    #[serde(rename = "item/started")]
    ItemStarted(ItemParams),
    #[serde(rename = "item/updated")]
    ItemUpdated(ItemParams),
    #[serde(rename = "item/completed")]
    ItemCompleted(ItemParams),
    #[serde(rename = "item/agentMessage/delta")]
    AgentMessageDelta(DeltaParams),
    #[serde(rename = "item/plan/delta")]
    PlanDelta(DeltaParams),
    #[serde(rename = "item/reasoning/summaryPartAdded")]
    ReasoningSummaryPartAdded(SummaryPartAddedParams),
    #[serde(rename = "item/reasoning/summaryTextDelta")]
    ReasoningSummaryTextDelta(SummaryTextDeltaParams),
    #[serde(rename = "item/reasoning/textDelta")]
    ReasoningTextDelta(ReasoningTextDeltaParams),
    #[serde(rename = "item/commandExecution/outputDelta")]
    CommandExecutionOutputDelta(DeltaParams),
    #[serde(rename = "item/commandExecution/terminalInteraction")]
    CommandExecutionTerminalInteraction(TerminalInteractionParams),
    #[serde(rename = "item/fileChange/patchUpdated")]
    FileChangePatchUpdated(PatchUpdatedParams),
    #[serde(rename = "item/mcpToolCall/progress")]
    McpToolCallProgress(McpToolCallProgressParams),
    #[serde(rename = "item/autoApprovalReview/completed")]
    AutoApprovalReviewCompleted(AutoApprovalReviewCompletedParams),

    #[serde(rename = "model/rerouted")]
    ModelRerouted(ModelReroutedParams),
    #[serde(rename = "model/safetyBuffering/updated")]
    ModelSafetyBufferingUpdated(SafetyBufferingUpdatedParams),
    #[serde(rename = "hook/completed")]
    HookCompleted(HookCompletedParams),

    #[serde(rename = "error")]
    Error(ErrorParams),
    #[serde(rename = "warning")]
    Warning(NoticeParams),
    #[serde(rename = "guardianWarning")]
    GuardianWarning(NoticeParams),
    #[serde(rename = "deprecationNotice")]
    DeprecationNotice(NoticeParams),
    #[serde(rename = "configWarning")]
    ConfigWarning(NoticeParams),

    #[serde(rename = "serverRequest/resolved")]
    ServerRequestResolved(ServerRequestResolvedParams),
    #[serde(rename = "account/rateLimits/updated")]
    AccountRateLimitsUpdated(RateLimitsUpdatedParams),
    #[serde(rename = "mcpServer/startupStatus/updated")]
    McpServerStartupStatusUpdated(McpServerParams),
    #[serde(rename = "mcpServer/oauthLogin/completed")]
    McpServerOauthLoginCompleted(McpServerParams),
    #[serde(rename = "project/changed")]
    ProjectChanged(ProjectChangedParams),
    #[serde(rename = "remoteControl/status/changed")]
    RemoteControlStatusChanged(RemoteControlStatusChangedParams),

    /// A method this build does not model, carrying the raw line so nothing
    /// is lost. Tagged `unknown` — no Codex method is spelled that way.
    #[serde(rename = "unknown")]
    Unknown(UnknownNotification),
}

impl CodexNotification {
    /// Decode one notification; never fails. See the module docs for why an
    /// undecodable line becomes [`Self::Unknown`] rather than an error.
    pub(crate) fn decode(method: &str, params: &Value) -> Self {
        // Adjacent tagging needs the content key present; a params-less
        // notification must not degrade to Unknown over that.
        let params = if params.is_null() {
            json!({})
        } else {
            params.clone()
        };
        match serde_json::from_value(json!({ "method": method, "params": params })) {
            Ok(event) => event,
            Err(error) => {
                if KNOWN_METHODS.contains(&method) {
                    eprintln!("codex notification {method} did not decode ({error}); forwarding as unknown");
                }
                Self::Unknown(UnknownNotification {
                    method: method.to_string(),
                    params: Json(params),
                })
            }
        }
    }
}

/// Every method the enum models, for telling "new to us" apart from "changed
/// under us" in the decode log line.
const KNOWN_METHODS: &[&str] = &[
    "thread/started",
    "thread/status/changed",
    "thread/name/updated",
    "thread/project/updated",
    "thread/goal/updated",
    "thread/tokenUsage/updated",
    "thread/settings/updated",
    "thread/queue/changed",
    "thread/reverted",
    "thread/compacted",
    "turn/started",
    "turn/completed",
    "turn/plan/updated",
    "item/started",
    "item/updated",
    "item/completed",
    "item/agentMessage/delta",
    "item/plan/delta",
    "item/reasoning/summaryPartAdded",
    "item/reasoning/summaryTextDelta",
    "item/reasoning/textDelta",
    "item/commandExecution/outputDelta",
    "item/commandExecution/terminalInteraction",
    "item/fileChange/patchUpdated",
    "item/mcpToolCall/progress",
    "item/autoApprovalReview/completed",
    "model/rerouted",
    "model/safetyBuffering/updated",
    "hook/completed",
    "error",
    "warning",
    "guardianWarning",
    "deprecationNotice",
    "configWarning",
    "serverRequest/resolved",
    "account/rateLimits/updated",
    "mcpServer/startupStatus/updated",
    "mcpServer/oauthLogin/completed",
    "project/changed",
    "remoteControl/status/changed",
];

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnknownNotification {
    /// The wire method, since the tag on the envelope is `unknown`.
    pub method: String,
    #[specta(type = Json)]
    pub params: Json,
}

macro_rules! params {
    ($(#[$meta:meta])* $name:ident { $($(#[$fmeta:meta])* $field:ident : $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
        #[serde(rename_all = "camelCase", default)]
        pub(crate) struct $name {
            $($(#[$fmeta])* pub $field: $ty,)*
        }
    };
}

params!(ThreadParams { thread_id: Option<String> });
params!(ThreadTurnParams { thread_id: Option<String>, turn_id: Option<String> });
params!(ThreadStartedParams { #[specta(type = Json)] thread: Option<Json> });
params!(ThreadStatusChangedParams { thread_id: Option<String>, #[specta(type = Json)] status: Option<Json> });
params!(ThreadNameUpdatedParams { thread_id: Option<String>, thread_name: Option<String> });
params!(ThreadProjectUpdatedParams { thread_id: Option<String>, project_id: Option<String> });
params!(ThreadGoalUpdatedParams { thread_id: Option<String>, turn_id: Option<String>, #[specta(type = Json)] goal: Option<Json> });
params!(ThreadTokenUsageUpdatedParams { thread_id: Option<String>, turn_id: Option<String>, #[specta(type = Json)] token_usage: Option<Json> });
params!(ThreadSettingsUpdatedParams { thread_id: Option<String>, #[specta(type = Json)] thread_settings: Option<Json> });

params!(TurnParams { thread_id: Option<String>, #[specta(type = Json)] turn: Option<Json> });
params!(TurnPlanUpdatedParams {
    thread_id: Option<String>,
    turn_id: Option<String>,
    explanation: Option<String>,
    #[specta(type = Json)] plan: Option<Json>,
});

params!(ItemParams { thread_id: Option<String>, turn_id: Option<String>, #[specta(type = Json)] item: Option<Json> });
params!(DeltaParams { thread_id: Option<String>, turn_id: Option<String>, item_id: Option<String>, delta: Option<String> });
params!(SummaryPartAddedParams { thread_id: Option<String>, turn_id: Option<String>, item_id: Option<String>, summary_index: Option<i64> });
params!(SummaryTextDeltaParams {
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    delta: Option<String>,
    summary_index: Option<i64>,
});
params!(ReasoningTextDeltaParams {
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    delta: Option<String>,
    content_index: Option<i64>,
});
params!(TerminalInteractionParams {
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    process_id: Option<String>,
    stdin: Option<String>,
});
params!(PatchUpdatedParams { thread_id: Option<String>, turn_id: Option<String>, item_id: Option<String>, #[specta(type = Json)] changes: Option<Json> });
params!(McpToolCallProgressParams { thread_id: Option<String>, turn_id: Option<String>, item_id: Option<String>, message: Option<String> });
params!(AutoApprovalReviewCompletedParams {
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    target_item_id: Option<String>,
    #[specta(type = Json)] review: Option<Json>,
});

params!(ModelReroutedParams { thread_id: Option<String>, turn_id: Option<String>, from_model: Option<String>, to_model: Option<String> });
params!(SafetyBufferingUpdatedParams { thread_id: Option<String>, turn_id: Option<String>, show_buffering_ui: Option<bool> });
params!(HookCompletedParams { thread_id: Option<String>, turn_id: Option<String>, #[specta(type = Json)] run: Option<Json> });

params!(ErrorParams { thread_id: Option<String>, turn_id: Option<String>, will_retry: Option<bool>, #[specta(type = Json)] error: Option<Json> });
params!(
    /// Codex spells the user-facing line differently per notification
    /// (`message`, or `summary` + `details` on a deprecation); the frontend
    /// tries each rather than guessing per method.
    NoticeParams {
        thread_id: Option<String>,
        message: Option<String>,
        summary: Option<String>,
        warning: Option<String>,
        details: Option<String>,
        additional_details: Option<String>,
    }
);

params!(ServerRequestResolvedParams { thread_id: Option<String>, request_id: Option<i64> });
params!(RateLimitsUpdatedParams { #[specta(type = Json)] rate_limits: Option<Json> });
params!(McpServerParams { thread_id: Option<String>, name: Option<String>, server_name: Option<String> });
params!(ProjectChangedParams { project_id: Option<String>, change_type: Option<String> });
params!(RemoteControlStatusChangedParams { #[specta(type = Json)] status: Option<Json>, server_name: Option<String> });

// ---------------------------------------------------------------------------
// Server requests
// ---------------------------------------------------------------------------

/// A request the app-server needs the user to answer, tagged by method.
/// Only the ones Rust does not answer itself reach the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "method", content = "params")]
pub(crate) enum ServerRequest {
    #[serde(rename = "item/commandExecution/requestApproval")]
    CommandExecutionRequestApproval(CommandApprovalParams),
    #[serde(rename = "item/fileChange/requestApproval")]
    FileChangeRequestApproval(FileChangeApprovalParams),
    #[serde(rename = "item/permissions/requestApproval")]
    PermissionsRequestApproval(PermissionsApprovalParams),
    #[serde(rename = "item/tool/requestUserInput")]
    ToolRequestUserInput(RequestUserInputParams),
    #[serde(rename = "mcpServer/elicitation/request")]
    McpServerElicitationRequest(ElicitationParams),
    #[serde(rename = "unknown")]
    Unknown(UnknownNotification),
}

impl ServerRequest {
    pub(crate) fn decode(method: &str, params: &Value) -> Self {
        let params = if params.is_null() {
            json!({})
        } else {
            params.clone()
        };
        match serde_json::from_value(json!({ "method": method, "params": params })) {
            Ok(request) => request,
            Err(error) => {
                eprintln!(
                    "codex server request {method} did not decode ({error}); forwarding as unknown"
                );
                Self::Unknown(UnknownNotification {
                    method: method.to_string(),
                    params: Json(params),
                })
            }
        }
    }
}

params!(CommandApprovalParams {
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
    reason: Option<String>,
});
params!(FileChangeApprovalParams {
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    reason: Option<String>,
    #[specta(type = Json)] changes: Option<Json>,
});
params!(PermissionsApprovalParams {
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    cwd: Option<String>,
    reason: Option<String>,
    #[specta(type = Json)] permissions: Option<Json>,
});
params!(RequestUserInputParams {
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    #[specta(type = Json)] questions: Option<Json>,
    /// Stamped by the session: the id of the item that preceded this
    /// question in stream order, so a restart can splice it back in place.
    after_item_id: Option<String>,
});
params!(ElicitationParams {
    thread_id: Option<String>,
    turn_id: Option<String>,
    server_name: Option<String>,
    mode: Option<String>,
    message: Option<String>,
    #[specta(type = Json)] requested_schema: Option<Json>,
    url: Option<String>,
    #[serde(rename = "_meta")]
    #[specta(type = Json)] meta: Option<Json>,
});

// ---------------------------------------------------------------------------
// Tauri event envelopes
// ---------------------------------------------------------------------------

/// `codex:event` — one notification, tagged with the home it belongs to so a
/// window bound to another account can drop it.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "codex:event")]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexEvent {
    pub codex_home: String,
    #[serde(flatten)]
    pub event: CodexNotification,
}

/// `codex:serverRequest` — a request awaiting the user's answer.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "codex:serverRequest")]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexServerRequest {
    pub codex_home: String,
    pub request_id: i64,
    #[serde(flatten)]
    pub request: ServerRequest,
}

/// `codex:disconnected` — the app-server child for this home went away.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "codex:disconnected")]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexDisconnected {
    pub codex_home: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Absent fields come back as `null`; drop them so the comparison is
    /// against what the frontend can observe.
    fn without_nulls(value: Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .filter(|(_, v)| !v.is_null())
                    .map(|(k, v)| (k, without_nulls(v)))
                    .collect(),
            ),
            other => other,
        }
    }

    /// Every known method round-trips to the JSON it was decoded from, so the
    /// wire the frontend sees is unchanged by typing it.
    fn round_trips(method: &str, params: Value) {
        let decoded = CodexNotification::decode(method, &params);
        assert!(
            !matches!(decoded, CodexNotification::Unknown(_)),
            "{method} decoded as Unknown"
        );
        let encoded = without_nulls(serde_json::to_value(&decoded).unwrap());
        assert_eq!(
            encoded,
            json!({ "method": method, "params": params }),
            "{method}"
        );
    }

    #[test]
    fn known_methods_round_trip_unchanged() {
        round_trips(
            "thread/started",
            json!({"thread": {"id": "t", "parentThreadId": "p"}}),
        );
        round_trips(
            "thread/status/changed",
            json!({"threadId": "t", "status": {"type": "idle"}}),
        );
        round_trips(
            "thread/name/updated",
            json!({"threadId": "t", "threadName": "Hi"}),
        );
        round_trips(
            "thread/project/updated",
            json!({"threadId": "t", "projectId": "p"}),
        );
        round_trips(
            "thread/goal/updated",
            json!({"threadId": "t", "turnId": "u", "goal": {"objective": "x"}}),
        );
        round_trips(
            "thread/tokenUsage/updated",
            json!({"threadId": "t", "turnId": "u", "tokenUsage": {"total": {}}}),
        );
        round_trips(
            "thread/settings/updated",
            json!({"threadId": "t", "threadSettings": {"subagentModelPolicy": "a"}}),
        );
        round_trips("thread/queue/changed", json!({"threadId": "t"}));
        round_trips("thread/reverted", json!({"threadId": "t"}));
        round_trips("thread/compacted", json!({"threadId": "t", "turnId": "u"}));
        round_trips(
            "turn/started",
            json!({"threadId": "t", "turn": {"id": "u", "status": "inProgress", "items": []}}),
        );
        round_trips(
            "turn/completed",
            json!({"threadId": "t", "turn": {"id": "u", "status": "completed"}}),
        );
        round_trips(
            "turn/plan/updated",
            json!({"threadId": "t", "turnId": "u", "explanation": "e", "plan": [{"step": "s", "status": "pending"}]}),
        );
        round_trips(
            "item/started",
            json!({"threadId": "t", "turnId": "u", "item": {"type": "agentMessage", "id": "i"}}),
        );
        round_trips(
            "item/updated",
            json!({"threadId": "t", "turnId": "u", "item": {"type": "commandExecution", "id": "i"}}),
        );
        round_trips(
            "item/completed",
            json!({"threadId": "t", "turnId": "u", "item": {"type": "exitedReviewMode", "id": "i"}}),
        );
        round_trips(
            "item/agentMessage/delta",
            json!({"threadId": "t", "turnId": "u", "itemId": "i", "delta": "d"}),
        );
        round_trips(
            "item/plan/delta",
            json!({"threadId": "t", "turnId": "u", "itemId": "i", "delta": "d"}),
        );
        round_trips(
            "item/reasoning/summaryPartAdded",
            json!({"threadId": "t", "turnId": "u", "itemId": "i", "summaryIndex": 1}),
        );
        round_trips(
            "item/reasoning/summaryTextDelta",
            json!({"threadId": "t", "turnId": "u", "itemId": "i", "delta": "d", "summaryIndex": 0}),
        );
        round_trips(
            "item/reasoning/textDelta",
            json!({"threadId": "t", "turnId": "u", "itemId": "i", "delta": "d", "contentIndex": 2}),
        );
        round_trips(
            "item/commandExecution/outputDelta",
            json!({"threadId": "t", "turnId": "u", "itemId": "i", "delta": "d"}),
        );
        round_trips(
            "item/commandExecution/terminalInteraction",
            json!({"threadId": "t", "turnId": "u", "itemId": "i", "processId": "p", "stdin": "y\n"}),
        );
        round_trips(
            "item/fileChange/patchUpdated",
            json!({"threadId": "t", "turnId": "u", "itemId": "i", "changes": [{"path": "a"}]}),
        );
        round_trips(
            "item/mcpToolCall/progress",
            json!({"threadId": "t", "turnId": "u", "itemId": "i", "message": "m"}),
        );
        round_trips(
            "item/autoApprovalReview/completed",
            json!({"threadId": "t", "turnId": "u", "itemId": "i", "targetItemId": "x", "review": {"decision": "approve"}}),
        );
        round_trips(
            "model/rerouted",
            json!({"threadId": "t", "turnId": "u", "fromModel": "a", "toModel": "b"}),
        );
        round_trips(
            "model/safetyBuffering/updated",
            json!({"threadId": "t", "turnId": "u", "showBufferingUi": true}),
        );
        round_trips(
            "hook/completed",
            json!({"threadId": "t", "turnId": "u", "run": {"status": "failed", "entries": []}}),
        );
        round_trips(
            "error",
            json!({"threadId": "t", "turnId": "u", "willRetry": true, "error": {"message": "boom"}}),
        );
        round_trips("warning", json!({"threadId": "t", "message": "careful"}));
        round_trips(
            "guardianWarning",
            json!({"threadId": "t", "message": "careful"}),
        );
        round_trips(
            "deprecationNotice",
            json!({"summary": "old", "details": "use new"}),
        );
        round_trips("configWarning", json!({"summary": "bad key"}));
        round_trips(
            "serverRequest/resolved",
            json!({"threadId": "t", "requestId": 7}),
        );
        round_trips(
            "account/rateLimits/updated",
            json!({"rateLimits": {"primary": {}}}),
        );
        round_trips(
            "mcpServer/startupStatus/updated",
            json!({"name": "s", "threadId": "t"}),
        );
        round_trips(
            "mcpServer/oauthLogin/completed",
            json!({"name": "s", "serverName": "s"}),
        );
        round_trips(
            "project/changed",
            json!({"projectId": "p", "changeType": "updated"}),
        );
        round_trips(
            "remoteControl/status/changed",
            json!({"status": "connected", "serverName": "s"}),
        );
    }

    #[test]
    fn every_known_method_is_listed() {
        // The list drives the log line; each entry must decode as a real variant.
        for method in KNOWN_METHODS {
            let decoded = CodexNotification::decode(method, &json!({}));
            assert!(
                !matches!(decoded, CodexNotification::Unknown(_)),
                "{method} missing from enum"
            );
        }
    }

    #[test]
    fn missing_fields_decode_as_none() {
        match CodexNotification::decode("turn/started", &Value::Null) {
            CodexNotification::TurnStarted(params) => {
                assert_eq!(params.thread_id, None);
                assert!(params.turn.is_none());
            }
            other => panic!("unexpected {other:?}"),
        }
        match CodexNotification::decode("error", &json!({"threadId": "t"})) {
            CodexNotification::Error(params) => assert_eq!(params.will_retry, None),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn unknown_method_keeps_the_raw_line() {
        let params = json!({"anything": [1, 2, 3]});
        match CodexNotification::decode("item/newThing", &params) {
            CodexNotification::Unknown(unknown) => {
                assert_eq!(unknown.method, "item/newThing");
                assert_eq!(unknown.params.0, params);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn mistyped_scalar_degrades_to_unknown() {
        match CodexNotification::decode("error", &json!({"willRetry": "yes"})) {
            CodexNotification::Unknown(unknown) => assert_eq!(unknown.method, "error"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn envelope_flattens_the_notification() {
        let event = CodexEvent {
            codex_home: "/home".into(),
            event: CodexNotification::decode("thread/queue/changed", &json!({"threadId": "t"})),
        };
        assert_eq!(
            without_nulls(serde_json::to_value(&event).unwrap()),
            json!({"codexHome": "/home", "method": "thread/queue/changed", "params": {"threadId": "t"}})
        );
    }

    #[test]
    fn server_requests_round_trip_and_carry_the_request_id() {
        let params =
            json!({"threadId": "t", "turnId": "u", "itemId": "i", "questions": [{"id": "q"}]});
        let mut request = ServerRequest::decode("item/tool/requestUserInput", &params);
        if let ServerRequest::ToolRequestUserInput(inner) = &mut request {
            inner.after_item_id = Some("prev".into());
        } else {
            panic!("unexpected {request:?}");
        }
        let envelope = CodexServerRequest {
            codex_home: "/home".into(),
            request_id: 3,
            request,
        };
        assert_eq!(
            without_nulls(serde_json::to_value(&envelope).unwrap()),
            json!({
                "codexHome": "/home",
                "requestId": 3,
                "method": "item/tool/requestUserInput",
                "params": {"threadId": "t", "turnId": "u", "itemId": "i", "questions": [{"id": "q"}], "afterItemId": "prev"}
            })
        );
        let elicitation = ServerRequest::decode(
            "mcpServer/elicitation/request",
            &json!({"threadId": "t", "_meta": {"suggestion_id": 1}}),
        );
        match elicitation {
            ServerRequest::McpServerElicitationRequest(inner) => {
                assert_eq!(inner.meta.unwrap().0, json!({"suggestion_id": 1}))
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            ServerRequest::decode("item/tool/call", &json!({})),
            ServerRequest::Unknown(_)
        ));
    }
}
