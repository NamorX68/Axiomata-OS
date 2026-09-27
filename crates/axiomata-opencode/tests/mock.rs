//! The whole client path against a stand-in for the Opencode service:
//! discovery through a fake `opencode` script, login, version check, session,
//! prompt, event stream and reading the turn back. The stand-in is a small
//! hand-written HTTP server on the loopback — no extra dependency, and it
//! answers exactly the shapes measured against Opencode 2.0.18
//! (`docs/plans/opencode2.md`).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axiomata_opencode::{
    ModelRef, NewSession, OpencodeError, Service, TurnRequest, TurnSession, unattended_permissions,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

/// `Basic base64("opencode:pw")` — the login the stand-in accepts.
const LOGIN: &str = "Basic b3BlbmNvZGU6cHc=";

/// How the stand-in behaves.
#[derive(Clone)]
struct Scenario {
    version: &'static str,
    /// The model asks for a permission instead of answering.
    asks_permission: bool,
}

/// What the stand-in saw.
#[derive(Default)]
struct Seen {
    requests: Vec<(String, String, String)>,
}

struct Mock {
    port: u16,
    seen: Arc<Mutex<Seen>>,
}

impl Mock {
    async fn start(scenario: Scenario) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Seen::default()));
        let (events, _) = broadcast::channel::<String>(64);
        let state = (Arc::clone(&seen), events, scenario);
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let (seen, events, scenario) =
                    (Arc::clone(&state.0), state.1.clone(), state.2.clone());
                tokio::spawn(async move { serve(stream, seen, events, scenario).await });
            }
        });
        Self { port, seen }
    }

    fn requests(&self) -> Vec<(String, String, String)> {
        self.seen.lock().unwrap().requests.clone()
    }
}

/// Reads one request: method, path, the `authorization` header, and the body.
async fn read_request(stream: &mut TcpStream) -> Option<(String, String, String, String)> {
    let mut buf = Vec::new();
    let head_end = loop {
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.lines();
    let mut first = lines.next()?.split(' ');
    let (method, path) = (first.next()?.to_string(), first.next()?.to_string());
    let mut auth = String::new();
    let mut length = 0usize;
    for line in lines {
        let (name, value) = line.split_once(':').unwrap_or((line, ""));
        match name.to_ascii_lowercase().as_str() {
            "authorization" => auth = value.trim().to_string(),
            "content-length" => length = value.trim().parse().unwrap_or(0),
            _ => {}
        }
    }
    let mut body = buf[head_end..].to_vec();
    while body.len() < length {
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk).await.ok()?;
        body.extend_from_slice(&chunk[..n]);
    }
    Some((
        method,
        path,
        auth,
        String::from_utf8_lossy(&body).to_string(),
    ))
}

async fn respond(stream: &mut TcpStream, status: &str, body: &Value) {
    let text = body.to_string();
    let reply = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
        text.len()
    );
    let _ = stream.write_all(reply.as_bytes()).await;
}

fn event(kind: &str, data: Value) -> String {
    format!(
        "data: {}\n\n",
        json!({"id": "evt_1", "type": kind, "data": data})
    )
}

fn assistant(finish: &str, text: &str, input: u64, output: u64, cost: f64) -> Value {
    json!({
        "id": format!("msg_a{input}"), "type": "assistant", "agent": "build",
        "model": {"providerID": "ollama", "id": "m"}, "time": {"created": 2},
        "finish": finish, "cost": cost,
        "tokens": {"input": input, "output": output, "reasoning": 0, "cache": {"read": 0, "write": 0}},
        "content": [{"type": "text", "text": text}]
    })
}

async fn serve(
    mut stream: TcpStream,
    seen: Arc<Mutex<Seen>>,
    events: broadcast::Sender<String>,
    scenario: Scenario,
) {
    let Some((method, path, auth, body)) = read_request(&mut stream).await else {
        return;
    };
    seen.lock()
        .unwrap()
        .requests
        .push((method.clone(), path.clone(), body.clone()));
    if auth != LOGIN {
        return respond(
            &mut stream,
            "401 Unauthorized",
            &json!({"_tag": "UnauthorizedError"}),
        )
        .await;
    }
    let sid = "ses_mock1";
    match (method.as_str(), path.as_str()) {
        ("GET", "/api/info") => {
            respond(
                &mut stream,
                "200 OK",
                &json!({"version": scenario.version, "pid": 1}),
            )
            .await
        }
        ("GET", "/api/event") => {
            let mut rx = events.subscribe();
            let head =
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
            if stream.write_all(head.as_bytes()).await.is_err() {
                return;
            }
            let _ = stream
                .write_all(event("server.connected", json!({})).as_bytes())
                .await;
            let _ = stream.write_all(b": heartbeat\n\n").await;
            while let Ok(line) = rx.recv().await {
                if stream.write_all(line.as_bytes()).await.is_err() {
                    return;
                }
            }
        }
        ("POST", "/api/session") => {
            respond(&mut stream, "200 OK", &json!({"data": {"id": sid}})).await
        }
        ("POST", "/api/session/ses_mock1/prompt") => {
            respond(
                &mut stream,
                "200 OK",
                &json!({"data": {"id": "msg_user", "time": {"created": 1}}}),
            )
            .await;
            // Another session's events must be ignored.
            let _ = events.send(event(
                "session.execution.succeeded",
                json!({"sessionID": "ses_other"}),
            ));
            if scenario.asks_permission {
                let asked = json!({"id": "per_1", "sessionID": sid, "action": "shell", "resources": ["rm -rf x"]});
                let _ = events.send(event("permission.asked", asked));
            } else {
                let _ = events.send(event(
                    "session.execution.started",
                    json!({"sessionID": sid}),
                ));
                let _ = events.send(event(
                    "session.execution.succeeded",
                    json!({"sessionID": sid}),
                ));
            }
        }
        ("POST", "/api/session/ses_mock1/permission/per_1/reply") => {
            respond(&mut stream, "200 OK", &json!({})).await;
            let _ = events.send(event(
                "session.execution.interrupted",
                json!({"sessionID": sid, "reason": "shutdown"}),
            ));
        }
        ("GET", "/api/session/ses_mock1/message?order=desc&limit=50") => {
            // Newest first; the second page is behind a cursor that needs encoding.
            let page = if scenario.asks_permission {
                json!({"data": [{"id": "msg_x", "type": "assistant", "finish": "error", "content": [],
                                 "error": {"type": "aborted", "message": "Step interrupted"}},
                                {"id": "msg_user", "type": "user"}],
                       "cursor": {"next": null}})
            } else {
                json!({"data": [{"id": "msg_idle", "type": "idle", "outcome": "succeeded"},
                                assistant("stop", "{\"ok\": true}", 30, 5, 0.003)],
                       "cursor": {"next": "page 2"}})
            };
            respond(&mut stream, "200 OK", &page).await
        }
        ("GET", "/api/session/ses_mock1/message?order=desc&limit=50&cursor=page%202") => {
            let page = json!({"data": [assistant("tool-calls", "", 10, 2, 0.001), {"id": "msg_user", "type": "user"},
                                       assistant("stop", "an earlier turn", 99, 99, 9.0)],
                              "cursor": {"next": null}});
            respond(&mut stream, "200 OK", &page).await
        }
        _ => respond(&mut stream, "404 Not Found", &json!({})).await,
    }
}

/// A fake `opencode` that prints `<dir>/state` for `debug paths state` and,
/// for `service start`, copies `<dir>/pending.json` into place.
fn fake_opencode(dir: &Path) -> PathBuf {
    let state = dir.join("state");
    std::fs::create_dir_all(&state).unwrap();
    let script = dir.join("opencode");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\ncase \"$1 $2 $3\" in\n  \"debug paths state\") echo '{s}' ;;\n  \"service start \") \
             cp '{d}/pending.json' '{s}/service.json' && chmod 600 '{s}/service.json' ;;\n  *) exit 3 ;;\nesac\n",
            s = state.display(),
            d = dir.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    script
}

fn discovery(port: u16, password: &str) -> String {
    json!({"id": "svc", "version": "2.0.18", "url": format!("http://127.0.0.1:{port}"), "pid": 1, "password": password})
        .to_string()
}

fn write_discovery(dir: &Path, text: &str) {
    let file = dir.join("state/service.json");
    std::fs::write(&file, text).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "axiomata-opencode-mock-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn env() -> Vec<(String, String)> {
    vec![("PATH".into(), "/usr/bin:/bin".into())]
}

fn turn(directory: &Path) -> TurnRequest {
    TurnRequest {
        session: TurnSession::New(NewSession {
            directory: directory.display().to_string(),
            title: Some("t".into()),
            model: ModelRef::parse("ollama/m"),
            permissions: unattended_permissions(true),
            ..NewSession::default()
        }),
        text: "go".into(),
        timeout: Duration::from_secs(10),
    }
}

#[tokio::test]
async fn a_turn_runs_end_to_end_and_reads_only_its_own_messages() {
    let mock = Mock::start(Scenario {
        version: "2.0.18",
        asks_permission: false,
    })
    .await;
    let dir = scratch("turn");
    let bin = fake_opencode(&dir);
    write_discovery(&dir, &discovery(mock.port, "pw"));

    let service = Service::connect(&bin, &env()).await.expect("connect");
    assert_eq!(service.version(), "2.0.18");
    let outcome = service.run_turn(turn(&dir)).await.expect("turn");

    assert!(outcome.succeeded(), "{:?}", outcome.failure);
    assert_eq!(outcome.session_id, "ses_mock1");
    assert_eq!(
        outcome.turns, 2,
        "the earlier turn behind the prompt is not counted"
    );
    assert_eq!((outcome.input_tokens, outcome.output_tokens), (40, 7));
    assert!((outcome.cost - 0.004).abs() < 1e-12);
    assert_eq!(outcome.reply, "\n{\"ok\": true}");

    let requests = mock.requests();
    let (_, _, created) = requests
        .iter()
        .find(|(m, p, _)| m == "POST" && p == "/api/session")
        .unwrap();
    let created: Value = serde_json::from_str(created).unwrap();
    assert_eq!(
        created["location"]["directory"],
        json!(dir.display().to_string())
    );
    assert_eq!(created["model"], json!({"providerID": "ollama", "id": "m"}));
    assert_eq!(
        created["permissions"][0],
        json!({"action": "*", "resource": "*", "effect": "allow"})
    );
    assert_eq!(created["permissions"][1]["action"], json!("question"));
    let (_, _, prompt) = requests
        .iter()
        .find(|(_, p, _)| p.ends_with("/prompt"))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(prompt).unwrap(),
        json!({"text": "go"})
    );
}

#[tokio::test]
async fn a_permission_request_is_rejected_and_fails_the_turn() {
    let mock = Mock::start(Scenario {
        version: "2.0.18",
        asks_permission: true,
    })
    .await;
    let dir = scratch("perm");
    let bin = fake_opencode(&dir);
    write_discovery(&dir, &discovery(mock.port, "pw"));

    let service = Service::connect(&bin, &env()).await.expect("connect");
    let outcome = service.run_turn(turn(&dir)).await.expect("turn");
    let failure = outcome.failure.expect("fails");
    assert!(
        failure.contains("permission rejected (unattended run): shell: rm -rf x"),
        "{failure}"
    );
    assert!(failure.contains("interrupted (shutdown)"), "{failure}");
    let requests = mock.requests();
    let (_, _, reply) = requests
        .iter()
        .find(|(_, p, _)| p.ends_with("/reply"))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(reply).unwrap(),
        json!({"decision": "reject"})
    );
}

#[tokio::test]
async fn a_wrong_password_is_a_refused_login() {
    let mock = Mock::start(Scenario {
        version: "2.0.18",
        asks_permission: false,
    })
    .await;
    let dir = scratch("login");
    let bin = fake_opencode(&dir);
    write_discovery(&dir, &discovery(mock.port, "wrong"));
    assert!(matches!(
        Service::connect(&bin, &env()).await,
        Err(OpencodeError::Unauthorized)
    ));
}

#[tokio::test]
async fn another_major_version_is_refused() {
    let mock = Mock::start(Scenario {
        version: "3.0.0",
        asks_permission: false,
    })
    .await;
    let dir = scratch("version");
    let bin = fake_opencode(&dir);
    write_discovery(&dir, &discovery(mock.port, "pw"));
    assert!(matches!(
        Service::connect(&bin, &env()).await,
        Err(OpencodeError::UnsupportedVersion { found }) if found == "3.0.0"
    ));
}

#[tokio::test]
async fn a_service_that_is_not_running_is_started_once() {
    let mock = Mock::start(Scenario {
        version: "2.0.18",
        asks_permission: false,
    })
    .await;
    let dir = scratch("start");
    let bin = fake_opencode(&dir);
    // No discovery file yet: `opencode service start` puts it in place.
    std::fs::write(dir.join("pending.json"), discovery(mock.port, "pw")).unwrap();
    let service = Service::connect(&bin, &env())
        .await
        .expect("started and connected");
    assert_eq!(service.version(), "2.0.18");
}

#[tokio::test]
async fn a_service_that_stays_unreachable_is_a_clear_error() {
    let dir = scratch("down");
    let bin = fake_opencode(&dir);
    // `service start` succeeds but points at a port nobody listens on.
    let closed = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    std::fs::write(dir.join("pending.json"), discovery(closed, "pw")).unwrap();
    assert!(matches!(
        Service::connect(&bin, &env()).await,
        Err(OpencodeError::Unavailable(_))
    ));
}

#[tokio::test]
async fn find_connects_to_a_running_service() {
    let mock = Mock::start(Scenario {
        version: "2.0.18",
        asks_permission: false,
    })
    .await;
    let dir = scratch("find");
    let bin = fake_opencode(&dir);
    write_discovery(&dir, &discovery(mock.port, "pw"));
    assert_eq!(
        Service::find(&bin, &env()).await.expect("found").version(),
        "2.0.18"
    );
}

#[tokio::test]
async fn find_never_starts_a_service_that_is_not_running() {
    let mock = Mock::start(Scenario {
        version: "2.0.18",
        asks_permission: false,
    })
    .await;
    let dir = scratch("find-down");
    let bin = fake_opencode(&dir);
    // `service start` would put a working discovery file in place — `find` must not call it.
    std::fs::write(dir.join("pending.json"), discovery(mock.port, "pw")).unwrap();
    assert!(matches!(
        Service::find(&bin, &env()).await,
        Err(OpencodeError::Unavailable(_))
    ));
    assert!(
        !dir.join("state/service.json").exists(),
        "the service was started"
    );
}
