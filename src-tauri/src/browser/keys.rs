//! Key presses for `Input.dispatchKeyEvent`, spelled the way people write
//! them: `Enter`, `Tab`, `Shift+Tab`, `Meta+a`, `ArrowDown`.

use serde_json::{json, Value};

const ALT: i64 = 1;
const CTRL: i64 = 2;
const META: i64 = 4;
const SHIFT: i64 = 8;

/// `(key, code, windowsVirtualKeyCode, text)` for the named keys.
fn named(key: &str) -> Option<(&'static str, &'static str, i64, Option<&'static str>)> {
    Some(match key.to_ascii_lowercase().as_str() {
        "enter" | "return" => ("Enter", "Enter", 13, Some("\r")),
        "tab" => ("Tab", "Tab", 9, None),
        "escape" | "esc" => ("Escape", "Escape", 27, None),
        "backspace" => ("Backspace", "Backspace", 8, None),
        "delete" | "del" => ("Delete", "Delete", 46, None),
        "space" | " " => (" ", "Space", 32, Some(" ")),
        "arrowup" | "up" => ("ArrowUp", "ArrowUp", 38, None),
        "arrowdown" | "down" => ("ArrowDown", "ArrowDown", 40, None),
        "arrowleft" | "left" => ("ArrowLeft", "ArrowLeft", 37, None),
        "arrowright" | "right" => ("ArrowRight", "ArrowRight", 39, None),
        "home" => ("Home", "Home", 36, None),
        "end" => ("End", "End", 35, None),
        "pageup" => ("PageUp", "PageUp", 33, None),
        "pagedown" => ("PageDown", "PageDown", 34, None),
        _ => return None,
    })
}

/// The `keyDown` and `keyUp` params for a chord such as `Shift+Tab`.
pub(crate) fn key_events(chord: &str) -> Result<[Value; 2], String> {
    let parts: Vec<&str> = chord.split('+').map(str::trim).collect();
    let (key, modifier_names) = match parts.split_last() {
        Some((key, rest)) if !key.is_empty() => (*key, rest),
        // "Ctrl++" splits into an empty last part.
        _ if chord.ends_with('+') => ("+", &parts[..parts.len().saturating_sub(2)]),
        _ => return Err(format!("no key in {chord:?}")),
    };
    let mut modifiers = 0;
    for name in modifier_names {
        modifiers |= match name.to_ascii_lowercase().as_str() {
            "alt" | "option" => ALT,
            "ctrl" | "control" => CTRL,
            "meta" | "cmd" | "command" => META,
            "shift" => SHIFT,
            other => return Err(format!("unknown modifier {other:?} in {chord:?}")),
        };
    }
    let (key, code, key_code, text) = match named(key) {
        Some((key, code, key_code, text)) => (
            key.to_string(),
            code.to_string(),
            key_code,
            text.map(str::to_string),
        ),
        None if key.chars().count() == 1 => {
            let character = key.chars().next().unwrap();
            let upper = character.to_ascii_uppercase();
            let code = if upper.is_ascii_alphabetic() {
                format!("Key{upper}")
            } else if character.is_ascii_digit() {
                format!("Digit{character}")
            } else {
                String::new()
            };
            let shown = if modifiers & SHIFT != 0 {
                upper.to_string()
            } else {
                character.to_string()
            };
            (shown.clone(), code, upper as i64, Some(shown))
        }
        None => return Err(format!("unknown key {key:?}")),
    };
    // A chord with Ctrl/Meta/Alt is a shortcut, not typing.
    let text = text.filter(|_| modifiers & (CTRL | META | ALT) == 0);
    let mut down = json!({
        "type": if text.is_some() { "keyDown" } else { "rawKeyDown" },
        "key": key, "code": code, "windowsVirtualKeyCode": key_code, "modifiers": modifiers,
    });
    if let Some(text) = &text {
        down["text"] = json!(text);
        down["unmodifiedText"] = json!(text);
    }
    if cfg!(target_os = "macos") && modifiers & META != 0 {
        // Chrome on macOS routes Cmd shortcuts through editing commands.
        if let Some(command) = mac_command(&key) {
            down["commands"] = json!([command]);
        }
    }
    let up = json!({
        "type": "keyUp", "key": key, "code": code, "windowsVirtualKeyCode": key_code, "modifiers": modifiers,
    });
    Ok([down, up])
}

fn mac_command(key: &str) -> Option<&'static str> {
    Some(match key.to_ascii_lowercase().as_str() {
        "a" => "selectAll",
        "c" => "copy",
        "x" => "cut",
        "v" => "paste",
        "z" => "undo",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_become_key_down_and_up() {
        let [down, up] = key_events("Enter").unwrap();
        assert_eq!(down["type"], "keyDown");
        assert_eq!(down["text"], "\r");
        assert_eq!(up["type"], "keyUp");
        let [down, _] = key_events("Shift+Tab").unwrap();
        assert_eq!(down["modifiers"], SHIFT);
        assert_eq!(down["type"], "rawKeyDown");
        let [down, _] = key_events("Meta+a").unwrap();
        assert_eq!(down["modifiers"], META);
        assert!(down.get("text").is_none());
        let [down, _] = key_events("Shift+x").unwrap();
        assert_eq!(down["text"], "X");
        assert_eq!(down["code"], "KeyX");
        assert!(key_events("Hyper+a").is_err());
        assert!(key_events("F13").is_err());
    }
}
