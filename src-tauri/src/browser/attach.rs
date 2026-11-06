//! Finding the user's running Chrome.
//!
//! Since Chrome 136 `--remote-debugging-port` is ignored on the default
//! profile. Chrome 144+ instead lets the user switch on remote debugging for
//! the browser they are already using (chrome://inspect/#remote-debugging);
//! Chrome then writes `DevToolsActivePort` into its user-data dir and asks
//! the user to allow each incoming connection. Every Chromium-family browser
//! writes the same file, so discovery is a list of user-data dirs.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A browser whose user-data dir Pingex knows how to find.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BrowserChannel {
    Chrome,
    ChromeBeta,
    ChromeCanary,
    Chromium,
    Brave,
    Edge,
}

impl BrowserChannel {
    pub(crate) const ALL: [BrowserChannel; 6] = [
        Self::Chrome,
        Self::ChromeBeta,
        Self::ChromeCanary,
        Self::Chromium,
        Self::Brave,
        Self::Edge,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome",
            Self::ChromeBeta => "Chrome Beta",
            Self::ChromeCanary => "Chrome Canary",
            Self::Chromium => "Chromium",
            Self::Brave => "Brave",
            Self::Edge => "Edge",
        }
    }

    /// The user-data dir, relative to the per-OS base (see [`user_data_dir`]).
    fn relative_dir(self) -> &'static str {
        if cfg!(target_os = "macos") {
            match self {
                Self::Chrome => "Google/Chrome",
                Self::ChromeBeta => "Google/Chrome Beta",
                Self::ChromeCanary => "Google/Chrome Canary",
                Self::Chromium => "Chromium",
                Self::Brave => "BraveSoftware/Brave-Browser",
                Self::Edge => "Microsoft Edge",
            }
        } else if cfg!(windows) {
            match self {
                Self::Chrome => "Google/Chrome/User Data",
                Self::ChromeBeta => "Google/Chrome Beta/User Data",
                Self::ChromeCanary => "Google/Chrome SxS/User Data",
                Self::Chromium => "Chromium/User Data",
                Self::Brave => "BraveSoftware/Brave-Browser/User Data",
                Self::Edge => "Microsoft/Edge/User Data",
            }
        } else {
            match self {
                Self::Chrome => "google-chrome",
                Self::ChromeBeta => "google-chrome-beta",
                Self::ChromeCanary => "google-chrome-unstable",
                Self::Chromium => "chromium",
                Self::Brave => "BraveSoftware/Brave-Browser",
                Self::Edge => "microsoft-edge",
            }
        }
    }

    pub(crate) fn user_data_dir(self) -> Option<PathBuf> {
        // macOS: ~/Library/Application Support; Windows: %LOCALAPPDATA%;
        // Linux: ~/.config. The browser always runs where the app runs, even
        // for a WSL Home: Pingex on Windows drives the Windows browser.
        let base = if cfg!(windows) {
            dirs::data_local_dir()
        } else {
            dirs::config_dir()
        }?;
        Some(base.join(self.relative_dir()))
    }
}

/// Where a running browser's DevTools endpoint is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Endpoint {
    pub channel: BrowserChannel,
    pub port: u16,
    pub path: String,
}

impl Endpoint {
    pub(crate) fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}{}", self.port, self.path)
    }
}

/// Parse a `DevToolsActivePort` file: the port on the first line, the
/// browser target's WebSocket path on the second.
pub(crate) fn parse_active_port(contents: &str) -> Option<(u16, String)> {
    let mut lines = contents.lines().map(str::trim);
    let port = lines.next()?.parse::<u16>().ok().filter(|port| *port > 0)?;
    let path = lines
        .next()
        .filter(|path| path.starts_with("/devtools/browser/"))?;
    Some((port, path.to_string()))
}

pub(crate) fn read_endpoint(channel: BrowserChannel, dir: &Path) -> Option<Endpoint> {
    let contents = std::fs::read_to_string(dir.join("DevToolsActivePort")).ok()?;
    let (port, path) = parse_active_port(&contents)?;
    Some(Endpoint {
        channel,
        port,
        path,
    })
}

/// Every browser that currently advertises a DevTools endpoint, in
/// preference order: the chosen channel first, then [`BrowserChannel::ALL`].
/// A stale file (the browser quit without removing it) still shows up here;
/// connecting is what tells the two apart.
pub(crate) fn discover(preferred: Option<BrowserChannel>) -> Vec<Endpoint> {
    let order = preferred.into_iter().chain(
        BrowserChannel::ALL
            .into_iter()
            .filter(|c| Some(*c) != preferred),
    );
    order
        .filter_map(|channel| {
            let dir = channel.user_data_dir()?;
            read_endpoint(channel, &dir)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_port_files_parse_port_and_browser_path() {
        assert_eq!(
            parse_active_port("9222\n/devtools/browser/91cd8ff6-4d68\n"),
            Some((9222, "/devtools/browser/91cd8ff6-4d68".to_string()))
        );
        assert_eq!(
            parse_active_port("9222\r\n/devtools/browser/x\r\n")
                .unwrap()
                .0,
            9222
        );
        assert_eq!(parse_active_port(""), None);
        assert_eq!(parse_active_port("0\n/devtools/browser/x"), None);
        assert_eq!(parse_active_port("9222\n/json/version"), None);
        assert_eq!(parse_active_port("abc\n/devtools/browser/x"), None);
    }

    #[test]
    fn endpoints_are_read_from_a_user_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_endpoint(BrowserChannel::Chrome, dir.path()), None);
        std::fs::write(
            dir.path().join("DevToolsActivePort"),
            "53110\n/devtools/browser/abc",
        )
        .unwrap();
        let endpoint = read_endpoint(BrowserChannel::Brave, dir.path()).unwrap();
        assert_eq!(
            endpoint.ws_url(),
            "ws://127.0.0.1:53110/devtools/browser/abc"
        );
        assert_eq!(endpoint.channel, BrowserChannel::Brave);
    }
}
