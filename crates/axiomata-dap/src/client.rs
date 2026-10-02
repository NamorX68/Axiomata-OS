//! An adapter process and the conversation with it.
//!
//! Threads and channels, no async runtime: one thread reads the adapter's stdout and sorts what arrives —
//! a response goes to whoever is waiting for it, events and requests from the adapter go to [`Incoming`] —
//! another keeps the tail of its stderr for error messages. A request is sent with [`Client::send`] and
//! waited for with [`Pending::wait`]; DAP needs the two apart (a `launch` is answered only after
//! `configurationDone`).

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::frame;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DapError {
    #[error("could not start the debug adapter: {0}")]
    Spawn(String),
    #[error("i/o with the debug adapter failed: {0}")]
    Io(String),
    #[error("the debug adapter spoke out of turn: {0}")]
    Protocol(String),
    #[error("the debug adapter refused “{command}”: {message}")]
    Refused { command: String, message: String },
    #[error("the debug adapter did not answer “{0}” in time")]
    Timeout(String),
    #[error("the debug adapter has ended{}", tail(.0))]
    Closed(String),
    #[error("{0}")]
    Invalid(String),
}

fn tail(stderr: &str) -> String {
    if stderr.trim().is_empty() {
        String::new()
    } else {
        format!(" — it said: {}", stderr.trim())
    }
}

/// What starts an adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<std::path::PathBuf>,
    pub env: Vec<(String, String)>,
}

/// What the adapter sends unasked.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Event {
        event: String,
        body: Value,
    },
    /// A request *from* the adapter (`runInTerminal`, `startDebugging`); answer it with [`Client::respond`].
    Request {
        seq: i64,
        command: String,
        arguments: Value,
    },
    /// The adapter's output ended (it exited or was killed).
    Closed,
}

type Waiting = Arc<Mutex<HashMap<i64, Sender<Result<Value, DapError>>>>>;

/// A request on its way; [`Pending::wait`] gives the answer.
pub struct Pending {
    command: String,
    rx: Receiver<Result<Value, DapError>>,
}

impl Pending {
    pub fn wait(self, timeout: Duration) -> Result<Value, DapError> {
        match self.rx.recv_timeout(timeout) {
            Ok(answer) => answer,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(DapError::Timeout(self.command)),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(DapError::Closed(String::new())),
        }
    }
}

pub struct Client {
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    seq: AtomicI64,
    waiting: Waiting,
    stderr: Arc<Mutex<String>>,
}

/// How much of the adapter's stderr is kept for an error message.
const STDERR_KEEP: usize = 4000;

impl Client {
    /// Starts the adapter and returns the client and the channel its events arrive on.
    pub fn spawn(command: &AdapterCommand) -> Result<(Client, Receiver<Incoming>), DapError> {
        let mut process = Command::new(&command.program);
        process
            .args(&command.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &command.cwd {
            process.current_dir(cwd);
        }
        for (key, value) in &command.env {
            process.env(key, value);
        }
        let mut child = process
            .spawn()
            .map_err(|e| DapError::Spawn(format!("{}: {e}", command.program)))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| DapError::Spawn("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| DapError::Spawn("no stdout".into()))?;
        let stderr_pipe = child
            .stderr
            .take()
            .ok_or_else(|| DapError::Spawn("no stderr".into()))?;

        let waiting: Waiting = Arc::default();
        let stderr = Arc::new(Mutex::new(String::new()));
        let (tx, rx) = mpsc::channel();

        {
            let stderr = Arc::clone(&stderr);
            std::thread::spawn(move || {
                for line in BufReader::new(stderr_pipe).lines().map_while(Result::ok) {
                    let mut kept = stderr.lock().unwrap_or_else(|p| p.into_inner());
                    kept.push_str(&line);
                    kept.push('\n');
                    if kept.len() > STDERR_KEEP {
                        let cut = kept.len() - STDERR_KEEP;
                        let at = (cut..kept.len())
                            .find(|i| kept.is_char_boundary(*i))
                            .unwrap_or(kept.len());
                        kept.drain(..at);
                    }
                }
            });
        }
        {
            let waiting = Arc::clone(&waiting);
            let stderr = Arc::clone(&stderr);
            std::thread::spawn(move || read_loop(BufReader::new(stdout), &waiting, &stderr, &tx));
        }
        let client = Client {
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            seq: AtomicI64::new(1),
            waiting,
            stderr,
        };
        Ok((client, rx))
    }

    /// The tail of what the adapter wrote to stderr — for a message when something went wrong.
    pub fn stderr_tail(&self) -> String {
        self.stderr
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    fn write(&self, message: &Value) -> Result<(), DapError> {
        let mut stdin = self.stdin.lock().unwrap_or_else(|p| p.into_inner());
        frame::write_message(&mut *stdin, message)
    }

    /// Sends a request and returns at once; wait for the answer with [`Pending::wait`].
    pub fn send(&self, command: &str, arguments: Value) -> Result<Pending, DapError> {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.waiting
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(seq, tx);
        let mut message = json!({ "seq": seq, "type": "request", "command": command });
        if !arguments.is_null() {
            message["arguments"] = arguments;
        }
        if let Err(err) = self.write(&message) {
            self.waiting
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&seq);
            return Err(match err {
                DapError::Io(_) => DapError::Closed(self.stderr_tail()),
                other => other,
            });
        }
        Ok(Pending {
            command: command.to_string(),
            rx,
        })
    }

    /// Sends a request and waits for its answer.
    pub fn request(
        &self,
        command: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value, DapError> {
        self.send(command, arguments)?.wait(timeout)
    }

    /// Answers a request the adapter made.
    pub fn respond(
        &self,
        request_seq: i64,
        command: &str,
        result: Result<Value, String>,
    ) -> Result<(), DapError> {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let message = match result {
            Ok(body) => {
                json!({ "seq": seq, "type": "response", "request_seq": request_seq, "success": true, "command": command, "body": body })
            }
            Err(why) => {
                json!({ "seq": seq, "type": "response", "request_seq": request_seq, "success": false, "command": command, "message": why })
            }
        };
        self.write(&message)
    }

    /// Ends the adapter: closes its input, then kills it if it is still there.
    pub fn kill(&self) {
        let mut child = self.child.lock().unwrap_or_else(|p| p.into_inner());
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // Never leave an adapter (and the program under it) running behind a dropped session.
        self.kill();
    }
}

fn read_loop(
    mut reader: impl BufRead,
    waiting: &Waiting,
    stderr: &Mutex<String>,
    tx: &Sender<Incoming>,
) {
    let closed = |reason: DapError| {
        for (_, sender) in waiting.lock().unwrap_or_else(|p| p.into_inner()).drain() {
            let _ = sender.send(Err(reason.clone()));
        }
    };
    loop {
        match frame::read_message(&mut reader) {
            Ok(Some(message)) => dispatch(message, waiting, tx),
            Ok(None) | Err(_) => {
                // Give the stderr thread a moment to collect the last words before they are quoted.
                std::thread::sleep(Duration::from_millis(50));
                closed(DapError::Closed(
                    stderr.lock().unwrap_or_else(|p| p.into_inner()).clone(),
                ));
                let _ = tx.send(Incoming::Closed);
                return;
            }
        }
    }
}

fn dispatch(message: Value, waiting: &Waiting, tx: &Sender<Incoming>) {
    match message["type"].as_str() {
        Some("response") => {
            let Some(seq) = message["request_seq"].as_i64() else {
                return;
            };
            let Some(sender) = waiting
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&seq)
            else {
                return;
            };
            let answer = if message["success"].as_bool().unwrap_or(false) {
                Ok(message.get("body").cloned().unwrap_or(Value::Null))
            } else {
                Err(DapError::Refused {
                    command: message["command"].as_str().unwrap_or("?").to_string(),
                    message: message["message"]
                        .as_str()
                        .unwrap_or("no reason given")
                        .to_string(),
                })
            };
            let _ = sender.send(answer);
        }
        Some("event") => {
            let event = message["event"].as_str().unwrap_or("").to_string();
            let _ = tx.send(Incoming::Event {
                event,
                body: message.get("body").cloned().unwrap_or(Value::Null),
            });
        }
        Some("request") => {
            let _ = tx.send(Incoming::Request {
                seq: message["seq"].as_i64().unwrap_or(0),
                command: message["command"].as_str().unwrap_or("").to_string(),
                arguments: message.get("arguments").cloned().unwrap_or(Value::Null),
            });
        }
        _ => {}
    }
}
