//! A one-tool MCP server over stdio, used only by the live end-to-end suite
//! (`tests/live_codex`) so a real `codex app-server` has an MCP server whose
//! behaviour we fully control. `echo` returns its `text` argument verbatim.
//!
//! Built automatically by `cargo test` (examples are), and pointed at from
//! the suite's generated `config.toml`.

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

fn reply(out: &mut impl Write, id: &Value, result: Value) {
    let message = json!({"jsonrpc": "2.0", "id": id, "result": result});
    let _ = writeln!(out, "{message}");
    let _ = out.flush();
}

fn main() {
    let stdin = io::stdin();
    let mut out = io::stdout();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        if id.is_null() {
            // Notifications (`notifications/initialized`) need no answer.
            continue;
        }
        match method {
            "initialize" => reply(
                &mut out,
                &id,
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "pingex-e2e-echo", "version": "0.1.0"},
                }),
            ),
            "ping" => reply(&mut out, &id, json!({})),
            "tools/list" => reply(
                &mut out,
                &id,
                json!({
                    "tools": [{
                        "name": "echo",
                        "description": "Returns the given text unchanged.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {"text": {"type": "string"}},
                            "required": ["text"],
                        },
                    }]
                }),
            ),
            "tools/call" => {
                let text = message
                    .pointer("/params/arguments/text")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                reply(
                    &mut out,
                    &id,
                    json!({"content": [{"type": "text", "text": format!("ECHO:{text}")}], "isError": false}),
                );
            }
            _ => {
                let error = json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("unknown method {method}")}});
                let _ = writeln!(out, "{error}");
                let _ = out.flush();
            }
        }
    }
}
