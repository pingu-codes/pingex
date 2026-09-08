//! Where a Home's harness and repositories live: the machine Pingex runs on,
//! or a WSL distribution reached through `wsl.exe`.
//!
//! A Host owns the three things that differ between the two: how a process is
//! spawned, how a *host path* (what the harness sees, `/home/u/repo`) maps to a
//! *local path* (what `std::fs` on the app's own OS can open,
//! `\\wsl.localhost\Ubuntu\home\u\repo`), and how a binary is located. Nothing
//! else in the app branches on the host kind; it asks the Host.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use super::process::{run, Run};

/// The Windows-side launcher for a distribution.
pub(crate) const WSL_EXE: &str = "wsl.exe";
/// `env(1)` inside the distribution, by absolute path so it resolves even
/// before the user's PATH exists (`wsl.exe --exec` runs no shell).
const WSL_ENV: &str = "/usr/bin/env";
/// The prefix a WSL home key carries so it never collides with a native one.
const WSL_KEY_PREFIX: &str = "wsl:";
/// Install locations checked after the login-shell PATH inside a distribution;
/// a `sh -l` that reads only `.profile` misses what `.bashrc` adds.
const WSL_FALLBACK_DIRS: [&str; 8] = [
    "$HOME/.local/bin",
    "$HOME/.claude/local",
    "$HOME/.npm-global/bin",
    "$HOME/.bun/bin",
    "$HOME/.cargo/bin",
    "$HOME/.volta/bin",
    "/usr/local/bin",
    "/usr/bin",
];
/// How long a lookup inside a distribution may take. Starting a stopped
/// distribution is the slow case; a hung shell must not hang the app.
const WSL_LOOKUP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type, Default)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Host {
    #[default]
    Native,
    Wsl {
        distro: String,
    },
}

impl Host {
    pub fn wsl(distro: impl Into<String>) -> Self {
        Host::Wsl {
            distro: distro.into(),
        }
    }

    pub fn is_wsl(&self) -> bool {
        matches!(self, Host::Wsl { .. })
    }

    /// Short human label for badges and messages.
    pub fn label(&self) -> String {
        match self {
            Host::Native => "native".to_string(),
            Host::Wsl { distro } => format!("WSL · {distro}"),
        }
    }

    /// The full argument vector that [`Host::command`] spawns, program first.
    /// Public so the live suites and the unit tests assert the exact argv
    /// without spawning anything.
    pub fn argv(
        &self,
        program: &str,
        args: &[&str],
        cwd: Option<&str>,
        env: &[(&str, &str)],
        unset: &[&str],
    ) -> Vec<String> {
        match self {
            Host::Native => std::iter::once(program)
                .chain(args.iter().copied())
                .map(str::to_string)
                .collect(),
            Host::Wsl { distro } => {
                let mut argv = vec![WSL_EXE.to_string(), "-d".to_string(), distro.clone()];
                if let Some(cwd) = cwd {
                    argv.push("--cd".to_string());
                    argv.push(cwd.to_string());
                }
                // `--exec` runs the program directly; without it wsl.exe joins
                // the arguments and hands them to the login shell, which
                // re-parses quotes and spaces.
                argv.push("--exec".to_string());
                argv.push(WSL_ENV.to_string());
                for name in unset {
                    argv.push("-u".to_string());
                    argv.push((*name).to_string());
                }
                for (key, value) in env {
                    argv.push(format!("{key}={value}"));
                }
                argv.push(program.to_string());
                argv.extend(args.iter().map(|arg| (*arg).to_string()));
                argv
            }
        }
    }

    /// A `Command` that runs `program` on this host. `cwd` and every path in
    /// `env` are host paths. Nothing is spawned here.
    pub fn command(
        &self,
        program: &str,
        args: &[&str],
        cwd: Option<&str>,
        env: &[(&str, &str)],
        unset: &[&str],
    ) -> Command {
        match self {
            Host::Native => {
                let mut command = Command::new(program);
                command.args(args);
                if let Some(cwd) = cwd {
                    command.current_dir(cwd);
                }
                for (key, value) in env {
                    command.env(key, value);
                }
                for name in unset {
                    command.env_remove(name);
                }
                command
            }
            Host::Wsl { .. } => {
                let argv = self.argv(program, args, cwd, env, unset);
                let mut command = Command::new(&argv[0]);
                command.args(&argv[1..]);
                command
            }
        }
    }

    /// Append `segment` to a host path. A Linux path must never go through
    /// `Path::join` on Windows, which would insert a backslash.
    pub fn join_str(&self, base: &str, segment: &str) -> String {
        match self {
            Host::Native => Path::new(base).join(segment).display().to_string(),
            Host::Wsl { .. } => {
                let base = base.trim_end_matches('/');
                let segment = segment.trim_start_matches('/');
                if base.is_empty() {
                    format!("/{segment}")
                } else {
                    format!("{base}/{segment}")
                }
            }
        }
    }

    /// A host path that went through `PathBuf` on the way here, with the
    /// separators this host expects. On Windows `Path::join` inserts `\\`
    /// into a Linux path; the distribution wants `/`.
    pub fn path_string(&self, path: &Path) -> String {
        let text = path.to_string_lossy();
        match self {
            Host::Native => text.into_owned(),
            Host::Wsl { .. } => text.replace('\\', "/"),
        }
    }

    /// The parent of a host path, or `None` at the root.
    pub fn parent_str(&self, path: &str) -> Option<String> {
        match self {
            Host::Native => Path::new(path)
                .parent()
                .map(|parent| parent.to_string_lossy().into_owned()),
            Host::Wsl { .. } => {
                let trimmed = path.trim_end_matches('/');
                let index = trimmed.rfind('/')?;
                Some(if index == 0 {
                    "/".to_string()
                } else {
                    trimmed[..index].to_string()
                })
            }
        }
    }

    /// Whether `path` is `root` or lies beneath it, lexically.
    pub fn is_under(&self, root: &str, path: &str) -> bool {
        match self {
            Host::Native => Path::new(path).starts_with(Path::new(root)),
            Host::Wsl { .. } => {
                let root = root.trim_end_matches('/');
                path == root || path.starts_with(&format!("{root}/"))
            }
        }
    }

    /// The local path `std::fs` opens for a host path. Native is identity; a
    /// WSL `/mnt/<drive>/..` is the drive itself and anything else goes over
    /// the `\\wsl.localhost\<distro>` share.
    pub fn to_local(&self, host_path: &str) -> PathBuf {
        match self {
            Host::Native => PathBuf::from(host_path),
            Host::Wsl { distro } => {
                if let Some(drive_path) = mnt_to_drive(host_path) {
                    return PathBuf::from(drive_path);
                }
                let rest = host_path.trim_start_matches('/').replace('/', "\\");
                PathBuf::from(format!("\\\\wsl.localhost\\{distro}\\{rest}"))
            }
        }
    }

    /// Recognise a local path that lives in a distribution
    /// (`\\wsl.localhost\<distro>\..` or `\\wsl$\<distro>\..`). Anything else
    /// is a native path, drive letters included: a `C:\` pick is only a WSL
    /// path when the caller already knows the host (see
    /// [`Host::to_host_path`]).
    pub fn from_local(local: &str) -> (Host, String) {
        let normalised = local.replace('/', "\\");
        let stripped = normalised.trim_start_matches('\\');
        for share in ["wsl.localhost\\", "wsl$\\"] {
            if let Some(rest) = stripped.strip_prefix(share) {
                let (distro, path) = rest.split_once('\\').unwrap_or((rest, ""));
                if !distro.is_empty() {
                    let path = format!("/{}", path.replace('\\', "/"));
                    return (Host::wsl(distro), path);
                }
            }
        }
        (Host::Native, local.to_string())
    }

    /// Turn a local path (a dialog pick) into a host path for this host. On
    /// WSL a share path in this distribution becomes its Linux path and a
    /// drive path becomes `/mnt/<drive>/..`; a share path in another
    /// distribution is left alone, since the harness cannot reach it.
    pub fn to_host_path(&self, local: &str) -> String {
        match self {
            Host::Native => local.to_string(),
            Host::Wsl { distro } => {
                if let Some(path) = drive_to_mnt(local) {
                    return path;
                }
                match Host::from_local(local) {
                    (Host::Wsl { distro: found }, path) if &found == distro => path,
                    _ => local.to_string(),
                }
            }
        }
    }

    /// The registry key for a home on this host. Native keys are the canonical
    /// path unchanged, so existing keys and saved recents stay valid.
    pub fn home_key(&self, canonical: &str) -> String {
        match self {
            Host::Native => canonical.to_string(),
            Host::Wsl { distro } => format!("{WSL_KEY_PREFIX}{distro}:{canonical}"),
        }
    }

    /// Parse a short spec such as `native` or `wsl:Ubuntu`, as accepted by
    /// `PINGEX_CODEX_HOST`.
    pub fn parse_spec(spec: &str) -> Option<Host> {
        let spec = spec.trim();
        if spec.is_empty() || spec.eq_ignore_ascii_case("native") {
            return Some(Host::Native);
        }
        let distro = spec.strip_prefix(WSL_KEY_PREFIX)?.trim();
        (!distro.is_empty()).then(|| Host::wsl(distro))
    }

    /// Canonical form of a host path. Native uses the filesystem; WSL asks the
    /// distribution and falls back to a lexical clean-up when it cannot.
    pub fn canonical(&self, path: &str) -> String {
        match self {
            Host::Native => std::fs::canonicalize(path)
                .unwrap_or_else(|_| PathBuf::from(path))
                .display()
                .to_string(),
            // `-m` resolves what exists and keeps the missing tail, like the
            // lenient canonicalisation the worktree classifiers need.
            Host::Wsl { .. } => self
                .command("readlink", &["-m", "--", path], None, &[], &[])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                .filter(|resolved| resolved.starts_with('/'))
                .unwrap_or_else(|| normalise_posix(path)),
        }
    }

    /// Whether a host path is a directory, without spawning: the share is
    /// readable from Windows, so this is one metadata call either way.
    pub fn is_dir(&self, host_path: &str) -> bool {
        self.to_local(host_path).is_dir()
    }

    /// Resolve a configured binary to an absolute path the host can execute.
    /// Native goes through the PATH-and-fallbacks search; WSL asks a login
    /// shell in the distribution (a GUI-launched `wsl.exe` has no login PATH,
    /// so bare names are useless at spawn time) and remembers hits.
    pub fn resolve_binary(&self, configured: &str) -> Option<String> {
        let configured = configured.trim();
        if configured.is_empty() {
            return None;
        }
        match self {
            Host::Native => crate::codex::binary::resolve(Path::new(configured))
                .map(|path| path.display().to_string()),
            Host::Wsl { distro } => {
                let key = (distro.clone(), configured.to_string());
                if let Some(hit) = binary_cache()
                    .lock()
                    .ok()
                    .and_then(|c| c.get(&key).cloned())
                {
                    return Some(hit);
                }
                let script = wsl_resolve_script(configured);
                // A login `sh` sees `.profile`; version managers (fnm, nvm)
                // usually export their PATH from `.bashrc`, which only an
                // interactive bash reads, so that is the second attempt.
                let found = self
                    .shell_lookup("sh", &["-lc", &script])
                    .or_else(|| self.shell_lookup("bash", &["-lic", &script]))?;
                if let Ok(mut cache) = binary_cache().lock() {
                    cache.insert(key, found.clone());
                }
                Some(found)
            }
        }
    }

    /// Run a shell inside the distribution, with a timeout, and return the
    /// absolute path it printed, if any.
    fn shell_lookup(&self, shell: &str, args: &[&str]) -> Option<String> {
        let output = run(Run::on(self, shell, "/", args, WSL_LOOKUP_TIMEOUT)).ok()?;
        if !output.ok {
            return None;
        }
        let path = output.stdout.trim().to_string();
        path.starts_with('/').then_some(path)
    }

    /// Forget every remembered WSL binary location, e.g. after the user edits
    /// a binary path in Settings.
    pub fn clear_binary_cache() {
        if let Ok(mut cache) = binary_cache().lock() {
            cache.clear();
        }
    }

    /// The distribution's `$HOME`, when it can be asked.
    pub fn home_dir(&self) -> Option<String> {
        match self {
            Host::Native => dirs::home_dir().map(|home| home.display().to_string()),
            Host::Wsl { .. } => self
                .command("sh", &["-lc", "printf %s \"$HOME\""], None, &[], &[])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                .filter(|home| home.starts_with('/')),
        }
    }
}

/// The installed WSL distributions, for the host pickers. Empty anywhere
/// `wsl.exe` does not exist.
pub fn list_distros() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    Command::new(WSL_EXE)
        .args(["-l", "-q"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| parse_distro_list(&output.stdout))
        .unwrap_or_default()
}

/// `wsl.exe -l -q` prints UTF-16LE with a BOM; older builds print UTF-8.
pub fn parse_distro_list(bytes: &[u8]) -> Vec<String> {
    let text =
        if bytes.starts_with(&[0xFF, 0xFE]) || bytes.iter().skip(1).step_by(2).any(|b| *b == 0) {
            let units: Vec<u16> = bytes
                .chunks(2)
                .filter(|pair| pair.len() == 2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            String::from_utf8_lossy(bytes).into_owned()
        };
    text.lines()
        .map(|line| line.trim_matches(|c: char| c == '\u{FEFF}' || c == '\0' || c.is_whitespace()))
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// PIDs of `codex app-server` processes whose parent is gone (ppid 1), from
/// `ps -axo pid=,ppid=,command=` output.
pub fn orphan_pids(ps_output: &str) -> Vec<String> {
    ps_output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pid = parts.next()?;
            let ppid = parts.next()?;
            let arg0 = parts.next()?;
            let arg1 = parts.next();
            (ppid == "1" && arg0.ends_with("codex") && arg1 == Some("app-server"))
                .then(|| pid.to_string())
        })
        .collect()
}

fn binary_cache() -> &'static Mutex<HashMap<(String, String), String>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, String), String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Single-quote a value for `sh`.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// The `sh -lc` script that prints where `configured` lives: an explicit path
/// is checked as is (with `~` expanded by the shell), a bare name is looked up
/// on the login PATH and then in the usual install directories.
fn wsl_resolve_script(configured: &str) -> String {
    if configured.starts_with('~') {
        let rest = configured.trim_start_matches('~');
        return format!(
            "p=\"$HOME\"{}; [ -x \"$p\" ] && printf %s \"$p\"",
            shell_quote(rest)
        );
    }
    if configured.contains('/') {
        let quoted = shell_quote(configured);
        return format!("[ -x {quoted} ] && printf %s {quoted}");
    }
    let name = shell_quote(configured);
    let dirs = WSL_FALLBACK_DIRS
        .iter()
        .map(|dir| format!("\"{dir}\""))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "found=$(command -v {name} 2>/dev/null) && [ -n \"$found\" ] && {{ printf %s \"$found\"; exit 0; }}; \
         for d in {dirs}; do [ -x \"$d/\"{name} ] && {{ printf %s \"$d/\"{name}; exit 0; }}; done; exit 1"
    )
}

/// `/mnt/c/Users/x` → `C:\Users\x`. Only single-letter mounts count.
fn mnt_to_drive(host_path: &str) -> Option<String> {
    let rest = host_path.strip_prefix("/mnt/")?;
    let mut chars = rest.chars();
    let drive = chars.next().filter(|c| c.is_ascii_alphabetic())?;
    let tail = chars.as_str();
    if !(tail.is_empty() || tail.starts_with('/')) {
        return None;
    }
    let tail = tail.trim_start_matches('/').replace('/', "\\");
    Some(if tail.is_empty() {
        format!("{}:\\", drive.to_ascii_uppercase())
    } else {
        format!("{}:\\{tail}", drive.to_ascii_uppercase())
    })
}

/// `C:\Users\x` → `/mnt/c/Users/x`.
fn drive_to_mnt(local: &str) -> Option<String> {
    let mut chars = local.chars();
    let drive = chars.next().filter(|c| c.is_ascii_alphabetic())?;
    if chars.next() != Some(':') {
        return None;
    }
    let tail = chars.as_str();
    if !(tail.is_empty() || tail.starts_with('\\') || tail.starts_with('/')) {
        return None;
    }
    let tail = tail.trim_start_matches(['\\', '/']).replace('\\', "/");
    Some(if tail.is_empty() {
        format!("/mnt/{}", drive.to_ascii_lowercase())
    } else {
        format!("/mnt/{}/{tail}", drive.to_ascii_lowercase())
    })
}

/// Lexical clean-up of a POSIX path: collapse `//`, drop `.`, resolve `..`,
/// strip a trailing `/`.
fn normalise_posix(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_argv_is_the_program_and_its_arguments() {
        let argv = Host::Native.argv("codex", &["app-server"], Some("/x"), &[("A", "1")], &["B"]);
        assert_eq!(argv, vec!["codex", "app-server"]);
    }

    #[test]
    fn wsl_argv_wraps_in_wsl_exe_with_env_and_cwd() {
        let argv = Host::wsl("Ubuntu").argv(
            "/usr/bin/codex",
            &["app-server", "--stdio"],
            Some("/home/u/repo"),
            &[("CODEX_HOME", "/home/u/.codex")],
            &["ANTHROPIC_API_KEY"],
        );
        assert_eq!(
            argv,
            vec![
                "wsl.exe",
                "-d",
                "Ubuntu",
                "--cd",
                "/home/u/repo",
                "--exec",
                "/usr/bin/env",
                "-u",
                "ANTHROPIC_API_KEY",
                "CODEX_HOME=/home/u/.codex",
                "/usr/bin/codex",
                "app-server",
                "--stdio",
            ]
        );
    }

    #[test]
    fn wsl_argv_without_cwd_has_no_cd_flag() {
        let argv = Host::wsl("Ubuntu").argv("codex", &[], None, &[], &[]);
        assert!(!argv.contains(&"--cd".to_string()));
        assert_eq!(argv[3], "--exec");
    }

    #[test]
    fn join_never_uses_a_backslash_on_wsl() {
        let host = Host::wsl("Ubuntu");
        assert_eq!(
            host.join_str("/home/u/.codex", "worktrees"),
            "/home/u/.codex/worktrees"
        );
        assert_eq!(
            host.join_str("/home/u/.codex/", "/pingex.db"),
            "/home/u/.codex/pingex.db"
        );
        assert_eq!(host.path_string(Path::new("/home/u\\repo")), "/home/u/repo");
    }

    #[test]
    fn parents_and_containment_on_wsl_are_posix() {
        let host = Host::wsl("Ubuntu");
        assert_eq!(
            host.parent_str("/home/u/repo/"),
            Some("/home/u/repo".into())
                .map(|p: String| p.rsplit_once('/').map(|(a, _)| a.to_string()).unwrap())
        );
        assert_eq!(host.parent_str("/home"), Some("/".into()));
        assert_eq!(host.parent_str("/"), None);
        assert!(host.is_under("/home/u", "/home/u/repo"));
        assert!(host.is_under("/home/u/", "/home/u"));
        assert!(!host.is_under("/home/u", "/home/user"));
    }

    #[test]
    fn to_local_maps_linux_paths_to_the_share_and_mounts_to_drives() {
        let host = Host::wsl("Ubuntu");
        assert_eq!(
            host.to_local("/home/u/repo"),
            PathBuf::from("\\\\wsl.localhost\\Ubuntu\\home\\u\\repo")
        );
        assert_eq!(
            host.to_local("/mnt/c/Users/u"),
            PathBuf::from("C:\\Users\\u")
        );
        assert_eq!(host.to_local("/mnt/d"), PathBuf::from("D:\\"));
        // `/mnt/wsl` and `/mnt/data` are not drives.
        assert_eq!(
            host.to_local("/mnt/wsl/x"),
            PathBuf::from("\\\\wsl.localhost\\Ubuntu\\mnt\\wsl\\x")
        );
        assert_eq!(Host::Native.to_local("/tmp/x"), PathBuf::from("/tmp/x"));
    }

    #[test]
    fn from_local_recognises_both_share_names() {
        assert_eq!(
            Host::from_local("\\\\wsl.localhost\\Ubuntu\\home\\u\\repo"),
            (Host::wsl("Ubuntu"), "/home/u/repo".to_string())
        );
        assert_eq!(
            Host::from_local("\\\\wsl$\\Debian\\"),
            (Host::wsl("Debian"), "/".to_string())
        );
        assert_eq!(
            Host::from_local("//wsl.localhost/Ubuntu/etc"),
            (Host::wsl("Ubuntu"), "/etc".to_string())
        );
        assert_eq!(
            Host::from_local("C:\\Users\\u"),
            (Host::Native, "C:\\Users\\u".to_string())
        );
        assert_eq!(
            Host::from_local("/Users/u"),
            (Host::Native, "/Users/u".to_string())
        );
    }

    #[test]
    fn to_host_path_translates_only_what_this_distro_can_reach() {
        let host = Host::wsl("Ubuntu");
        assert_eq!(host.to_host_path("C:\\Users\\u\\src"), "/mnt/c/Users/u/src");
        assert_eq!(host.to_host_path("D:"), "/mnt/d");
        assert_eq!(
            host.to_host_path("\\\\wsl.localhost\\Ubuntu\\home\\u"),
            "/home/u"
        );
        // Another distribution's share is not translated.
        assert_eq!(
            host.to_host_path("\\\\wsl.localhost\\Debian\\home\\u"),
            "\\\\wsl.localhost\\Debian\\home\\u"
        );
        assert_eq!(host.to_host_path("/home/u"), "/home/u");
        assert_eq!(Host::Native.to_host_path("C:\\x"), "C:\\x");
    }

    #[test]
    fn home_keys_carry_the_distribution_and_native_keys_are_unchanged() {
        assert_eq!(Host::Native.home_key("/Users/u/.codex"), "/Users/u/.codex");
        let key = Host::wsl("Ubuntu").home_key("/home/u/.codex");
        assert_eq!(key, "wsl:Ubuntu:/home/u/.codex");
    }

    #[test]
    fn host_specs_parse() {
        assert_eq!(Host::parse_spec("native"), Some(Host::Native));
        assert_eq!(Host::parse_spec(""), Some(Host::Native));
        assert_eq!(Host::parse_spec("wsl:Ubuntu"), Some(Host::wsl("Ubuntu")));
        assert_eq!(Host::parse_spec("wsl:"), None);
        assert_eq!(Host::parse_spec("docker:x"), None);
    }

    #[test]
    fn lexical_normalisation_cleans_a_posix_path() {
        assert_eq!(normalise_posix("/home//u/./repo/"), "/home/u/repo");
        assert_eq!(normalise_posix("/home/u/../v"), "/home/v");
        assert_eq!(normalise_posix("/"), "/");
    }

    #[test]
    fn distro_list_decodes_utf16_and_utf8() {
        let mut utf16 = vec![0xFF, 0xFE];
        for unit in "Ubuntu\r\ndocker-desktop\r\n\r\n".encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(parse_distro_list(&utf16), vec!["Ubuntu", "docker-desktop"]);
        assert_eq!(
            parse_distro_list(b"Ubuntu\nDebian\n"),
            vec!["Ubuntu", "Debian"]
        );
        assert!(parse_distro_list(b"").is_empty());
    }

    #[test]
    fn orphan_pids_picks_parentless_app_servers_only() {
        let ps = "\
 100     1 /usr/bin/codex app-server --stdio
 101   100 /usr/bin/codex app-server
 102     1 codex exec
 103     1 /home/u/.local/bin/codex app-server
 104     1 node /x/claude";
        assert_eq!(orphan_pids(ps), vec!["100", "103"]);
    }

    #[test]
    fn resolve_script_quotes_names_and_checks_paths() {
        assert!(wsl_resolve_script("codex").starts_with("found=$(command -v 'codex'"));
        assert!(wsl_resolve_script("codex").contains("$HOME/.local/bin"));
        assert_eq!(
            wsl_resolve_script("/usr/bin/codex"),
            "[ -x '/usr/bin/codex' ] && printf %s '/usr/bin/codex'"
        );
        assert!(
            wsl_resolve_script("~/.local/bin/codex").starts_with("p=\"$HOME\"'/.local/bin/codex'")
        );
        assert!(wsl_resolve_script("it's").contains("'it'\"'\"'s'"));
    }

    #[test]
    fn labels_and_accessors() {
        assert_eq!(Host::Native.label(), "native");
        assert_eq!(Host::wsl("Ubuntu").label(), "WSL · Ubuntu");
        assert!(!Host::Native.is_wsl());
    }

    #[test]
    fn serialises_as_a_tagged_object() {
        assert_eq!(
            serde_json::to_string(&Host::Native).unwrap(),
            r#"{"kind":"native"}"#
        );
        assert_eq!(
            serde_json::to_string(&Host::wsl("Ubuntu")).unwrap(),
            r#"{"kind":"wsl","distro":"Ubuntu"}"#
        );
        assert_eq!(
            serde_json::from_str::<Host>(r#"{"kind":"wsl","distro":"Ubuntu"}"#).unwrap(),
            Host::wsl("Ubuntu")
        );
    }

    #[cfg(unix)]
    #[test]
    fn native_canonical_and_home_dir_use_the_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().display().to_string();
        assert_eq!(
            Host::Native.canonical(&path),
            std::fs::canonicalize(dir.path())
                .unwrap()
                .display()
                .to_string()
        );
        assert!(Host::Native.home_dir().is_some());
        assert!(Host::Native.resolve_binary("sh").is_some());
        assert!(Host::Native.resolve_binary("").is_none());
    }
}
