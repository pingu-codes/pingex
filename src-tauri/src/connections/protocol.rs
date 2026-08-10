//! Talking to the remote-control relay, and merging what it reports with the
//! local records.
//!
//! Protocol clients are the source of truth for health; the local store is the
//! source of truth for the user-chosen name.

use serde_json::{json, Value};
use tauri::{AppHandle, State};

use super::store::{read_records, upsert_seen};
use super::{Connection, DeviceRecord, ProtocolClient};
use crate::util::json::{arr_or_empty, i64_at, str_at};
use crate::util::time::unix_secs;
use crate::AppState;

/// Merge protocol-reported clients with locally-stored records. Protocol
/// clients win on live fields; the store wins on the user-chosen name. Records
/// with no matching protocol client are surfaced as `"local"`.
pub(crate) fn merge_connections(
    protocol: &[ProtocolClient],
    records: &[DeviceRecord],
) -> Vec<Connection> {
    let mut connections = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for client in protocol {
        seen.insert(client.client_id.clone());
        let record = records.iter().find(|r| r.client_id == client.client_id);
        let name = record
            .and_then(|r| r.name.clone())
            .filter(|name| !name.trim().is_empty())
            .or_else(|| client.display_name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| fallback_name(&client.platform));
        let last_seen = max_opt(client.last_seen_at, record.and_then(|r| r.last_seen));
        connections.push(Connection {
            client_id: client.client_id.clone(),
            name,
            platform: client
                .platform
                .clone()
                .or_else(|| record.and_then(|r| r.platform.clone())),
            device_model: client.device_model.clone(),
            app_version: client.app_version.clone(),
            paired_at: record.map(|r| r.paired_at),
            last_seen,
            scope: record.and_then(|r| r.scope.clone()),
            source: "protocol".into(),
        });
    }

    for record in records {
        if seen.contains(&record.client_id) {
            continue;
        }
        let name = record
            .name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| fallback_name(&record.platform));
        connections.push(Connection {
            client_id: record.client_id.clone(),
            name,
            platform: record.platform.clone(),
            device_model: None,
            app_version: None,
            paired_at: Some(record.paired_at),
            last_seen: record.last_seen,
            scope: record.scope.clone(),
            source: "local".into(),
        });
    }

    connections.sort_by(|a, b| {
        let a_key = a.last_seen.or(a.paired_at).unwrap_or(0);
        let b_key = b.last_seen.or(b.paired_at).unwrap_or(0);
        b_key
            .cmp(&a_key)
            .then_with(|| a.client_id.cmp(&b.client_id))
    });
    connections
}

fn fallback_name(platform: &Option<String>) -> String {
    match platform.as_deref() {
        Some(platform) if !platform.trim().is_empty() => format!("{platform} device"),
        _ => "Paired device".to_string(),
    }
}

fn max_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (value @ Some(_), None) | (None, value @ Some(_)) => value,
        (None, None) => None,
    }
}

fn parse_protocol_clients(response: &Value) -> Vec<ProtocolClient> {
    let text = |item: &Value, key: &str| str_at(item, key).map(str::to_string);
    arr_or_empty(response, "data")
        .iter()
        .filter_map(|item| {
            Some(ProtocolClient {
                client_id: str_at(item, "clientId")?.to_string(),
                display_name: text(item, "displayName"),
                platform: text(item, "platform"),
                device_model: text(item, "deviceModel"),
                app_version: text(item, "appVersion"),
                last_seen_at: i64_at(item, "lastSeenAt"),
            })
        })
        .collect()
}

/// The relay's `environmentId`, needed by `client/list` and `client/revoke`.
/// Returns `None` when remote control is disabled or unavailable — callers then
/// fall back to Store-only data instead of failing.
pub(crate) async fn environment_id(app: &AppHandle, state: &State<'_, AppState>) -> Option<String> {
    let status = state
        .session
        .request(app, "remoteControl/status/read", json!({}))
        .await
        .ok()?;
    status
        .get("environmentId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Fetch the live client list (best-effort) and upsert every client so a device
/// recorded at pairing claim time gains its live metadata. Returns the parsed
/// protocol clients, or an empty vec when the relay is unavailable.
pub(crate) async fn refresh_from_protocol(
    app: &AppHandle,
    state: &State<'_, AppState>,
) -> Vec<ProtocolClient> {
    let Some(environment_id) = environment_id(app, state).await else {
        return Vec::new();
    };
    let response = state
        .session
        .request(
            app,
            "remoteControl/client/list",
            json!({ "environmentId": environment_id }),
        )
        .await;
    let clients = match response {
        Ok(response) => parse_protocol_clients(&response),
        Err(_) => return Vec::new(),
    };
    let now = unix_secs();
    for client in &clients {
        let _ = upsert_seen(
            &state.database(),
            &client.client_id,
            client.platform.as_deref(),
            client.last_seen_at,
            None,
            now,
        )
        .await;
    }
    clients
}

pub(crate) async fn collect_connections(
    app: &AppHandle,
    state: &State<'_, AppState>,
) -> Result<Vec<Connection>, String> {
    crate::connections::store::ensure_table(&state.database()).await?;
    let protocol = refresh_from_protocol(app, state).await;
    let records = read_records(&state.database()).await?;
    Ok(merge_connections(&protocol, &records))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_client_list_payload() {
        let payload = json!({
            "data": [
                {"clientId": "c1", "displayName": "Phone", "platform": "iOS", "lastSeenAt": 123},
                {"clientId": "c2", "platform": "android", "deviceModel": "Pixel"},
                {"displayName": "no id — dropped"}
            ],
            "nextCursor": null
        });
        let clients = parse_protocol_clients(&payload);
        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].client_id, "c1");
        assert_eq!(clients[0].last_seen_at, Some(123));
        assert_eq!(clients[1].device_model.as_deref(), Some("Pixel"));
    }
    fn record(
        client_id: &str,
        name: Option<&str>,
        last_seen: Option<i64>,
        paired_at: i64,
    ) -> DeviceRecord {
        DeviceRecord {
            client_id: client_id.into(),
            platform: Some("iOS".into()),
            name: name.map(str::to_string),
            paired_at,
            last_seen,
            scope: Some("full".into()),
        }
    }

    fn client(client_id: &str, display: Option<&str>, last_seen: Option<i64>) -> ProtocolClient {
        ProtocolClient {
            client_id: client_id.into(),
            display_name: display.map(str::to_string),
            platform: Some("iOS".into()),
            device_model: Some("iPhone".into()),
            app_version: Some("1.0".into()),
            last_seen_at: last_seen,
        }
    }

    #[test]
    fn store_name_overrides_protocol_display_name() {
        let merged = merge_connections(
            &[client("a", Some("Protocol Name"), Some(100))],
            &[record("a", Some("My Phone"), Some(50), 10)],
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name, "My Phone");
        // Live health wins: newest last_seen across both sources.
        assert_eq!(merged[0].last_seen, Some(100));
        assert_eq!(merged[0].source, "protocol");
        assert_eq!(merged[0].paired_at, Some(10));
    }
    #[test]
    fn local_only_records_are_surfaced() {
        let merged = merge_connections(&[], &[record("ghost", None, None, 42)]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source, "local");
        assert_eq!(merged[0].name, "iOS device");
        assert_eq!(merged[0].paired_at, Some(42));
    }
    #[test]
    fn protocol_display_name_used_when_no_local_name() {
        let merged = merge_connections(&[client("a", Some("Ada's iPad"), Some(9))], &[]);
        assert_eq!(merged[0].name, "Ada's iPad");
        assert_eq!(merged[0].device_model.as_deref(), Some("iPhone"));
    }
    #[test]
    fn connections_sort_by_most_recent_activity() {
        let merged = merge_connections(
            &[
                client("old", None, Some(10)),
                client("new", None, Some(500)),
            ],
            &[record("stale", None, None, 5)],
        );
        assert_eq!(merged[0].client_id, "new");
        assert_eq!(merged[1].client_id, "old");
        assert_eq!(merged[2].client_id, "stale");
    }
}
