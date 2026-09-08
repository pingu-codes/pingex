//! Live suite for a WSL Host: the app's real spawn path against a real
//! distribution.
//!
//! Run:  `PINGEX_E2E_WSL=<distro> deno task test:e2e:wsl`
//! Optional: `PINGEX_E2E_WSL_CODEX=<path or name>` (default `codex`),
//! `PINGEX_E2E_WSL_HOME=<CODEX_HOME inside the distribution>` (default
//! `$HOME/.codex` there). Skips silently without the variable so
//! `cargo test` stays offline. Works from Windows and, through interop,
//! from inside a WSL shell.

use pingex_app_lib::e2e::{
    kill_orphaned_app_servers, orphan_pids, requests, run_process, Host, Run, CODEX_APP_SERVER_ARGS,
};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::time::Duration;

fn distro() -> Option<String> {
    let distro = std::env::var("PINGEX_E2E_WSL").ok()?;
    if distro.trim().is_empty() {
        eprintln!("skipping: PINGEX_E2E_WSL is empty");
        return None;
    }
    Some(distro)
}

fn shell(host: &Host, script: &str) -> String {
    let output = run_process(Run::on(
        host,
        "sh",
        "/",
        &["-c", script],
        Duration::from_secs(60),
    ))
    .unwrap_or_else(|error| panic!("could not run `{script}` in the distribution: {error:?}"));
    assert!(output.ok, "`{script}` failed: {}", output.stderr);
    output.stdout.trim().to_string()
}

#[test]
fn the_distribution_answers_and_paths_translate() {
    let Some(distro) = distro() else { return };
    let host = Host::wsl(&distro);
    assert_eq!(shell(&host, "printf %s ok"), "ok");

    let home = host.home_dir().expect("the distribution reports $HOME");
    assert!(home.starts_with('/'), "home dir is a Linux path: {home}");

    // Canonical paths come from `readlink -m` inside the distribution.
    assert_eq!(host.canonical(&format!("{home}//./")), home);
    assert!(
        host.is_dir(&home) || !cfg!(windows),
        "the share sees the home dir"
    );

    let local = host.to_local(&format!("{home}/repo"));
    let expected = format!(
        "\\\\wsl.localhost\\{distro}\\{}\\repo",
        home.trim_start_matches('/').replace('/', "\\")
    );
    assert_eq!(local.to_string_lossy(), expected);

    if cfg!(windows) {
        // A file written inside the distribution is readable over the share.
        shell(&host, "printf hello > /tmp/pingex-e2e-wsl.txt");
        let text = std::fs::read_to_string(host.to_local("/tmp/pingex-e2e-wsl.txt"))
            .expect("read through the share");
        assert_eq!(text, "hello");
    }
}

#[test]
fn git_runs_inside_the_distribution() {
    let Some(distro) = distro() else { return };
    let host = Host::wsl(&distro);
    let output = run_process(Run::on(
        &host,
        "git",
        "/",
        &["--version"],
        Duration::from_secs(60),
    ))
    .expect("git spawns through wsl.exe");
    assert!(output.ok, "git --version failed: {}", output.stderr);
    assert!(
        output.stdout.starts_with("git version"),
        "{}",
        output.stdout
    );
}

#[test]
fn codex_resolves_spawns_and_leaves_no_orphan() {
    let Some(distro) = distro() else { return };
    let host = Host::wsl(&distro);
    let configured = std::env::var("PINGEX_E2E_WSL_CODEX").unwrap_or_else(|_| "codex".into());
    let program = host
        .resolve_binary(&configured)
        .unwrap_or_else(|| panic!("{configured} does not resolve inside {distro}"));
    assert!(
        program.starts_with('/'),
        "resolved to a Linux path: {program}"
    );

    let codex_home = std::env::var("PINGEX_E2E_WSL_HOME")
        .unwrap_or_else(|_| host.join_str(&host.home_dir().expect("home dir"), ".codex"));

    // The exact argv the app uses (`codex::child::spawn_child`).
    let argv = host.argv(
        &program,
        &CODEX_APP_SERVER_ARGS,
        None,
        &[("CODEX_HOME", &codex_home)],
        &[],
    );
    assert_eq!(argv[0], "wsl.exe");
    assert!(argv.contains(&"--exec".to_string()));

    let mut child = host
        .command(
            &program,
            &CODEX_APP_SERVER_ARGS,
            None,
            &[("CODEX_HOME", &codex_home)],
            &[],
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn codex app-server through wsl.exe");
    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");

    let initialize = requests::initialize("pingex-e2e-wsl");
    let request = json!({"id": 1, "method": initialize.method, "params": initialize.params});
    writeln!(stdin, "{request}").expect("write initialize");

    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(message) = serde_json::from_str::<Value>(&line) {
                if message.get("id").and_then(Value::as_i64) == Some(1) {
                    let _ = sender.send(message);
                    return;
                }
            }
        }
    });
    let response = receiver
        .recv_timeout(Duration::from_secs(120))
        .expect("initialize answered within two minutes");
    let user_agent = response
        .pointer("/result/userAgent")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    assert!(
        user_agent.to_lowercase().contains("codex"),
        "unexpected initialize response: {response}"
    );

    // Closing stdin is the app's shutdown signal; killing the launcher is the
    // fallback. Whatever survives inside the distribution, the reaper finds.
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    std::thread::sleep(Duration::from_secs(2));
    kill_orphaned_app_servers(std::slice::from_ref(&host));
    let ps = shell(&host, "ps -axo pid=,ppid=,command=");
    assert!(
        orphan_pids(&ps).is_empty(),
        "orphaned app-servers remain in {distro}:\n{ps}"
    );
}
