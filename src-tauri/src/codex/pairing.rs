//! Remote control pairing: enable the relay, get a pairing code, and render it
//! as a QR the phone can scan. Both commands are thin passes through to the
//! app-server — the QR is the only thing built here.

use serde_json::{json, Value};
use tauri::{AppHandle, State};

use crate::util::json::str_at;
use crate::AppState;

#[tauri::command]
pub(crate) async fn remote_pairing_start(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    ctx.session
        .request(&app, "remoteControl/enable", json!({}))
        .await?;
    let response = ctx
        .session
        .request(&app, "remoteControl/pairing/start", json!({}))
        .await?;
    let pairing_code = str_at(&response, "pairingCode")
        .ok_or_else(|| "Codex returned no pairing code".to_string())?;
    let qr = qrcode::QrCode::new(pairing_code.as_bytes())
        .map_err(|error| format!("Could not build pairing QR code: {error}"))?;
    let qr_svg = qr
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(220, 220)
        .quiet_zone(true)
        .build();
    Ok(json!({
        "qrSvg": qr_svg,
        "pairingCode": pairing_code,
        "manualPairingCode": response.get("manualPairingCode").cloned().unwrap_or(Value::Null),
        "expiresAt": response.get("expiresAt").cloned().unwrap_or(Value::Null),
    }))
}

#[tauri::command]
pub(crate) async fn remote_pairing_status(
    pairing_code: String,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    ctx.session
        .request(
            &app,
            "remoteControl/pairing/status",
            json!({"pairingCode": pairing_code}),
        )
        .await
}
