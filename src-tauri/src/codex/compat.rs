//! Recognising "this Codex does not have that API" across CLI versions.
//!
//! The app never pins a Codex version. Instead, an optional API — one that
//! landed in some release, or sits behind the `experimentalApi` capability —
//! is described as a [`Feature`], tried once, and the refusal remembered on
//! the live child so later calls short-circuit. See `CodexSession::send_gated`.
//!
//! A refusal is not a version check: the same shapes come back from a build
//! that predates the method, a build that has it but withheld the capability,
//! and a well-behaved server answering with JSON-RPC "method not found".

use serde_json::Value;

/// JSON-RPC "method not found" — what a well-behaved server returns for an API
/// it does not have. Older Codex builds instead fail to deserialise the method
/// name at all, so [`method_unsupported`] has to recognise that shape too.
const METHOD_NOT_FOUND: i64 = -32601;

/// An app-server API that some supported Codex versions lack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feature {
    /// Method-name prefix shared by every call in the feature, e.g.
    /// `thread/queue`. Also the key the refusal is cached under.
    pub method_prefix: &'static str,
    /// Prefix put on the error handed to the frontend so it can tell "absent"
    /// from "failed". The frontend matches these exact strings.
    pub error_prefix: &'static str,
    /// The first Codex `(major, minor)` release that has the API. Documentation
    /// for `docs/SUPPORTED_VERSIONS.md` and the live suite's `expect_legacy`;
    /// the app itself never compares versions.
    pub since: (u64, u64),
}

impl Feature {
    pub const REVERT: Feature = Feature {
        method_prefix: "thread/revert",
        error_prefix: "codex-revert-unsupported",
        since: (0, 149),
    };
    pub const QUEUE: Feature = Feature {
        method_prefix: "thread/queue",
        error_prefix: "codex-queue-unsupported",
        since: (0, 149),
    };
    pub const PROJECTS: Feature = Feature {
        method_prefix: "project/",
        error_prefix: "codex-projects-unsupported",
        since: (0, 149),
    };
    pub const SECTIONS: Feature = Feature {
        method_prefix: "threadSection/",
        error_prefix: "codex-sections-unsupported",
        since: (0, 149),
    };
    /// `turn/settings/update`: change model or effort while a turn is running
    /// (Codex ≥0.151).
    pub const TURN_SETTINGS: Feature = Feature {
        method_prefix: "turn/settings/update",
        error_prefix: "codex-turn-settings-unsupported",
        since: (0, 151),
    };

    /// Every gated API, for the docs matrix and the live suite.
    pub const ALL: [Feature; 5] = [
        Self::REVERT,
        Self::QUEUE,
        Self::PROJECTS,
        Self::SECTIONS,
        Self::TURN_SETTINGS,
    ];

    pub(crate) fn error(&self, reason: &str) -> String {
        format!("{}: {reason}", self.error_prefix)
    }
}

/// The JSON-RPC error object inside a failure reported by `child.rs`, which
/// formats them as `Codex request failed: {json}`.
pub(crate) fn error_payload(error: &str) -> Option<Value> {
    error
        .split_once("Codex request failed: ")
        .and_then(|(_, rest)| serde_json::from_str::<Value>(rest).ok())
}

/// Why this Codex cannot serve `method_prefix`, or `None` if the error is a
/// normal failure of an API that does work and should be surfaced as-is.
///
/// Deliberately does not key off the `-32600` "invalid request" code, which
/// recoverable errors also carry; treating it as unsupported would strand the
/// feature for the rest of the child's life over a transient failure.
pub fn method_unsupported(error: &str, method_prefix: &str) -> Option<String> {
    let payload = error_payload(error);
    let code = payload
        .as_ref()
        .and_then(|value| value.get("code"))
        .and_then(Value::as_i64);
    let message = payload
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or(error);

    if code == Some(METHOD_NOT_FOUND) {
        return Some(format!("this Codex has no {method_prefix} APIs"));
    }
    // Older builds predate the variant, so serde rejects the method name
    // before the server ever dispatches it.
    if message.contains("unknown variant") && message.contains(method_prefix) {
        return Some(format!(
            "this Codex version is older than the {method_prefix} APIs"
        ));
    }
    if message.contains("requires experimentalApi capability") && message.contains(method_prefix) {
        return Some(format!(
            "Codex did not grant the experimental API {method_prefix} needs"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::method_unsupported;

    /// Wrap a message the way `child.rs` reports a JSON-RPC failure.
    pub(crate) fn failure(code: i64, message: &str) -> String {
        format!(
            "Codex request failed: {}",
            serde_json::json!({"code": code, "message": message})
        )
    }

    #[test]
    fn classifies_a_codex_without_the_method() {
        assert!(method_unsupported(
            &failure(
                -32600,
                "Invalid request: unknown variant `thread/revert`, expected one of `initialize`, `thread/start`"
            ),
            "thread/revert"
        )
        .is_some());
        assert!(method_unsupported(&failure(-32601, "Method not found"), "project/").is_some());
    }

    #[test]
    fn classifies_a_codex_without_turn_settings() {
        assert!(method_unsupported(
            &failure(
                -32600,
                "Invalid request: unknown variant `turn/settings/update`, expected one of `initialize`"
            ),
            super::Feature::TURN_SETTINGS.method_prefix
        )
        .is_some());
    }

    #[test]
    fn classifies_a_withheld_capability() {
        assert!(method_unsupported(
            &failure(-32600, "project/list requires experimentalApi capability"),
            "project/"
        )
        .is_some());
    }

    #[test]
    fn only_matches_its_own_prefix() {
        let refusal = failure(
            -32600,
            "Invalid request: unknown variant `thread/queue/add`, expected one of `initialize`",
        );
        assert!(method_unsupported(&refusal, "thread/queue").is_some());
        assert!(method_unsupported(&refusal, "threadSection/").is_none());
    }

    #[test]
    fn passes_through_ordinary_failures() {
        assert!(
            method_unsupported(&failure(-32600, "thread not found: abc"), "thread/revert")
                .is_none()
        );
        assert!(method_unsupported("Codex exited before responding", "thread/revert").is_none());
        assert!(method_unsupported("Codex request failed: client not found", "project/").is_none());
    }
}
