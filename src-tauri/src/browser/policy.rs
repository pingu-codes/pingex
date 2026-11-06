//! What an agent may do in the user's browser.
//!
//! Inside a tab the agent owns, anything goes: the high-level tools and raw
//! CDP alike pass through, because owning the tab is the grant. Reaching
//! outside it (other tabs, every site's cookies, the browser itself) needs
//! the thread's Full browser access. Navigations are policed separately, by
//! the per-tab gate in `tab.rs`, so that a navigation started from page
//! JavaScript is caught too, not just one started by a tool.

use serde_json::Value;

/// The part of a URL a site grant is about. `None` means the URL has no site
/// of its own and is always allowed (`about:blank`, `data:`, `blob:`).
pub(crate) fn site_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once(':')?;
    let scheme = scheme.to_ascii_lowercase();
    match scheme.as_str() {
        "about" | "data" | "blob" | "javascript" => None,
        "file" => Some("file://".into()),
        "http" | "https" | "ws" | "wss" => {
            let rest = rest.strip_prefix("//")?;
            let authority = rest.split(['/', '?', '#']).next()?;
            let host_port = authority.rsplit('@').next()?;
            let host_port = host_port.to_ascii_lowercase();
            let (host, port) = match host_port.strip_prefix('[') {
                Some(v6) => {
                    let (host, after) = v6.split_once(']')?;
                    (format!("[{host}]"), after.strip_prefix(':'))
                }
                None => match host_port.rsplit_once(':') {
                    Some((host, port)) => (host.to_string(), Some(port)),
                    None => (host_port.clone(), None),
                },
            };
            let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
            if host.is_empty() {
                return None;
            }
            let default_port = matches!(
                (scheme.as_str(), port),
                (_, None) | ("http" | "ws", Some("80")) | ("https" | "wss", Some("443"))
            );
            Some(match port {
                Some(port) if !default_port => format!("{host}:{port}"),
                _ => host,
            })
        }
        // chrome://settings and friends are sensitive in their own right.
        other => {
            let host = rest.trim_start_matches('/').split(['/', '?', '#']).next()?;
            Some(format!("{other}://{host}"))
        }
    }
}

/// Whether a grant for `granted` covers `site`: the same site, or a
/// subdomain of it (a grant for `github.com` covers `gist.github.com`).
pub(crate) fn grant_covers(granted: &str, site: &str) -> bool {
    site == granted
        || (site.len() > granted.len()
            && site.ends_with(granted)
            && site.as_bytes()[site.len() - granted.len() - 1] == b'.')
}

/// Where a raw CDP call is aimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallTarget {
    /// An owned tab's session.
    Tab,
    /// The browser target itself.
    Browser,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    Allow,
    /// Needs the thread's Full browser access; the string says why.
    NeedsFullAccess(String),
    /// Needs a one-off approval from the user.
    NeedsApproval(String),
}

/// Raw CDP calls that only Pingex itself may make: they would let the agent
/// see or attach to tabs it does not own, or switch off the navigation gate.
const TAB_SESSION_REFUSED: &[&str] = &[
    "Target.setDiscoverTargets",
    "Target.getTargets",
    "Target.attachToTarget",
    "Target.attachToBrowserTarget",
    "Target.activateTarget",
    "Target.closeTarget",
    "Target.createTarget",
    "Target.createBrowserContext",
    "Target.disposeBrowserContext",
    "Target.exposeDevToolsProtocol",
];

/// Raw CDP calls on an owned tab's session that still reach every site.
const TAB_SESSION_BROWSER_WIDE: &[&str] = &[
    "Network.getAllCookies",
    "Network.clearBrowserCookies",
    "Network.clearBrowserCache",
    "Storage.getCookies",
    "Storage.setCookies",
    "Storage.clearCookies",
    "Browser.setDownloadBehavior",
    "Page.setDownloadBehavior",
];

/// Check a raw `cdp` call. `site_granted` answers whether the thread may act
/// on a site, for calls that name sites in their params.
pub(crate) fn check_raw(
    target: CallTarget,
    method: &str,
    params: &Value,
    full_access: bool,
    site_granted: &dyn Fn(&str) -> bool,
) -> Verdict {
    if target == CallTarget::Browser {
        return if full_access {
            Verdict::Allow
        } else {
            Verdict::NeedsFullAccess(format!(
                "{method} on the browser target reaches every tab and site"
            ))
        };
    }
    if full_access {
        return Verdict::Allow;
    }
    if TAB_SESSION_REFUSED.contains(&method) {
        return Verdict::NeedsFullAccess(format!(
            "{method} reaches tabs this thread does not own; use the `tabs`, `open`, `take` and `close` tools instead"
        ));
    }
    if TAB_SESSION_BROWSER_WIDE.contains(&method) {
        return Verdict::NeedsFullAccess(format!("{method} reaches every site"));
    }
    if method == "DOM.setFileInputFiles" {
        let files = params
            .get("files")
            .and_then(Value::as_array)
            .map(|files| {
                files
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        return Verdict::NeedsApproval(format!("upload {files}"));
    }
    // Cookie reads and writes that name URLs or domains must stay on
    // granted sites.
    if matches!(
        method,
        "Network.getCookies" | "Network.setCookie" | "Network.setCookies" | "Network.deleteCookies"
    ) {
        for site in named_sites(params) {
            if !site_granted(&site) {
                return Verdict::NeedsFullAccess(format!(
                    "{method} names {site}, which this thread has no grant for"
                ));
            }
        }
    }
    if method == "Storage.clearDataForOrigin" || method == "Storage.getStorageKeyForFrame" {
        if let Some(site) = params
            .get("origin")
            .and_then(Value::as_str)
            .and_then(site_of)
        {
            if !site_granted(&site) {
                return Verdict::NeedsFullAccess(format!(
                    "{method} names {site}, which this thread has no grant for"
                ));
            }
        }
    }
    Verdict::Allow
}

/// Sites named by a cookie call: `urls`, `url`, `domain`, or the entries of
/// `cookies`.
fn named_sites(params: &Value) -> Vec<String> {
    fn add(sites: &mut Vec<String>, value: &Value) {
        if let Some(url) = value.get("url").and_then(Value::as_str) {
            sites.extend(site_of(url));
        }
        if let Some(domain) = value.get("domain").and_then(Value::as_str) {
            let domain = domain.trim_start_matches('.').to_ascii_lowercase();
            let domain = domain.strip_prefix("www.").unwrap_or(&domain).to_string();
            if !domain.is_empty() {
                sites.push(domain);
            }
        }
    }
    let mut sites = Vec::new();
    add(&mut sites, params);
    if let Some(urls) = params.get("urls").and_then(Value::as_array) {
        sites.extend(urls.iter().filter_map(Value::as_str).filter_map(site_of));
    }
    for cookie in params
        .get("cookies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        add(&mut sites, cookie);
    }
    sites
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sites_are_hosts_without_www_or_default_ports() {
        assert_eq!(
            site_of("https://www.GitHub.com/x?y#z").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            site_of("http://localhost:5173/").as_deref(),
            Some("localhost:5173")
        );
        assert_eq!(
            site_of("https://example.com:443/").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            site_of("http://user:pw@host.test:80").as_deref(),
            Some("host.test")
        );
        assert_eq!(site_of("http://[::1]:8080/").as_deref(), Some("[::1]:8080"));
        assert_eq!(site_of("file:///tmp/a.html").as_deref(), Some("file://"));
        assert_eq!(
            site_of("chrome://settings/passwords").as_deref(),
            Some("chrome://settings")
        );
        assert_eq!(site_of("about:blank"), None);
        assert_eq!(site_of("data:text/html,hi"), None);
        assert_eq!(site_of("not a url"), None);
    }

    #[test]
    fn grants_cover_subdomains_but_not_lookalikes() {
        assert!(grant_covers("github.com", "github.com"));
        assert!(grant_covers("github.com", "gist.github.com"));
        assert!(!grant_covers("github.com", "evilgithub.com"));
        assert!(!grant_covers("gist.github.com", "github.com"));
        assert!(!grant_covers("localhost:5173", "localhost:3000"));
    }

    fn granted(site: &str) -> bool {
        grant_covers("example.com", site)
    }

    #[test]
    fn owned_tabs_pass_raw_cdp_through() {
        for method in [
            "Runtime.evaluate",
            "Page.navigate",
            "Emulation.setDeviceMetricsOverride",
            "Fetch.enable",
            "Network.enable",
            "Debugger.enable",
            "Tracing.start",
            "Target.setAutoAttach",
            "Input.dispatchMouseEvent",
        ] {
            assert_eq!(
                check_raw(CallTarget::Tab, method, &json!({}), false, &granted),
                Verdict::Allow,
                "{method}"
            );
        }
    }

    #[test]
    fn reaching_outside_owned_tabs_needs_full_access() {
        for method in [
            "Target.getTargets",
            "Target.attachToTarget",
            "Network.getAllCookies",
            "Storage.getCookies",
        ] {
            assert!(matches!(
                check_raw(CallTarget::Tab, method, &json!({}), false, &granted),
                Verdict::NeedsFullAccess(_)
            ));
            assert_eq!(
                check_raw(CallTarget::Tab, method, &json!({}), true, &granted),
                Verdict::Allow
            );
        }
        assert!(matches!(
            check_raw(
                CallTarget::Browser,
                "Browser.getVersion",
                &json!({}),
                false,
                &granted
            ),
            Verdict::NeedsFullAccess(_)
        ));
        assert_eq!(
            check_raw(
                CallTarget::Browser,
                "Browser.getVersion",
                &json!({}),
                true,
                &granted
            ),
            Verdict::Allow
        );
    }

    #[test]
    fn cookie_calls_stay_on_granted_sites() {
        let own = json!({"urls": ["https://example.com/a", "https://api.example.com"]});
        assert_eq!(
            check_raw(CallTarget::Tab, "Network.getCookies", &own, false, &granted),
            Verdict::Allow
        );
        let other = json!({"urls": ["https://bank.test/"]});
        assert!(matches!(
            check_raw(
                CallTarget::Tab,
                "Network.getCookies",
                &other,
                false,
                &granted
            ),
            Verdict::NeedsFullAccess(_)
        ));
        let set = json!({"cookies": [{"name": "a", "value": "b", "domain": ".bank.test"}]});
        assert!(matches!(
            check_raw(CallTarget::Tab, "Network.setCookies", &set, false, &granted),
            Verdict::NeedsFullAccess(_)
        ));
        // No urls: the current page's cookies, which the tab already owns.
        assert_eq!(
            check_raw(
                CallTarget::Tab,
                "Network.getCookies",
                &json!({}),
                false,
                &granted
            ),
            Verdict::Allow
        );
    }

    #[test]
    fn file_uploads_need_an_approval_each_time() {
        let upload = json!({"files": ["/Users/me/a.pdf"], "backendNodeId": 4});
        assert_eq!(
            check_raw(
                CallTarget::Tab,
                "DOM.setFileInputFiles",
                &upload,
                false,
                &granted
            ),
            Verdict::NeedsApproval("upload /Users/me/a.pdf".into())
        );
        assert_eq!(
            check_raw(
                CallTarget::Tab,
                "DOM.setFileInputFiles",
                &upload,
                true,
                &granted
            ),
            Verdict::Allow
        );
    }
}
