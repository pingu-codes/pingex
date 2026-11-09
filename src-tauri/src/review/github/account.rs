//! Which signed-in GitHub account a repository's `gh` commands run as.
//!
//! A pin the user chose wins, then a signed-in login that owns one of the
//! clone's GitHub remotes, then gh's active account. The chosen account's
//! token goes to `gh` through one process's environment, so the active account,
//! and with it `gh` in the user's terminal, stays as it was. A token set in
//! the environment outranks all of this: gh would use it regardless.

use std::fmt;
use std::sync::LazyLock;
use std::time::Duration;

use turso::Database;

use super::repos;
use super::run::{redact_gh_error, run_gh, Gh, READ_TIMEOUT};
use crate::git::remotes::{read_remotes, GitRemote};
use crate::review::repository::account_key;
use crate::review::types::{AccountSource, GitHubAccount, RepositoryAccount};
use crate::storage;
use crate::util::host::Host;
use crate::util::ttl_cache::TtlCache;

/// A token from `gh auth token`. It can reach `gh`'s environment and nowhere
/// else: no `Display`, no `Serialize`, and `Debug` hides it.
pub(crate) struct GhToken(String);

impl GhToken {
    pub(super) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for GhToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GhToken(<redacted>)")
    }
}

/// What one command runs as.
pub(super) struct Scope {
    pub(super) login: String,
    pub(super) token: Option<GhToken>,
}

/// A repository's account, before any token is looked up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Resolved {
    pub(crate) login: Option<String>,
    pub(crate) source: AccountSource,
    /// False when gh would use this account anyway.
    pub(crate) inject: bool,
}

fn signed_in<'a>(account: &'a GitHubAccount, login: &str) -> Option<&'a str> {
    account
        .logins
        .iter()
        .find(|known| known.eq_ignore_ascii_case(login))
        .map(String::as_str)
}

/// The resolution order. `owners` are in `owners_in_gh_order`'s order.
pub(crate) fn resolve_account(
    pin: Option<&str>,
    owners: &[String],
    account: &GitHubAccount,
) -> Resolved {
    let active = account.login.as_deref();
    let global = |source| Resolved {
        login: account.login.clone(),
        source,
        inject: false,
    };
    let chosen = |login: &str, source| Resolved {
        login: Some(login.to_string()),
        source,
        inject: active != Some(login),
    };
    if matches!(
        account.token_source.as_deref(),
        Some("GH_TOKEN" | "GITHUB_TOKEN")
    ) {
        return global(AccountSource::Environment);
    }
    if let Some(pin) = pin {
        return match signed_in(account, pin) {
            Some(login) => chosen(login, AccountSource::Pinned),
            None => Resolved {
                login: Some(pin.to_string()),
                source: AccountSource::PinMissing,
                inject: false,
            },
        };
    }
    owners
        .iter()
        .find_map(|owner| signed_in(account, owner))
        .map(|login| chosen(login, AccountSource::Owner))
        .unwrap_or_else(|| global(AccountSource::Global))
}

/// The owners of the clone's GitHub remotes, in the order gh picks a base
/// repository from them: `upstream`, `github`, `origin`, then the rest.
pub(crate) fn owners_in_gh_order(remotes: &[GitRemote]) -> Vec<String> {
    let rank = |remote: &GitRemote| match remote.name.as_str() {
        "upstream" => 0,
        "github" => 1,
        "origin" => 2,
        _ => 3,
    };
    let mut ordered: Vec<&GitRemote> = remotes.iter().collect();
    ordered.sort_by_key(|remote| rank(remote));
    let mut owners: Vec<String> = Vec::new();
    for remote in ordered {
        let owner = remote
            .slug
            .as_deref()
            .and_then(|slug| slug.split('/').next());
        if let Some(owner) = owner.filter(|owner| !owners.iter().any(|known| known == owner)) {
            owners.push(owner.to_string());
        }
    }
    owners
}

/// `gh auth status` checks each account's token over the network, and every
/// command needs its answer. It can only go stale in who owns a repository:
/// a pin or token lookup that fails reads it again.
static ACCOUNTS: LazyLock<TtlCache<Host, GitHubAccount>> =
    LazyLock::new(|| TtlCache::new(Duration::from_secs(5 * 60), 16));

/// Read the accounts now, for whatever reads them next. Nobody signed in isn't
/// worth remembering: the next `gh auth login` should count at once.
pub(super) fn fresh_account(global: &Gh<'_>) -> Result<GitHubAccount, String> {
    let account = repos::account(global)?;
    if !account.logins.is_empty() {
        ACCOUNTS.insert(global.host.clone(), account.clone());
    }
    Ok(account)
}

fn cached_account(global: &Gh<'_>) -> Result<GitHubAccount, String> {
    match ACCOUNTS.get(global.host) {
        Some(account) => Ok(account),
        None => fresh_account(global),
    }
}

fn token_for(global: &Gh<'_>, login: &str) -> Result<GhToken, String> {
    let failed = format!("Could not use the GitHub account {login}: run `gh auth login`");
    let output = run_gh(
        global,
        &["auth", "token", "--hostname", "github.com", "--user", login],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err(redact_gh_error(&failed, &output));
    }
    let token = output.stdout.trim();
    if token.is_empty() {
        return Err(failed);
    }
    Ok(GhToken(token.to_string()))
}

fn remote_owners(global: &Gh<'_>) -> Vec<String> {
    read_remotes(global.host, global.dir)
        .map(|remotes| owners_in_gh_order(&remotes))
        .unwrap_or_default()
}

fn scope_for(
    global: &Gh<'_>,
    pin: Option<&str>,
    owners: &[String],
    account: &GitHubAccount,
) -> Result<Scope, String> {
    let resolved = resolve_account(pin, owners, account);
    let login = resolved.login.unwrap_or_default();
    if resolved.source == AccountSource::PinMissing {
        return Err(format!(
            "This repository is set to use {login}, who is not signed in to gh. Choose another account or run `gh auth login`."
        ));
    }
    let token = if resolved.inject {
        Some(token_for(global, &login)?)
    } else {
        None
    };
    Ok(Scope { login, token })
}

/// The repository's pin. The read is async and this runs behind a sync trait,
/// possibly on the async runtime, where `block_on` panics: a thread of its
/// own is safe from anywhere.
fn read_pin(global: &Gh<'_>, database: &Database) -> Result<Option<String>, String> {
    let key = account_key(global.host, global.dir);
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                tauri::async_runtime::block_on(storage::read_repository_account(database, &key))
            })
            .join()
    })
    .map_err(|_| "Could not read the repository's GitHub account".to_string())?
}

/// What the repository's commands should run as. A stale list of accounts
/// shouldn't be why this fails, so a failure reads them again before giving up.
pub(super) fn open_scope(global: &Gh<'_>, database: &Database) -> Result<Scope, String> {
    let pin = read_pin(global, database)?;
    let owners = remote_owners(global);
    let cached = cached_account(global)?;
    match scope_for(global, pin.as_deref(), &owners, &cached) {
        Ok(scope) => Ok(scope),
        Err(_) => {
            let fresh = fresh_account(global)?;
            scope_for(global, pin.as_deref(), &owners, &fresh)
        }
    }
}

/// The answer the account menu shows, from `pin` and a fresh read of the
/// accounts. A pin whose account has gone still answers here, so it can be
/// undone.
pub(super) fn repository_account(
    global: &Gh<'_>,
    pin: Option<&str>,
) -> Result<RepositoryAccount, String> {
    let account = fresh_account(global)?;
    Ok(describe_account(pin, &remote_owners(global), account))
}

pub(crate) fn describe_account(
    pin: Option<&str>,
    owners: &[String],
    account: GitHubAccount,
) -> RepositoryAccount {
    let resolved = resolve_account(pin, owners, &account);
    RepositoryAccount {
        login: resolved.login,
        source: resolved.source,
        pin: pin.map(str::to_string),
        logins: account.logins,
        global: account.login,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(active: &str, logins: &[&str]) -> GitHubAccount {
        GitHubAccount {
            login: Some(active.to_string()),
            token_source: Some("keyring".to_string()),
            logins: logins.iter().map(|login| login.to_string()).collect(),
        }
    }

    fn two() -> GitHubAccount {
        account("work", &["Personal", "work"])
    }

    fn owners(owners: &[&str]) -> Vec<String> {
        owners.iter().map(|owner| owner.to_string()).collect()
    }

    fn remote(name: &str, slug: Option<&str>) -> GitRemote {
        GitRemote {
            name: name.to_string(),
            url: String::new(),
            slug: slug.map(str::to_string),
        }
    }

    #[test]
    fn a_pin_wins_over_the_owner() {
        let resolved = resolve_account(Some("work"), &owners(&["personal"]), &two());
        assert_eq!(resolved.login.as_deref(), Some("work"));
        assert_eq!(resolved.source, AccountSource::Pinned);
    }

    #[test]
    fn a_pin_to_an_account_that_has_gone_is_not_replaced() {
        let resolved = resolve_account(Some("gone"), &owners(&["personal"]), &two());
        assert_eq!(resolved.login.as_deref(), Some("gone"));
        assert_eq!(resolved.source, AccountSource::PinMissing);
        assert!(!resolved.inject);
    }

    #[test]
    fn the_owner_matches_ignoring_case_and_keeps_gh_s_spelling() {
        let resolved = resolve_account(None, &owners(&["personal"]), &two());
        assert_eq!(resolved.login.as_deref(), Some("Personal"));
        assert_eq!(resolved.source, AccountSource::Owner);
        assert!(resolved.inject);
    }

    #[test]
    fn a_fork_resolves_to_whichever_remote_owner_is_signed_in() {
        let resolved = resolve_account(None, &owners(&["openai", "personal"]), &two());
        assert_eq!(resolved.login.as_deref(), Some("Personal"));
    }

    #[test]
    fn an_unknown_owner_falls_back_to_the_global_account_without_a_token() {
        let resolved = resolve_account(None, &owners(&["acme"]), &two());
        assert_eq!(resolved.login.as_deref(), Some("work"));
        assert_eq!(resolved.source, AccountSource::Global);
        assert!(!resolved.inject);
    }

    #[test]
    fn no_token_is_needed_when_gh_would_use_that_account_anyway() {
        for pin in [None, Some("work")] {
            assert!(!resolve_account(pin, &owners(&["work"]), &two()).inject);
        }
    }

    #[test]
    fn a_token_in_the_environment_beats_everything() {
        for source in ["GH_TOKEN", "GITHUB_TOKEN"] {
            let mut env = two();
            env.token_source = Some(source.to_string());
            let resolved = resolve_account(Some("Personal"), &owners(&["personal"]), &env);
            assert_eq!(resolved.login.as_deref(), Some("work"));
            assert_eq!(resolved.source, AccountSource::Environment);
            assert!(!resolved.inject);
        }
    }

    #[test]
    fn signed_out_resolves_to_nobody() {
        let resolved = resolve_account(None, &owners(&["acme"]), &GitHubAccount::default());
        assert_eq!(resolved.login, None);
        assert_eq!(resolved.source, AccountSource::Global);
        assert!(!resolved.inject);
    }

    #[test]
    fn owners_follow_gh_s_remote_order_once_each() {
        let remotes = [
            remote("backup", Some("backup-org/repo")),
            remote("origin", Some("personal/repo")),
            remote("local", None),
            remote("github", Some("personal/repo")),
            remote("upstream", Some("openai/repo")),
        ];
        assert_eq!(
            owners_in_gh_order(&remotes),
            ["openai", "personal", "backup-org"]
        );
    }

    #[test]
    fn a_token_never_shows_in_debug_output() {
        let token = GhToken("gho_secret".to_string());
        assert!(!format!("{token:?}").contains("gho_secret"));
    }

    #[test]
    fn the_menu_still_answers_for_a_pin_whose_account_has_gone() {
        let answer = describe_account(Some("gone"), &[], two());
        assert_eq!(answer.source, AccountSource::PinMissing);
        assert_eq!(answer.pin.as_deref(), Some("gone"));
        assert_eq!(answer.global.as_deref(), Some("work"));
        assert_eq!(answer.logins, ["Personal", "work"]);
    }
}
