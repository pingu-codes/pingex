//! Parsing and building `codex://` links.
//!
//! Pure string work with no I/O: percent coding, query splitting, and the
//! `codex://threads/<id>` grammar. Unit tested against URL decoding, missing
//! params, and malformed ids.

/// What a `codex://` link asks the app to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeepLinkKind {
    /// Open an existing thread by id.
    Thread(String),
    /// Open a fresh draft.
    New,
}

/// A parsed `codex://threads/...` deep link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeepLink {
    pub(crate) kind: DeepLinkKind,
    /// Requested working directory (the worktree the thread belongs to).
    pub(crate) path: Option<String>,
    /// Requested `CODEX_HOME` (raw, may contain `~`).
    pub(crate) codex_home: Option<String>,
    /// Optional source label (e.g. the originating CLI).
    pub(crate) label: Option<String>,
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Decode `%XX` escapes and `+` (as space) from a query component.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                    (Some(hi), Some(lo)) => {
                        out.push(hi * 16 + lo);
                        i += 3;
                    }
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Split `a=1&b=2` into decoded key/value pairs.
fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = percent_decode(parts.next().unwrap_or(""));
            let value = percent_decode(parts.next().unwrap_or(""));
            (key, value)
        })
        .collect()
}

/// Parse a `codex://threads/<id>?...` (or `.../new`) deep link. Returns a
/// redacted, actionable error for anything that is not a well-formed link.
pub(crate) fn parse_deep_link(url: &str) -> Result<DeepLink, String> {
    let rest = url
        .strip_prefix("codex://")
        .ok_or_else(|| "This is not a codex:// link".to_string())?;
    let (path_part, query_part) = match rest.split_once('?') {
        Some((path, query)) => (path, query),
        None => (rest, ""),
    };
    let path_part = path_part.trim_end_matches('/');
    let after = path_part
        .strip_prefix("threads")
        .ok_or_else(|| "Unsupported codex:// link".to_string())?;
    // Only `threads` or `threads/<segment>` are valid; reject `threadsfoo`.
    if !after.is_empty() && !after.starts_with('/') {
        return Err("Unsupported codex:// link".to_string());
    }
    let id_segment = after.strip_prefix('/').unwrap_or(after);

    let kind = if id_segment.is_empty() {
        return Err("The codex:// link is missing a thread id".to_string());
    } else if id_segment == "new" {
        DeepLinkKind::New
    } else {
        let id = percent_decode(id_segment);
        if id.trim().is_empty() || id.contains('/') {
            return Err("The codex:// link has an invalid thread id".to_string());
        }
        DeepLinkKind::Thread(id)
    };

    let params = parse_query(query_part);
    let get = |key: &str| {
        params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, value)| value.clone())
            .filter(|value| !value.is_empty())
    };

    Ok(DeepLink {
        kind,
        path: get("path"),
        codex_home: get("codexHome"),
        label: get("label"),
    })
}

/// POSIX single-quote a value so it survives the shell verbatim.
fn shell_single_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            // Close the quote, emit an escaped quote, reopen.
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// `CODEX_HOME=<home> <binary> resume <id> --cd <cwd>` with every field quoted.
/// A bare `codex` on PATH is left unquoted for readability; an explicit binary
/// path is quoted.
pub(crate) fn build_resume_command(
    codex_home: &str,
    codex_binary: &str,
    thread_id: &str,
    cwd: &str,
) -> String {
    let binary = if codex_binary == "codex" {
        "codex".to_string()
    } else {
        shell_single_quote(codex_binary)
    };
    format!(
        "CODEX_HOME={} {} resume {} --cd {}",
        shell_single_quote(codex_home),
        binary,
        shell_single_quote(thread_id),
        shell_single_quote(cwd),
    )
}

/// Percent-encode an unreserved-only subset for a `codex://` query value.
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Build the shareable `codex://threads/<id>?path=&codexHome=&label=` link.
pub(crate) fn build_thread_link(
    thread_id: &str,
    cwd: &str,
    codex_home: &str,
    label: Option<&str>,
) -> String {
    let mut url = format!(
        "codex://threads/{}?path={}&codexHome={}",
        percent_encode(thread_id),
        percent_encode(cwd),
        percent_encode(codex_home),
    );
    if let Some(label) = label.filter(|label| !label.is_empty()) {
        url.push_str(&format!("&label={}", percent_encode(label)));
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_thread_link_with_all_params() {
        let link = parse_deep_link(
            "codex://threads/abc-123?path=%2Frepo%2Fwt&codexHome=%2Fhome%2F.codex&label=cli",
        )
        .unwrap();
        assert_eq!(link.kind, DeepLinkKind::Thread("abc-123".to_string()));
        assert_eq!(link.path.as_deref(), Some("/repo/wt"));
        assert_eq!(link.codex_home.as_deref(), Some("/home/.codex"));
        assert_eq!(link.label.as_deref(), Some("cli"));
    }
    #[test]
    fn parses_new_thread_link() {
        let link = parse_deep_link("codex://threads/new?path=%2Frepo").unwrap();
        assert_eq!(link.kind, DeepLinkKind::New);
        assert_eq!(link.path.as_deref(), Some("/repo"));
        assert_eq!(link.codex_home, None);
    }
    #[test]
    fn decodes_plus_and_spaces_in_params() {
        let link =
            parse_deep_link("codex://threads/id1?label=my+source&path=%2Ftmp%2Fa%20b").unwrap();
        assert_eq!(link.label.as_deref(), Some("my source"));
        assert_eq!(link.path.as_deref(), Some("/tmp/a b"));
    }
    #[test]
    fn missing_and_bad_ids_are_errors() {
        assert!(parse_deep_link("codex://threads").is_err());
        assert!(parse_deep_link("codex://threads/").is_err());
        // A slash inside the (decoded) id is rejected.
        assert!(parse_deep_link("codex://threads/a%2Fb").is_err());
        // Wrong scheme / host.
        assert!(parse_deep_link("https://threads/abc").is_err());
        assert!(parse_deep_link("codex://projects/abc").is_err());
        assert!(parse_deep_link("codex://threadsfoo/abc").is_err());
    }
    #[test]
    fn empty_params_are_dropped() {
        let link = parse_deep_link("codex://threads/id1?path=&codexHome=").unwrap();
        assert_eq!(link.path, None);
        assert_eq!(link.codex_home, None);
    }
    #[test]
    fn builds_thread_link_round_trips() {
        let url = build_thread_link("abc-123", "/repo/wt", "/home/.codex", Some("desktop"));
        assert_eq!(
            url,
            "codex://threads/abc-123?path=%2Frepo%2Fwt&codexHome=%2Fhome%2F.codex&label=desktop"
        );
        let parsed = parse_deep_link(&url).unwrap();
        assert_eq!(parsed.kind, DeepLinkKind::Thread("abc-123".into()));
        assert_eq!(parsed.path.as_deref(), Some("/repo/wt"));
        assert_eq!(parsed.codex_home.as_deref(), Some("/home/.codex"));
        assert_eq!(parsed.label.as_deref(), Some("desktop"));
    }
}
