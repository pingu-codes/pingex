//! One spawn-with-timeout process runner, shared by the `git` and `gh` wrappers.
//!
//! Both wrappers need the same three guarantees: the child never blocks on
//! stdin, a hung child is killed rather than leaking, and output is decoded
//! lossily so invalid UTF-8 in a filename or commit message is not fatal.
//! Error *wording* stays with the caller — only the failure modes are shared.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// A finished invocation with its captured, lossily-decoded output. A non-zero
/// exit is reported through `ok` rather than as an error so callers can classify
/// it themselves (e.g. "not a repository" versus "not authenticated").
#[derive(Debug)]
pub(crate) struct CommandOutput {
    pub(crate) ok: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// Why an invocation never produced output. Callers map these to their own
/// user-facing message so the text names the actual tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunError {
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
pub(crate) struct Run<'a> {
    pub(crate) program: &'a str,
    pub(crate) dir: &'a Path,
    pub(crate) args: &'a [&'a str],
    /// Extra environment variables, applied on top of the inherited environment.
    pub(crate) env: &'a [(&'a str, &'a str)],
    /// Written to the child's stdin and then closed. `None` denies stdin
    /// entirely so a prompting tool fails fast instead of blocking.
    pub(crate) stdin: Option<&'a str>,
    pub(crate) timeout: Duration,
}

impl<'a> Run<'a> {
    /// A plain invocation: no extra environment, no stdin.
    pub(crate) fn new(
        program: &'a str,
        dir: &'a Path,
        args: &'a [&'a str],
        timeout: Duration,
    ) -> Self {
        Self {
            program,
            dir,
            args,
            env: &[],
            stdin: None,
            timeout,
        }
    }
}

pub(crate) fn run(spec: Run<'_>) -> Result<CommandOutput, RunError> {
    let mut command = Command::new(spec.program);
    command
        .current_dir(spec.dir)
        .args(spec.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in spec.env {
        command.env(key, value);
    }
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

    let pid = child.id();
    let (sender, receiver) = mpsc::channel();
    let waiter = std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });

    match receiver.recv_timeout(spec.timeout) {
        Ok(result) => {
            let _ = waiter.join();
            let output = result.map_err(|_| RunError::NoOutput)?;
            Ok(CommandOutput {
                ok: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
        Err(_) => {
            // Best-effort kill so a runaway child does not linger. The reader
            // thread is detached; it observes the closed pipes and exits.
            #[cfg(unix)]
            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
            #[cfg(not(unix))]
            let _ = pid;
            Err(RunError::Timeout)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd() -> &'static Path {
        Path::new(".")
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
    fn applies_extra_environment() {
        let output = run(Run {
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
