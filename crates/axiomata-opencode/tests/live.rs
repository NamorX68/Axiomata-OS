//! Live checks against the real Opencode service — `#[ignore]`d, run on demand:
//!
//! ```sh
//! AXIOMATA_OPENCODE_LIVE_MODEL=ollama/lfm2.5:8b cargo test -p axiomata-opencode --test live -- --ignored --nocapture
//! ```
//!
//! They create sessions in a temporary directory on the owner's running
//! service (a local model keeps them free) and delete them again.

use std::path::PathBuf;
use std::time::Duration;

use axiomata_opencode::{
    ModelRef, NewSession, Service, TurnRequest, TurnSession, unattended_permissions,
};

/// The model to run on, from `AXIOMATA_OPENCODE_LIVE_MODEL` (`provider/model`).
fn live_model() -> ModelRef {
    let full =
        std::env::var("AXIOMATA_OPENCODE_LIVE_MODEL").expect("set AXIOMATA_OPENCODE_LIVE_MODEL");
    ModelRef::parse(&full).expect("provider/model")
}

/// `opencode` on `PATH`, and the environment the child commands get.
fn opencode() -> (PathBuf, Vec<(String, String)>) {
    let path = std::env::var_os("PATH").expect("PATH");
    let bin = std::env::split_paths(&path)
        .map(|dir| dir.join("opencode"))
        .find(|candidate| candidate.is_file())
        .expect("opencode on PATH");
    (bin, std::env::vars().collect())
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "axiomata-opencode-live-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
#[ignore = "talks to the real Opencode service"]
async fn a_turn_runs_and_is_read_back() {
    let (bin, env) = opencode();
    let service = Service::connect(&bin, &env).await.expect("connect");
    let dir = scratch_dir("turn");
    let outcome = service
        .run_turn(TurnRequest {
            session: TurnSession::New(NewSession {
                directory: dir.display().to_string(),
                title: Some("axiomata-opencode live test".into()),
                model: Some(live_model()),
                permissions: unattended_permissions(true),
                ..NewSession::default()
            }),
            text: "Reply with exactly the word PONG and nothing else.".into(),
            timeout: Duration::from_secs(240),
        })
        .await
        .expect("turn");
    println!("{outcome:#?}");
    assert!(outcome.succeeded(), "{:?}", outcome.failure);
    assert!(outcome.reply.contains("PONG"));
    assert!(outcome.has_usage && outcome.turns >= 1);

    // A second turn in the same session only reads its own messages.
    let second = service
        .run_turn(TurnRequest {
            session: TurnSession::Existing {
                id: outcome.session_id.clone(),
                model: Some(live_model()),
            },
            text: "Now reply with exactly the word PING.".into(),
            timeout: Duration::from_secs(240),
        })
        .await
        .expect("second turn");
    println!("{second:#?}");
    assert!(second.succeeded(), "{:?}", second.failure);
    assert!(second.reply.contains("PING") && !second.reply.contains("PONG"));
    service
        .delete_session(&outcome.session_id)
        .await
        .expect("delete");
}

#[tokio::test]
#[ignore = "talks to the real Opencode service"]
async fn without_auto_approval_a_permission_request_fails_the_turn() {
    let (bin, env) = opencode();
    let service = Service::connect(&bin, &env).await.expect("connect");
    let dir = scratch_dir("perm");
    let mut permissions = unattended_permissions(false);
    permissions.push(axiomata_opencode::PermissionRule::new("shell", "*", "ask"));
    // A small local model does not always call the tool; only a run that did
    // (and so was asked for permission) says anything about the rejection.
    for attempt in 1..=3 {
        let outcome = service
            .run_turn(TurnRequest {
                session: TurnSession::New(NewSession {
                    directory: dir.display().to_string(),
                    title: Some("axiomata-opencode live test (permission)".into()),
                    model: Some(live_model()),
                    permissions: permissions.clone(),
                    ..NewSession::default()
                }),
                text: "Call the shell tool with the command `echo hello`. You must call the tool."
                    .into(),
                timeout: Duration::from_secs(240),
            })
            .await
            .expect("turn");
        println!("attempt {attempt}: {:?}", outcome.failure);
        service
            .delete_session(&outcome.session_id)
            .await
            .expect("delete");
        if let Some(failure) = &outcome.failure
            && failure.contains("permission rejected")
        {
            assert!(failure.contains("shell: echo hello"), "{failure}");
            assert!(failure.contains("interrupted"), "{failure}");
            return;
        }
    }
    panic!("the model never called the shell tool in three attempts");
}

#[tokio::test]
#[ignore = "talks to the real Opencode service"]
async fn the_tracker_follows_a_plan_agent_turn_and_seeding_finds_its_plan() {
    use axiomata_opencode::{EventStream, SessionState, Tracker};

    let (bin, env) = opencode();
    let service = Service::connect(&bin, &env).await.expect("connect");
    let dir = scratch_dir("tracker");
    let session = service
        .create_session(&NewSession {
            directory: dir.display().to_string(),
            title: Some("axiomata-opencode live test (tracker)".into()),
            model: Some(live_model()),
            agent: Some("plan".into()),
            permissions: unattended_permissions(false),
        })
        .await
        .expect("session");
    let mut events = EventStream::open(&service).await.expect("stream");
    service
        .prompt(
            &session,
            "Give a two-step plan for printing hello world in Rust. Do not use any tools.",
        )
        .await
        .expect("prompt");

    let mut tracker = Tracker::default();
    let mut seen = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(240);
    while let Ok(Ok(Some(event))) = tokio::time::timeout_at(deadline, events.next()).await {
        tracker.apply(&event, 0);
        if event.session_id() == Some(session.as_str()) {
            let state = tracker.get(&session).map(|s| s.state);
            if seen.last() != Some(&state) {
                seen.push(state);
            }
            if event.kind.starts_with("session.execution.")
                && event.kind != "session.execution.started"
            {
                break;
            }
        }
    }
    println!("states: {seen:?}");
    assert!(seen.contains(&Some(SessionState::Working)));
    assert_eq!(tracker.get(&session).unwrap().state, SessionState::Idle);
    let plan = tracker
        .get(&session)
        .unwrap()
        .plan
        .clone()
        .expect("the plan agent's answer");
    println!("plan: {}", plan.markdown);

    let active = service.active_sessions().await.expect("active");
    let snapshot = service
        .session_snapshot(&session, &active)
        .await
        .expect("snapshot");
    assert_eq!(snapshot.state, SessionState::Idle);
    assert_eq!(snapshot.plan.map(|p| p.markdown), Some(plan.markdown));
    service.delete_session(&session).await.expect("delete");
}
