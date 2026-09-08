//! Read-only smoke test. Runs installed CLIs concurrently without model turns.
//! Set PINGEX_E2E_MIXED_HOSTS to the WSL distribution to enable it.
use pingex_app_lib::e2e::{run_process, Host, Run};
use std::{
    sync::{Arc, Barrier},
    time::Duration,
};

#[test]
fn both_harnesses_start_on_both_hosts_concurrently() {
    let Ok(distro) = std::env::var("PINGEX_E2E_MIXED_HOSTS") else {
        return;
    };
    let barrier = Arc::new(Barrier::new(4));
    let mut tasks = Vec::new();
    for host in [Host::Native, Host::wsl(&distro)] {
        for harness in ["codex", "claude"] {
            let host = host.clone();
            let barrier = barrier.clone();
            tasks.push(std::thread::spawn(move || {
                barrier.wait();
                let binary = host.resolve_binary(harness).expect("installed harness");
                let cwd = host.home_dir().expect("Host home directory");
                let output = run_process(Run::on(
                    &host,
                    &binary,
                    &cwd,
                    &["--version"],
                    Duration::from_secs(45),
                ))
                .expect("CLI starts");
                assert!(output.ok, "{host:?} {harness}: {}", output.stderr);
                assert!(!output.stdout.trim().is_empty());
                eprintln!("{host:?} {harness}: {}", output.stdout.trim());
            }));
        }
    }
    let results: Vec<_> = tasks.into_iter().map(|task| task.join()).collect();
    assert!(
        results.iter().all(Result::is_ok),
        "At least one Host/harness failed"
    );
}
