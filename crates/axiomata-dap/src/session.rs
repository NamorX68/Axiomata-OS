//! One debugging session: the start-up dance, typed events, and the questions a debugger UI asks.
//!
//! DAP's start order is not request/response in a line: `initialize` is answered at once, `launch` is
//! **sent** but only answered after `configurationDone`, and between them the adapter announces
//! `initialized`, after which the client sends its breakpoints. [`Session::start`] does that whole dance.
//! After it, [`Session::events`] delivers what happens (a stop, output, the end) and the methods below ask
//! for details (stack, scopes, variables) or move the program (continue, step).

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

use crate::client::{AdapterCommand, Client, DapError, Incoming};

const FAST: Duration = Duration::from_secs(10);
const SLOW: Duration = Duration::from_secs(40);

/// Why and where the program stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StopState {
    pub thread_id: i64,
    /// `breakpoint`, `step`, `exception`, `pause`, `entry`, … as the adapter names it.
    pub reason: String,
    /// The adapter's own wording (“Paused on exception”), when it gave one.
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum DebugEvent {
    Stopped(StopState),
    Continued {
        thread_id: i64,
    },
    /// `stdout`, `stderr`, `console` (the adapter's own), …
    Output {
        category: String,
        text: String,
    },
    /// The program ended with this exit code.
    Exited {
        code: i64,
    },
    /// The session is over (the program and the adapter are done).
    Terminated,
    /// The adapter went away; `stderr` is what it said last.
    Closed {
        stderr: String,
    },
    /// The adapter asks for the program to be started in a terminal (`runInTerminal`): type `line` into a
    /// shell there. Not produced by [`Session::next_event`] — it reaches the [`TerminalHandler`] while the
    /// session is still starting, since the program only gets going once it has been run.
    RunInTerminal {
        title: String,
        line: String,
    },
}

/// What the adapter asks for with `runInTerminal`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRequest {
    pub title: String,
    pub cwd: Option<String>,
    pub args: Vec<String>,
    /// `None` = unset the variable.
    pub env: Vec<(String, Option<String>)>,
}

/// `value` as one single-quoted shell word (`it's` → `'it'\''s'`).
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

impl TerminalRequest {
    /// The shell line that does it: `cd` first, the variables set for the program alone, every word quoted.
    pub fn command_line(&self) -> String {
        let mut line = String::new();
        if let Some(cwd) = &self.cwd {
            line.push_str(&format!("cd {} && ", shell_quote(cwd)));
        }
        for (key, value) in &self.env {
            let valid =
                !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            match value {
                Some(v) if valid => line.push_str(&format!("{key}={} ", shell_quote(v))),
                _ => {}
            }
        }
        let words: Vec<String> = self.args.iter().map(|a| shell_quote(a)).collect();
        line.push_str(&words.join(" "));
        line
    }
}

/// Called with each `runInTerminal` request, on the reader's thread — it must not block.
pub type TerminalHandler = Arc<dyn Fn(TerminalRequest) + Send + Sync>;

/// A line breakpoint as the adapter confirmed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Breakpoint {
    /// The line it actually sits on (the adapter may move it to the next executable line).
    pub line: u32,
    pub verified: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Frame {
    pub id: i64,
    pub name: String,
    /// The source file; `None` for generated code with no file.
    pub path: Option<String>,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Scope {
    pub name: String,
    pub variables_reference: i64,
    /// Fetching it is costly (globals) — the UI should ask only when it is opened.
    pub expensive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Variable {
    pub name: String,
    pub value: String,
    pub type_name: Option<String>,
    /// Non-zero: it has children, fetched with [`Session::variables`].
    pub variables_reference: i64,
}

pub struct Session {
    client: Arc<Client>,
    events: Mutex<Receiver<DebugEvent>>,
    capabilities: Value,
}

impl Session {
    /// Starts the adapter, launches the program with `launch` (the adapter-specific arguments of the DAP
    /// `launch` request) and hands the adapter `breakpoints` (file → lines) before the program runs.
    pub fn start(
        adapter: &AdapterCommand,
        adapter_id: &str,
        launch: Value,
        breakpoints: &[(String, Vec<u32>)],
    ) -> Result<Session, DapError> {
        Session::start_with(adapter, adapter_id, launch, breakpoints, None)
    }

    /// [`Session::start`], with a `terminal` that can run the program (`launch` must ask for it: debugpy's
    /// `"console": "integratedTerminal"`). Without one the adapter is told there is no terminal.
    pub fn start_with(
        adapter: &AdapterCommand,
        adapter_id: &str,
        launch: Value,
        breakpoints: &[(String, Vec<u32>)],
        terminal: Option<TerminalHandler>,
    ) -> Result<Session, DapError> {
        let (client, incoming) = Client::spawn(adapter)?;
        let client = Arc::new(client);
        let (events_tx, events_rx) = mpsc::channel();
        let (init_tx, init_rx) = mpsc::channel();
        let can_terminal = terminal.is_some();
        {
            let client = Arc::clone(&client);
            std::thread::spawn(move || {
                pump(&client, incoming, &events_tx, &init_tx, terminal.as_ref())
            });
        }

        let capabilities = client.request(
            "initialize",
            json!({
                "clientID": "axiomata-studio",
                "clientName": "Axiomata Studio",
                "adapterID": adapter_id,
                "linesStartAt1": true,
                "columnsStartAt1": true,
                "pathFormat": "path",
                "supportsVariableType": true,
                "supportsVariablePaging": false,
                "supportsRunInTerminalRequest": can_terminal,
                "supportsProgressReporting": false,
            }),
            SLOW,
        )?;

        let launched = client.send("launch", launch)?;
        init_rx
            .recv_timeout(SLOW)
            .map_err(|_| match client.stderr_tail() {
                tail if !tail.trim().is_empty() => DapError::Closed(tail),
                _ => DapError::Timeout("initialized".into()),
            })?;

        let session = Session {
            client: Arc::clone(&client),
            events: Mutex::new(events_rx),
            capabilities,
        };
        for (path, lines) in breakpoints {
            session.set_breakpoints(path, lines)?;
        }
        // Stop on an exception nobody catches, when the adapter offers it.
        let uncaught = session.capabilities["exceptionBreakpointFilters"]
            .as_array()
            .is_some_and(|f| f.iter().any(|x| x["filter"] == "uncaught"));
        client.request(
            "setExceptionBreakpoints",
            json!({ "filters": if uncaught { json!(["uncaught"]) } else { json!([]) } }),
            FAST,
        )?;
        if session.capabilities["supportsConfigurationDoneRequest"]
            .as_bool()
            .unwrap_or(true)
        {
            client.request("configurationDone", Value::Null, FAST)?;
        }
        launched.wait(SLOW)?;
        Ok(session)
    }

    /// What the session reported since the last call, waiting up to `timeout` for the next thing.
    pub fn next_event(&self, timeout: Duration) -> Option<DebugEvent> {
        self.events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .recv_timeout(timeout)
            .ok()
    }

    /// Replaces all breakpoints of a file; the answer says where each one landed.
    pub fn set_breakpoints(&self, path: &str, lines: &[u32]) -> Result<Vec<Breakpoint>, DapError> {
        let body = self.client.request(
            "setBreakpoints",
            json!({
                "source": { "path": path },
                "breakpoints": lines.iter().map(|l| json!({ "line": l })).collect::<Vec<_>>(),
            }),
            FAST,
        )?;
        Ok(body["breakpoints"]
            .as_array()
            .map(|list| {
                list.iter()
                    .map(|b| Breakpoint {
                        line: b["line"].as_u64().unwrap_or(0) as u32,
                        verified: b["verified"].as_bool().unwrap_or(false),
                        message: b["message"].as_str().map(str::to_string),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn stack_trace(&self, thread_id: i64) -> Result<Vec<Frame>, DapError> {
        let body = self.client.request(
            "stackTrace",
            json!({ "threadId": thread_id, "levels": 50 }),
            FAST,
        )?;
        Ok(body["stackFrames"]
            .as_array()
            .map(|frames| {
                frames
                    .iter()
                    .map(|f| Frame {
                        id: f["id"].as_i64().unwrap_or(0),
                        name: f["name"].as_str().unwrap_or("?").to_string(),
                        path: f["source"]["path"].as_str().map(str::to_string),
                        line: f["line"].as_u64().unwrap_or(0) as u32,
                        column: f["column"].as_u64().unwrap_or(0) as u32,
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn scopes(&self, frame_id: i64) -> Result<Vec<Scope>, DapError> {
        let body = self
            .client
            .request("scopes", json!({ "frameId": frame_id }), FAST)?;
        Ok(body["scopes"]
            .as_array()
            .map(|scopes| {
                scopes
                    .iter()
                    .map(|s| Scope {
                        name: s["name"].as_str().unwrap_or("?").to_string(),
                        variables_reference: s["variablesReference"].as_i64().unwrap_or(0),
                        expensive: s["expensive"].as_bool().unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn variables(&self, variables_reference: i64) -> Result<Vec<Variable>, DapError> {
        let body = self.client.request(
            "variables",
            json!({ "variablesReference": variables_reference }),
            FAST,
        )?;
        Ok(body["variables"]
            .as_array()
            .map(|list| list.iter().map(variable_of).collect())
            .unwrap_or_default())
    }

    /// Evaluates `expression` in a frame (the console's line). The answer is a [`Variable`] named after the expression.
    pub fn evaluate(&self, expression: &str, frame_id: Option<i64>) -> Result<Variable, DapError> {
        let mut args = json!({ "expression": expression, "context": "repl" });
        if let Some(frame) = frame_id {
            args["frameId"] = frame.into();
        }
        let body = self.client.request("evaluate", args, FAST)?;
        Ok(Variable {
            name: expression.to_string(),
            value: body["result"].as_str().unwrap_or("").to_string(),
            type_name: body["type"].as_str().map(str::to_string),
            variables_reference: body["variablesReference"].as_i64().unwrap_or(0),
        })
    }

    /// `continue`, `next`, `stepIn`, `stepOut` or `pause` for a thread.
    pub fn control(&self, action: Control, thread_id: i64) -> Result<(), DapError> {
        self.client
            .request(action.command(), json!({ "threadId": thread_id }), FAST)
            .map(|_| ())
    }

    /// Ends the session: asks the adapter to stop the program, then makes sure nothing is left running.
    pub fn end(&self) {
        let supports_terminate = self.capabilities["supportsTerminateRequest"]
            .as_bool()
            .unwrap_or(false);
        let _ = if supports_terminate {
            self.client
                .request("terminate", Value::Null, Duration::from_secs(2))
        } else {
            self.client.request(
                "disconnect",
                json!({ "terminateDebuggee": true }),
                Duration::from_secs(2),
            )
        };
        self.client.kill();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.client.kill();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Continue,
    Next,
    StepIn,
    StepOut,
    Pause,
}

impl Control {
    fn command(self) -> &'static str {
        match self {
            Control::Continue => "continue",
            Control::Next => "next",
            Control::StepIn => "stepIn",
            Control::StepOut => "stepOut",
            Control::Pause => "pause",
        }
    }
}

fn variable_of(v: &Value) -> Variable {
    Variable {
        name: v["name"].as_str().unwrap_or("?").to_string(),
        value: v["value"].as_str().unwrap_or("").to_string(),
        type_name: v["type"].as_str().map(str::to_string),
        variables_reference: v["variablesReference"].as_i64().unwrap_or(0),
    }
}

/// Sorts what the adapter sends unasked: typed events for the UI, `initialized` for the start-up dance,
/// and a plain "not supported" for anything the adapter asks of us.
fn pump(
    client: &Client,
    incoming: Receiver<Incoming>,
    events: &Sender<DebugEvent>,
    initialized: &Sender<()>,
    terminal: Option<&TerminalHandler>,
) {
    for message in incoming {
        match message {
            Incoming::Event { event, body } => match event.as_str() {
                "initialized" => {
                    let _ = initialized.send(());
                }
                "stopped" => {
                    let _ = events.send(DebugEvent::Stopped(StopState {
                        thread_id: body["threadId"].as_i64().unwrap_or(0),
                        reason: body["reason"].as_str().unwrap_or("").to_string(),
                        text: body["text"]
                            .as_str()
                            .or_else(|| body["description"].as_str())
                            .map(str::to_string),
                    }));
                }
                "continued" => {
                    let _ = events.send(DebugEvent::Continued {
                        thread_id: body["threadId"].as_i64().unwrap_or(0),
                    });
                }
                "output" => {
                    let _ = events.send(DebugEvent::Output {
                        category: body["category"].as_str().unwrap_or("console").to_string(),
                        text: body["output"].as_str().unwrap_or("").to_string(),
                    });
                }
                "exited" => {
                    let _ = events.send(DebugEvent::Exited {
                        code: body["exitCode"].as_i64().unwrap_or(0),
                    });
                }
                "terminated" => {
                    let _ = events.send(DebugEvent::Terminated);
                }
                _ => {}
            },
            Incoming::Request {
                seq,
                command,
                arguments,
                ..
            } if command == "runInTerminal" && terminal.is_some() => {
                let words = |v: &Value| -> Vec<String> {
                    v.as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default()
                };
                let request = TerminalRequest {
                    title: arguments["title"].as_str().unwrap_or("Debug").to_string(),
                    cwd: arguments["cwd"]
                        .as_str()
                        .filter(|c| !c.is_empty())
                        .map(str::to_string),
                    args: words(&arguments["args"]),
                    env: arguments["env"]
                        .as_object()
                        .map(|m| {
                            m.iter()
                                .map(|(k, v)| (k.clone(), v.as_str().map(str::to_string)))
                                .collect()
                        })
                        .unwrap_or_default(),
                };
                if request.args.is_empty() {
                    let _ = client.respond(
                        seq,
                        &command,
                        Err("runInTerminal without a command".into()),
                    );
                } else {
                    if let Some(handler) = terminal {
                        handler(request);
                    }
                    let _ = client.respond(seq, &command, Ok(json!({})));
                }
            }
            Incoming::Request { seq, command, .. } => {
                let _ = client.respond(
                    seq,
                    &command,
                    Err(format!("the Studio does not support “{command}”")),
                );
            }
            Incoming::Closed => {
                let _ = events.send(DebugEvent::Closed {
                    stderr: client.stderr_tail(),
                });
                return;
            }
        }
    }
}
