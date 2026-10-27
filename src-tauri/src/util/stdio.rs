//! The last lines a child process wrote to stderr, kept for the error a user
//! sees when it exits early.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

const TAIL_LINES: usize = 40;
const LINE_BYTES: usize = 2048;

pub(crate) struct StderrTail {
    lines: Arc<Mutex<VecDeque<String>>>,
    done: Mutex<mpsc::Receiver<()>>,
}

impl StderrTail {
    /// Drain `stderr` on a thread of its own, keeping a bounded tail.
    pub(crate) fn spawn(stderr: Option<impl Read + Send + 'static>) -> Self {
        let lines = Arc::new(Mutex::new(VecDeque::new()));
        let (finished, done) = mpsc::channel();
        match stderr {
            Some(stderr) => {
                let tail = lines.clone();
                std::thread::spawn(move || {
                    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                        let Ok(mut tail) = tail.lock() else { break };
                        if tail.len() == TAIL_LINES {
                            tail.pop_front();
                        }
                        // Bound each line as well as the count, including UTF-8.
                        let mut end = line.len().min(LINE_BYTES);
                        while !line.is_char_boundary(end) {
                            end -= 1;
                        }
                        tail.push_back(line[..end].to_string());
                    }
                    let _ = finished.send(());
                });
            }
            None => {
                let _ = finished.send(());
            }
        }
        Self {
            lines,
            done: Mutex::new(done),
        }
    }

    pub(crate) fn text(&self) -> String {
        self.lines
            .lock()
            .map(|lines| lines.iter().cloned().collect::<Vec<_>>().join("\n"))
            .unwrap_or_default()
    }

    /// EOF on stdout can race the stderr reader. Bound the wait so a child
    /// that merely closed stdout cannot hold anything up.
    pub(crate) fn wait_drained(&self, limit: Duration) {
        if let Ok(done) = self.done.lock() {
            let _ = done.recv_timeout(limit);
        }
    }
}

/// `text` without its terminal colour and cursor escapes.
pub(crate) fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}
