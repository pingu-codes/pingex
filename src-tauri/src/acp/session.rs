//! The ACP requests the driver sends, and the parts of their responses it
//! reads. Public through `e2e` so the live suite sends exactly these.

use serde_json::{json, Value};

use crate::util::json::str_at;

/// The protocol version this client speaks.
pub const PROTOCOL_VERSION: u64 = 1;

/// `initialize`: no client-hosted file system or terminals, so the agent
/// runs its own tools.
pub fn initialize() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "clientCapabilities": {
            "fs": {"readTextFile": false, "writeTextFile": false},
            "terminal": false,
        },
        "clientInfo": {"name": "pingex", "title": "Pingu Codex", "version": env!("CARGO_PKG_VERSION")},
    })
}

/// What `initialize` says the agent can do.
#[derive(Debug, Clone, Default)]
pub struct AgentInfo {
    pub protocol_version: u64,
    pub load_session: bool,
    pub images: bool,
    pub version: Option<String>,
}

pub fn agent_info(result: &Value) -> AgentInfo {
    let capabilities = result.get("agentCapabilities");
    let flag = |path: &[&str]| {
        let mut value = capabilities;
        for key in path {
            value = value.and_then(|value| value.get(*key));
        }
        value.and_then(Value::as_bool).unwrap_or(false)
    };
    AgentInfo {
        protocol_version: result
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        load_session: flag(&["loadSession"]),
        images: flag(&["promptCapabilities", "image"]),
        version: result
            .get("agentInfo")
            .and_then(|info| str_at(info, "version"))
            .map(str::to_string),
    }
}

pub fn new_session(cwd: &str) -> Value {
    json!({"cwd": cwd, "mcpServers": []})
}

pub fn load_session(session_id: &str, cwd: &str) -> Value {
    json!({"sessionId": session_id, "cwd": cwd, "mcpServers": []})
}

pub fn prompt(session_id: &str, blocks: Vec<Value>) -> Value {
    json!({"sessionId": session_id, "prompt": blocks})
}

pub fn cancel(session_id: &str) -> Value {
    json!({"sessionId": session_id})
}

pub fn set_mode(session_id: &str, mode_id: &str) -> Value {
    json!({"sessionId": session_id, "modeId": mode_id})
}

pub fn set_model(session_id: &str, model: &str) -> Value {
    json!({"sessionId": session_id, "configId": "model", "value": model})
}

/// The session's current mode, from `session/new` or `session/load`.
pub fn current_mode(result: &Value) -> Option<String> {
    result
        .get("modes")
        .and_then(|modes| str_at(modes, "currentModeId"))
        .map(str::to_string)
}

/// The model the session runs, from its `configOptions`.
pub fn current_model(result: &Value) -> Option<String> {
    result
        .get("configOptions")
        .and_then(Value::as_array)?
        .iter()
        .find(|option| str_at(option, "id") == Some("model"))
        .and_then(|option| str_at(option, "currentValue"))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maki_initialize_reports_load_and_images() {
        let info = agent_info(&json!({
            "protocolVersion": 1,
            "agentCapabilities": {
                "loadSession": true,
                "promptCapabilities": {"image": true, "audio": false, "embeddedContext": true},
                "sessionCapabilities": {},
            },
            "authMethods": [],
            "agentInfo": {"name": "maki", "version": "0.6.5"},
        }));
        assert_eq!(info.protocol_version, 1);
        assert!(info.load_session);
        assert!(info.images);
        assert_eq!(info.version.as_deref(), Some("0.6.5"));
    }

    #[test]
    fn a_new_session_names_its_mode_and_model() {
        let result = json!({
            "sessionId": "CgSx2zsTS2LfXXNkEUpKP",
            "modes": {"currentModeId": "build", "availableModes": []},
            "configOptions": [{"id": "model", "type": "select", "currentValue": "openai/gpt-6-luna"}],
        });
        assert_eq!(current_mode(&result).as_deref(), Some("build"));
        assert_eq!(current_model(&result).as_deref(), Some("openai/gpt-6-luna"));
    }
}
