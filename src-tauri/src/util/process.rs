//! One spawn-with-timeout process runner, shared by the `git` and `gh` wrappers.
//!
//! Both wrappers need the same three guarantees: the child never blocks on
//! stdin, a hung child is killed rather than leaking, and output is decoded
//! lossily so invalid UTF-8 in a filename or commit message is not fatal.
//! Error *wording* stays with the caller — only the failure modes are shared.

use std::io::{Read, Write};
use std::process::Stdio;
use std::time::{Duration, Instant};

use super::host::Host;

/// A finished invocation with its captured, lossily-decoded output. A non-zero
/// exit is reported through `ok` rather than as an error so callers can classify
/// it themselves (e.g. "not a repository" versus "not authenticated").
#[derive(Debug)]
pub struct CommandOutput {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Why an invocation never produced output. Callers map these to their own
/// user-facing message so the text names the actual tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunError {
    /// The executable is not installed or not on PATH.
    NotFound,
    /// The process could not be started for any other reason.
    Spawn,
    /// The process outlived its timeout and was killed.
    Timeout,
    /// The process started but its output could not be collected.
    NoOutput,
}

/// One invocation to run. Built as a struct rather than positional arguments
/// because the optional pieces (env, stdin) are needed by only some callers.
/// `dir` is a host path: what `host` sees, not necessarily what this process
/// could open.
pub struct Run<'a> {
    pub(crate) host: &'a Host,
    pub(crate) program: &'a str,
    pub(crate) dir: &'a str,
    pub(crate) args: &'a [&'a str],
    /// Extra environment variables, applied on top of the inherited environment.
    pub(crate) env: &'a [(&'a str, &'a str)],
    /// Written to the child's stdin and then closed. `None` denies stdin
    /// entirely so a prompting tool fails fast instead of blocking.
    pub(crate) stdin: Option<&'a str>,
    pub(crate) timeout: Duration,
}

impl<'a> Run<'a> {
    /// A plain invocation on the machine Pingex runs on: no extra environment,
    /// no stdin.
    #[cfg(test)]
    pub(crate) fn new(
        program: &'a str,
        dir: &'a str,
        args: &'a [&'a str],
        timeout: Duration,
    ) -> Self {
        Self::on(&Host::Native, program, dir, args, timeout)
    }

    /// A plain invocation on `host`.
    pub fn on(
        host: &'a Host,
        program: &'a str,
        dir: &'a str,
        args: &'a [&'a str],
        timeout: Duration,
    ) -> Self {
        Self {
            host,
            program,
            dir,
            args,
            env: &[],
            stdin: None,
            timeout,
        }
    }
}

pub fn run(spec: Run<'_>) -> Result<CommandOutput, RunError> {
    let mut command = spec
        .host
        .command(spec.program, spec.args, Some(spec.dir), spec.env, &[]);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    command.stdin(if spec.stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });

    let mut child = command.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => RunError::NotFound,
        _ => RunError::Spawn,
    })?;

    if let Some(input) = spec.stdin {
        // Take the handle so it is dropped (closing the pipe) before we wait,
        // otherwise a child reading to EOF would never finish.
        if let Some(mut handle) = child.stdin.take() {
            let _ = handle.write_all(input.as_bytes());
        }
    }

    // Drain both pipes on their own threads so a chatty child cannot fill one
    // and block; the handle stays here so the timeout can kill it on any OS.
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);

    let deadline = Instant::now() + spec.timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                // Best-effort kill so a runaway child does not linger. The
                // drain threads observe the closed pipes and exit.
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Timeout);
            }
            Err(_) => return Err(RunError::NoOutput),
        }
    };
    let collect = |handle: Option<std::thread::JoinHandle<Vec<u8>>>| {
        handle
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default()
    };
    Ok(CommandOutput {
        ok: status.success(),
        stdout: String::from_utf8_lossy(&collect(stdout)).into_owned(),
        stderr: String::from_utf8_lossy(&collect(stderr)).into_owned(),
    })
}

fn drain<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = reader.read_to_end(&mut buffer);
        buffer
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd() -> &'static str {
        "."
    }

    #[test]
    fn captures_stdout_and_success() {
        let output = run(Run::new("echo", cwd(), &["hello"], Duration::from_secs(5)))
            .expect("echo should run");
        assert!(output.ok);
        assert_eq!(output.stdout.trim(), "hello");
    }

    #[test]
    fn reports_non_zero_exit_through_ok_not_error() {
        let output = run(Run::new("false", cwd(), &[], Duration::from_secs(5)))
            .expect("a non-zero exit is not a run error");
        assert!(!output.ok);
    }

    #[test]
    fn missing_executable_is_not_found() {
        let error = run(Run::new(
            "pingex-definitely-not-a-real-binary",
            cwd(),
            &[],
            Duration::from_secs(5),
        ))
        .unwrap_err();
        assert_eq!(error, RunError::NotFound);
    }

    #[test]
    fn slow_child_times_out() {
        let error = run(Run::new("sleep", cwd(), &["5"], Duration::from_millis(150))).unwrap_err();
        assert_eq!(error, RunError::Timeout);
    }

    #[test]
    fn writes_stdin_and_closes_it() {
        let output = run(Run {
            host: &Host::Native,
            program: "cat",
            dir: cwd(),
            args: &[],
            env: &[],
            stdin: Some("piped input"),
            timeout: Duration::from_secs(5),
        })
        .expect("cat should run");
        assert!(output.ok);
        assert_eq!(output.stdout, "piped input");
    }

    #[test]
    fn a_wsl_run_where_wsl_is_absent_is_not_found_rather_than_a_panic() {
        // Off Windows `wsl.exe` normally does not exist and the runner reports
        // it like any other missing executable; inside a WSL shell interop
        // makes it real, in which case the run simply happens.
        let host = Host::wsl("no-such-distro-pingex");
        match run(Run::on(&host, "true", "/tmp", &[], Duration::from_secs(20))) {
            Err(error) => assert!(matches!(error, RunError::NotFound | RunError::Spawn)),
            Ok(output) => assert!(!output.ok, "an unknown distribution cannot run anything"),
        }
    }

    #[test]
    fn applies_extra_environment() {
        let output = run(Run {
            host: &Host::Native,
            program: "sh",
            dir: cwd(),
            args: &["-c", "printf %s \"$PINGU_TEST_VAR\""],
            env: &[("PINGU_TEST_VAR", "set-by-test")],
            stdin: None,
            timeout: Duration::from_secs(5),
        })
        .expect("sh should run");
        assert_eq!(output.stdout, "set-by-test");
    }
}
