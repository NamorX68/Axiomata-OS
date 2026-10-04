//! Whole conversations with the server, through its JSON-RPC front: the same lines a harness would send.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use axiomata_board::{self as board, CardFields, NewCard, flow, store};
use axiomata_ide::lifecycle::ChannelRoots;
use axiomata_ide::presence::{self, Presence};
use axiomata_ide::{AgentFields, Harness, NewAgent, NewProject};
use axiomata_roster::{Limits, Role, Source, Tier};
use serde_json::{Value, json};

use super::protocol::{MAX_LINE_BYTES, handle_line, run};
use super::{Context, ContextError};
use crate::AxiomataCore;
use crate::config::Config;
use crate::db;
use crate::ide::{agent_store, store as project_store};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "axiomata-mcp-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn role(name: &str, kind: &str, creates: &[&str]) -> Role {
    Role {
        name: name.into(),
        description: String::new(),
        kind: kind.into(),
        tier: Tier::Medium,
        engine: None,
        fallback_engines: vec![],
        permissions: vec![],
        limits: Limits::default(),
        creates: creates.iter().map(|s| (*s).to_owned()).collect(),
        instructions: String::new(),
        source: Source::User,
    }
}

fn roles() -> Vec<Role> {
    vec![
        role("builder", "implement", &[]),
        role("tester", "implement", &["test"]),
        role("reviewer", "review", &[]),
        role("planner", "plan", &[]),
    ]
}

/// A database file, a project, a board and the sessions on it.
struct World {
    dir: PathBuf,
    core: AxiomataCore,
    roots: ChannelRoots,
    project: i64,
    board: i64,
    open: i64,
    proposal: i64,
    held: Vec<Presence>,
}

fn core_at(dir: &std::path::Path) -> AxiomataCore {
    let config = Config {
        workspace_root: dir.join("workspace"),
        ..Config::default()
    };
    AxiomataCore {
        config: Arc::new(RwLock::new(config)),
        db: Arc::new(Mutex::new(
            db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap(),
        )),
    }
}

fn world() -> World {
    let dir = temp_dir();
    let core = core_at(&dir);
    let (project, board, open, proposal) = {
        let mut db = core.db_lock();
        let project = project_store::create_project(
            &db,
            NewProject {
                name: "P".into(),
                repo_root: dir.clone(),
            },
        )
        .unwrap();
        let board = store::create_board(&mut db, "B").unwrap();
        let columns = store::list_columns(&db, board.id).unwrap();
        let id = |name: &str| columns.iter().find(|c| c.name == name).unwrap().id;
        (project.id, board.id, id("Offen"), id("Vorschlag"))
    };
    World {
        roots: ChannelRoots {
            events: dir.join("events"),
            claude_tasks: dir.join("tasks"),
            claude_plans: dir.join("plans"),
        },
        dir,
        core,
        project,
        board,
        open,
        proposal,
        held: Vec::new(),
    }
}

impl World {
    /// A session of `role_name`, running (its presence lock is held for the life of the world).
    fn session(&mut self, name: &str, role_name: &str) -> i64 {
        let agent = agent_store::create_agent(
            &self.core.db_lock(),
            NewAgent {
                project_id: self.project,
                fields: AgentFields {
                    name: name.into(),
                    harness: Harness::Opencode,
                    command: String::new(),
                    model: None,
                    env: String::new(),
                },
            },
        )
        .unwrap();
        agent_store::set_role(&self.core.db_lock(), agent.id, role_name).unwrap();
        self.held
            .push(presence::hold(&self.roots, agent.id).unwrap().unwrap());
        agent.id
    }

    fn card(&self, column: i64, title: &str) -> i64 {
        store::create_card(
            &self.core.db_lock(),
            &NewCard {
                column_id: column,
                fields: CardFields {
                    title: title.into(),
                    ..CardFields::default()
                },
            },
        )
        .unwrap()
        .id
    }

    /// A card meant for `agent` (a role), which is what puts it within reach of a session that has no work yet.
    fn card_for(&self, column: i64, title: &str, agent: &str) -> i64 {
        store::create_card(
            &self.core.db_lock(),
            &NewCard {
                column_id: column,
                fields: CardFields {
                    title: title.into(),
                    agent: Some(agent.into()),
                    ..CardFields::default()
                },
            },
        )
        .unwrap()
        .id
    }

    fn plan(&self) -> i64 {
        flow::create_plan(
            &self.core.db_lock(),
            self.board,
            &board::PlanFields {
                name: "Plan".into(),
                auto_start_max: None,
                max_cost_usd: None,
                max_tokens: None,
            },
        )
        .unwrap()
        .id
    }

    fn client(&self, session: i64, card_env: Option<i64>, plan_env: Option<i64>) -> Client {
        Client {
            ctx: Context::new(
                &self.core,
                self.roots.clone(),
                &roles(),
                session,
                card_env,
                plan_env,
            )
            .unwrap(),
        }
    }
}

impl Drop for World {
    fn drop(&mut self) {
        self.held.clear();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Client {
    ctx: Context,
}

impl Client {
    fn rpc(&self, method: &str, params: Value) -> Value {
        let line =
            json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
        serde_json::from_str(&handle_line(&self.ctx, &line).expect("a request is answered"))
            .unwrap()
    }

    /// Calls a tool; `Ok` is the parsed result, `Err` the error text the model would read.
    fn call(&self, tool: &str, args: Value) -> Result<Value, String> {
        let answer = self.rpc("tools/call", json!({"name": tool, "arguments": args}));
        let result = &answer["result"];
        assert!(!result.is_null(), "protocol error for {tool}: {answer}");
        let body = result["content"][0]["text"].as_str().unwrap().to_owned();
        if result["isError"].as_bool().unwrap() {
            Err(body)
        } else {
            Ok(serde_json::from_str(&body).unwrap())
        }
    }

    fn tool_names(&self) -> Vec<String> {
        self.rpc("tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    }
}

// ---------------------------------------------------------------- tests ---

#[test]
fn the_server_negotiates_a_protocol_version_and_names_itself() {
    let mut w = world();
    let me = w.session("a", "builder");
    let c = w.client(me, None, None);
    let answer = c.rpc(
        "initialize",
        json!({"protocolVersion": "2025-03-26", "capabilities": {}}),
    );
    assert_eq!(answer["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(answer["result"]["serverInfo"]["name"], "axiomata");
    assert!(answer["result"]["capabilities"]["tools"].is_object());
    assert!(
        answer["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("read_inbox")
    );
    let unknown = c.rpc("initialize", json!({"protocolVersion": "1999-01-01"}));
    assert_eq!(
        unknown["result"]["protocolVersion"], "2025-11-25",
        "falls back to the newest it knows"
    );
    assert_eq!(c.rpc("ping", json!({}))["result"], json!({}));
}

#[test]
fn the_tools_offered_follow_the_role() {
    let mut w = world();
    let worker = w.session("w", "builder");
    let tester = w.session("t", "tester");
    let reviewer = w.session("r", "reviewer");
    let planner = w.session("p", "planner");
    let ghost = {
        let id = w.session("g", "builder");
        agent_store::set_role(&w.core.db_lock(), id, "no-such-role").unwrap();
        id
    };
    let names = |id| {
        let mut names = w.client(id, None, None).tool_names();
        names.sort();
        names
    };
    let base = [
        "get_card",
        "list_agents",
        "list_cards",
        "read_inbox",
        "send_message",
    ];
    let with = |extra: &[&str]| {
        let mut all: Vec<String> = base.iter().chain(extra).map(|s| (*s).to_owned()).collect();
        all.sort();
        all
    };
    assert_eq!(names(worker), with(&["claim_task", "report_done"]));
    assert_eq!(
        names(tester),
        with(&["claim_task", "create_card", "report_done"])
    );
    assert_eq!(names(reviewer), with(&["review_verdict"]));
    assert_eq!(names(planner), with(&["create_card"]));
    assert_eq!(
        names(ghost),
        with(&[]),
        "a session whose role file is gone can talk and read, nothing else"
    );

    // None of them has a tool for the owner's gates.
    for id in [worker, tester, reviewer, planner] {
        for forbidden in [
            "release_held",
            "approve_plan",
            "approve_proposal",
            "take_over",
            "move_card",
        ] {
            assert!(!names(id).contains(&forbidden.to_owned()));
        }
    }
}

#[test]
fn a_tool_that_is_not_offered_cannot_be_called_by_name() {
    let mut w = world();
    let worker = w.session("w", "builder");
    let card = w.card(w.open, "k");
    let c = w.client(worker, None, None);
    let answer = c.rpc(
        "tools/call",
        json!({"name": "review_verdict", "arguments": {"card_id": card, "verdict": "approve"}}),
    );
    assert_eq!(answer["error"]["code"], -32602, "{answer}");
    let answer = c.rpc(
        "tools/call",
        json!({"name": "create_card", "arguments": {"title": "x", "kind": "doc"}}),
    );
    assert_eq!(answer["error"]["code"], -32602, "{answer}");
}

#[test]
fn a_card_goes_through_claim_report_and_review() {
    let mut w = world();
    let worker = w.session("worker", "builder");
    let reviewer = w.session("rev", "reviewer");
    let card = w.card(w.open, "Do the thing");
    let working = w.client(worker, Some(card), None);
    let judging = w.client(reviewer, Some(card), None);

    let claimed = working.call("claim_task", json!({})).unwrap();
    assert_eq!(claimed["result"], "started");
    assert_eq!(claimed["card"]["state"], "working");
    assert_eq!(
        working.call("claim_task", json!({})).unwrap()["result"],
        "already_held"
    );

    // The reviewer cannot judge before it is reported, and not its own card's way around.
    let early = judging
        .call("review_verdict", json!({"verdict": "approve"}))
        .unwrap_err();
    assert!(early.contains("not in the review column"), "{early}");

    working
        .call("report_done", json!({"summary": "did it"}))
        .unwrap();
    let seen = judging.call("get_card", json!({})).unwrap();
    assert_eq!(seen["card"]["state"], "in_review");
    assert!(seen["history"].to_string().contains("summary: did it"));

    let needs_reason = judging
        .call("review_verdict", json!({"verdict": "return"}))
        .unwrap_err();
    assert!(needs_reason.contains("why"), "{needs_reason}");
    judging
        .call(
            "review_verdict",
            json!({"verdict": "return", "note": "tests missing"}),
        )
        .unwrap();
    assert_eq!(
        working.call("get_card", json!({})).unwrap()["card"]["returned_count"],
        1
    );
    // The worker is told by the studio, with the reviewer's words, and not by the reviewer.
    let inbox = working.call("read_inbox", json!({})).unwrap();
    assert_eq!(inbox["count"], 1, "{inbox}");
    let told = inbox["messages"][0].to_string();
    assert!(
        told.contains("tests missing") && told.contains("sent it back"),
        "{told}"
    );
    assert!(told.contains("studio") || told.contains("system"), "{told}");

    working.call("report_done", json!({})).unwrap();
    judging
        .call(
            "review_verdict",
            json!({"verdict": "approve", "note": "fine"}),
        )
        .unwrap();
    let done = working.call("get_card", json!({})).unwrap();
    assert_eq!(done["card"]["state"], "verified");
    // An approval needs no action from the worker, so it gets no notice.
    assert_eq!(working.call("read_inbox", json!({})).unwrap()["count"], 0);
}

#[test]
fn a_reviewer_of_an_earlier_report_cannot_judge_the_next_one() {
    let mut w = world();
    let worker = w.session("worker", "builder");
    let card = w.card(w.open, "Do the thing");
    let working = w.client(worker, Some(card), None);
    working.call("claim_task", json!({})).unwrap();
    working.call("report_done", json!({})).unwrap();

    // The studio made the first reviewer for the first report; it sends the card back.
    let first = w.session("rev-1", "reviewer");
    agent_store::set_card(
        &w.core.db_lock(),
        first,
        Some(card),
        true,
        Some(&"a".repeat(40)),
    )
    .unwrap();
    let first_client = w.client(first, Some(card), None);
    first_client
        .call(
            "review_verdict",
            json!({"verdict": "return", "note": "more"}),
        )
        .unwrap();
    working.call("report_done", json!({})).unwrap();

    // The second report has its own reviewer; the first one's pane is still open and tries again.
    std::thread::sleep(std::time::Duration::from_millis(5));
    let second = w.session("rev-2", "reviewer");
    agent_store::set_card(
        &w.core.db_lock(),
        second,
        Some(card),
        true,
        Some(&"b".repeat(40)),
    )
    .unwrap();
    let stale = first_client
        .call("review_verdict", json!({"verdict": "approve"}))
        .unwrap_err();
    assert!(stale.contains("earlier report"), "{stale}");
    assert_eq!(
        working.call("get_card", json!({})).unwrap()["card"]["state"],
        "in_review"
    );

    let current = w.client(second, Some(card), None);
    current
        .call("review_verdict", json!({"verdict": "approve"}))
        .unwrap();
    assert_eq!(
        working.call("get_card", json!({})).unwrap()["card"]["state"],
        "verified"
    );
}

#[test]
fn a_session_works_on_its_own_card_and_one_card_at_a_time() {
    let mut w = world();
    let worker = w.session("worker", "builder");
    let mine = w.card_for(w.open, "mine", "builder");
    let other = w.card_for(w.open, "other", "builder");

    // Started for card `mine`: the other card is not its business.
    let bound = w.client(worker, Some(mine), None);
    let wrong = bound
        .call("claim_task", json!({"card_id": other}))
        .unwrap_err();
    assert!(wrong.contains(&format!("#{mine}")), "{wrong}");

    // A session without a card takes one; once it holds one, that is its card and no other is its business.
    let free = w.client(worker, None, None);
    assert!(
        free.call("claim_task", json!({}))
            .unwrap_err()
            .contains("name a card")
    );
    free.call("claim_task", json!({"card_id": mine})).unwrap();
    let second = free
        .call("claim_task", json!({"card_id": other}))
        .unwrap_err();
    assert!(
        second.contains(&format!("works on card #{mine}")),
        "{second}"
    );
    // Started for another card while it still holds this one: one at a time.
    let rebound = w.client(worker, Some(other), None);
    let busy = rebound.call("claim_task", json!({})).unwrap_err();
    assert!(busy.contains("already hold"), "{busy}");
    // …and once it holds one, that is its context without being told.
    assert_eq!(
        free.call("get_card", json!({})).unwrap()["card"]["id"],
        mine
    );
}

#[test]
fn a_card_somebody_else_holds_or_that_is_not_ready_is_refused_with_the_reason() {
    let mut w = world();
    let a = w.session("a", "builder");
    let b = w.session("b", "builder");
    let held = w.card_for(w.open, "held", "builder");
    let proposed = w.card_for(w.proposal, "proposed", "builder");
    w.client(a, None, None)
        .call("claim_task", json!({"card_id": held}))
        .unwrap();
    let c = w.client(b, None, None);
    assert!(
        c.call("claim_task", json!({"card_id": held}))
            .unwrap_err()
            .contains("held by")
    );
    assert!(
        c.call("claim_task", json!({"card_id": proposed}))
            .unwrap_err()
            .contains("proposal")
    );
    assert!(
        c.call("claim_task", json!({"card_id": 9999}))
            .unwrap_err()
            .contains("no card")
    );
}

#[test]
fn two_servers_racing_for_a_card_produce_one_winner() {
    let mut w = world();
    let a = w.session("a", "builder");
    let b = w.session("b", "builder");
    let card = w.card_for(w.open, "k", "builder");
    // Two processes, each with its own connection to the one file.
    let other_core = core_at(&w.dir);
    let ctx_b = Context::new(&other_core, w.roots.clone(), &roles(), b, None, None).unwrap();
    let ctx_a = Context::new(&w.core, w.roots.clone(), &roles(), a, None, None).unwrap();
    let threads: Vec<_> = [ctx_a, ctx_b]
        .into_iter()
        .map(|ctx| {
            std::thread::spawn(move || {
                ctx.db()
                    .busy_timeout(std::time::Duration::from_secs(10))
                    .unwrap();
                Client { ctx }
                    .call("claim_task", json!({"card_id": card}))
                    .is_ok()
            })
        })
        .collect();
    let winners = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .filter(|won| *won)
        .count();
    assert_eq!(winners, 1);
}

#[test]
fn messages_reach_the_inbox_stamped_with_the_session_and_its_card() {
    let mut w = world();
    let worker = w.session("worker", "builder");
    let reviewer = w.session("rev", "reviewer");
    let card = w.card(w.open, "k");
    let from = w.client(worker, Some(card), None);
    let to = w.client(reviewer, Some(card), None);

    // Whatever the model claims about sender or card is ignored: there is no such argument.
    let sent = from
        .call(
            "send_message",
            json!({"to": "role:reviewer", "text": "please look", "sender": "owner",
            "card_id": 999, "from": "session:1"}),
        )
        .unwrap();
    assert_eq!(sent["outcome"], "delivered");
    assert_eq!(sent["delivered_to"], json!([format!("session:{reviewer}")]));

    let inbox = to.call("read_inbox", json!({})).unwrap();
    assert_eq!(inbox["count"], 1);
    let message = &inbox["messages"][0];
    assert_eq!(message["from"]["address"], format!("session:{worker}"));
    assert_eq!(message["from"]["name"], "worker");
    assert_eq!(message["from"]["role"], "builder");
    assert_eq!(
        message["card_id"], card,
        "the session's own card, not the argument"
    );
    assert_eq!(
        message["parts"][0],
        json!({"kind": "text", "text": "please look"})
    );
    assert_eq!(
        to.call("read_inbox", json!({})).unwrap()["count"],
        0,
        "read once"
    );
    assert_eq!(
        to.call("read_inbox", json!({"unread_only": false}))
            .unwrap()["count"],
        1
    );

    // Answering without naming the message links it, so the chain counter sees it.
    let reply = to
        .call(
            "send_message",
            json!({"to": format!("session:{worker}"), "text": "ok"}),
        )
        .unwrap();
    let back = from.call("read_inbox", json!({})).unwrap();
    assert_eq!(back["messages"][0]["in_reply_to"], message["message_id"]);
    assert_eq!(back["messages"][0]["message_id"], reply["message_id"]);
}

#[test]
fn files_and_data_travel_as_parts() {
    let mut w = world();
    let a = w.session("a", "builder");
    let b = w.session("b", "builder");
    let sent = w
        .client(a, None, None)
        .call(
            "send_message",
            json!({
                "to": format!("session:{b}"),
                "files": [{"name": "diff", "uri": "src/lib.rs"}],
                "data": {"coverage": 0.8},
            }),
        )
        .unwrap();
    assert_eq!(sent["outcome"], "delivered");
    let parts = &w
        .client(b, None, None)
        .call("read_inbox", json!({}))
        .unwrap()["messages"][0]["parts"];
    assert_eq!(parts[0]["kind"], "file");
    assert_eq!(parts[1], json!({"kind": "data", "data": {"coverage": 0.8}}));

    let c = w.client(a, None, None);
    assert!(
        c.call("send_message", json!({"to": "owner"}))
            .unwrap_err()
            .contains("at least one part")
    );
    assert!(
        c.call("send_message", json!({"to": "somebody", "text": "x"}))
            .unwrap_err()
            .contains("`to`")
    );
    assert!(
        c.call("send_message", json!({"to": "owner", "files": "x"}))
            .unwrap_err()
            .contains("list")
    );
    let too_many: Vec<Value> = (0..11)
        .map(|i| json!({"name": format!("f{i}"), "uri": "x"}))
        .collect();
    assert!(
        c.call("send_message", json!({"to": "owner", "files": too_many}))
            .unwrap_err()
            .contains("at most")
    );
}

#[test]
fn mail_to_a_session_that_is_not_running_comes_back() {
    let mut w = world();
    let a = w.session("a", "builder");
    let gone = w.session("gone", "reviewer");
    // Its server ended: the presence lock is released.
    w.held.remove(1);
    let c = w.client(a, None, None);
    let sent = c
        .call(
            "send_message",
            json!({"to": format!("session:{gone}"), "text": "hello?"}),
        )
        .unwrap();
    assert_eq!(sent["outcome"], "undeliverable");
    let notice = c.call("read_inbox", json!({})).unwrap();
    assert_eq!(notice["messages"][0]["kind"], "notice");
    assert_eq!(notice["messages"][0]["from"]["address"], "system");

    let listed = c.call("list_agents", json!({})).unwrap();
    let running = |name: &str| {
        listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap()["running"]
            .clone()
    };
    assert_eq!((running("a"), running("gone")), (json!(true), json!(false)));
}

#[test]
fn an_endless_conversation_is_stopped_and_the_owner_asked() {
    let mut w = world();
    let a = w.session("a", "builder");
    let b = w.session("b", "reviewer");
    let (ca, cb) = (w.client(a, None, None), w.client(b, None, None));
    ca.call(
        "send_message",
        json!({"to": format!("session:{b}"), "text": "q"}),
    )
    .unwrap();
    let mut outcome = String::new();
    for round in 0..10 {
        let (from, target) = if round % 2 == 0 { (&cb, a) } else { (&ca, b) };
        let sent = from
            .call(
                "send_message",
                json!({"to": format!("session:{target}"), "text": "again"}),
            )
            .unwrap();
        outcome = sent["outcome"].as_str().unwrap().to_owned();
        if outcome == "held" {
            break;
        }
    }
    assert_eq!(outcome, "held");
    let db = w.core.db_lock();
    assert!(
        axiomata_ide::mailbox::unread_count(&db, &axiomata_ide::mailbox::Inbox::Owner).unwrap()
            >= 1
    );
}

#[test]
fn a_planner_proposes_cards_into_the_proposal_column_of_its_plan() {
    let mut w = world();
    let planner = w.session("plan", "planner");
    let plan = w.plan();
    let c = w.client(planner, None, Some(plan));

    let first = c
        .call(
            "create_card",
            json!({"title": "Build it", "kind": "implement",
        "tier": "light", "agent": "builder", "acceptance": "works"}),
        )
        .unwrap();
    let second = c
        .call(
            "create_card",
            json!({"title": "Test it", "kind": "test", "agent": "builder", "needs": [first["card_id"]]}),
        )
        .unwrap();
    assert_eq!(second["state"], "proposed");

    let db = w.core.db_lock();
    let card = store::get_card(&db, second["card_id"].as_i64().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!((card.column_id, card.plan_id), (w.proposal, Some(plan)));
    assert_eq!(card.depends_on, vec![first["card_id"].as_i64().unwrap()]);
    let built = store::get_card(&db, first["card_id"].as_i64().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            built.tier,
            built.agent.as_deref(),
            built.acceptance.as_str()
        ),
        (Some(board::Tier::Light), Some("builder"), "works")
    );
    drop(db);

    // Nothing takes a proposal before the owner says yes.
    let worker = w.session("w", "builder");
    let id = second["card_id"].as_i64().unwrap();
    assert!(
        w.client(worker, None, None)
            .call("claim_task", json!({"card_id": id}))
            .unwrap_err()
            .contains("proposal")
    );

    // No plan, no card: nowhere to put it.
    let lost = w.client(planner, None, None);
    assert!(
        lost.call("create_card", json!({"title": "x", "kind": "doc"}))
            .unwrap_err()
            .contains("no card or plan")
    );
}

#[test]
fn a_role_proposes_only_the_kinds_it_is_allowed() {
    let mut w = world();
    let tester = w.session("t", "tester");
    let card = w.card(w.open, "source");
    let plan = w.plan();
    store::update_card(
        &w.core.db_lock(),
        card,
        &CardFields {
            title: "source".into(),
            plan_id: Some(plan),
            ..CardFields::default()
        },
    )
    .unwrap();
    let c = w.client(tester, Some(card), None);

    let refused = c
        .call("create_card", json!({"title": "x", "kind": "implement"}))
        .unwrap_err();
    assert!(refused.contains("test"), "{refused}");
    let made = c
        .call("create_card", json!({"title": "Add tests", "kind": "test"}))
        .unwrap();
    let db = w.core.db_lock();
    let new = store::get_card(&db, made["card_id"].as_i64().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        (new.column_id, new.plan_id),
        (w.proposal, Some(plan)),
        "the plan of the card it grew out of"
    );
    assert!(
        flow::list_events(&db, new.id, 5)
            .unwrap()
            .iter()
            .any(|e| e.actor.starts_with("agent:t-")),
        "the history says who proposed it"
    );
}

#[test]
fn a_proposal_with_a_bad_dependency_is_not_left_behind() {
    let mut w = world();
    let planner = w.session("plan", "planner");
    let plan = w.plan();
    let stranger = w.card(w.open, "not in the plan");
    let c = w.client(planner, None, Some(plan));
    let before = store::list_cards(&w.core.db_lock(), w.board, false)
        .unwrap()
        .len();
    let err = c
        .call(
            "create_card",
            json!({"title": "x", "kind": "doc", "needs": [stranger]}),
        )
        .unwrap_err();
    assert!(err.contains("same plan"), "{err}");
    assert_eq!(
        store::list_cards(&w.core.db_lock(), w.board, false)
            .unwrap()
            .len(),
        before
    );
}

#[test]
fn list_cards_defaults_to_the_board_of_the_session() {
    let mut w = world();
    let a = w.session("a", "builder");
    let card = w.card(w.open, "one");
    w.card(w.open, "two");
    let c = w.client(a, Some(card), None);
    let listed = c.call("list_cards", json!({})).unwrap();
    assert_eq!(listed["board_id"], w.board);
    assert_eq!(listed["cards"].as_array().unwrap().len(), 2);
    let nowhere = w
        .client(a, None, None)
        .call("list_cards", json!({}))
        .unwrap_err();
    assert!(nowhere.contains("no board"), "{nowhere}");
}

#[test]
fn bad_arguments_are_answered_as_tool_errors() {
    let mut w = world();
    let a = w.session("a", "builder");
    let c = w.client(a, None, None);
    for (tool, args) in [
        ("read_inbox", json!({"limit": -1})),
        ("read_inbox", json!({"unread_only": "yes"})),
        ("get_card", json!({"card_id": "one"})),
        ("send_message", json!({"to": 5, "text": "x"})),
    ] {
        assert!(c.call(tool, args.clone()).is_err(), "{tool} {args}");
    }
    let answer = c.rpc(
        "tools/call",
        json!({"name": "get_card", "arguments": "not an object"}),
    );
    assert_eq!(answer["error"]["code"], -32602);
    let answer = c.rpc("tools/call", json!({"arguments": {}}));
    assert_eq!(answer["error"]["code"], -32602);
}

#[test]
fn a_session_reaches_only_the_cards_of_its_own_board_or_role() {
    let mut w = world();
    let worker = w.session("worker", "builder");
    let reviewer = w.session("rev", "reviewer");
    // The owner's other board, with a card nobody assigned to anyone.
    let other_board = {
        let mut db = w.core.db_lock();
        let board = store::create_board(&mut db, "Owner's to-dos").unwrap();
        store::list_columns(&db, board.id)
            .unwrap()
            .into_iter()
            .find(|c| c.name == "Offen")
            .unwrap()
    };
    let private = w.card(other_board.id, "buy milk");
    let mine = w.card_for(w.open, "mine", "builder");
    let for_others = w.card_for(w.open, "for the tester", "tester");
    let working = w.client(worker, Some(mine), None);

    for tool in ["get_card", "claim_task"] {
        let err = working.call(tool, json!({"card_id": private})).unwrap_err();
        assert!(
            err.contains("another board") || err.contains("works on card"),
            "{tool}: {err}"
        );
    }
    assert!(
        working
            .call("list_cards", json!({"board_id": other_board.board_id}))
            .unwrap_err()
            .contains("not the board")
    );
    assert!(
        working
            .call("get_card", json!({"card_id": for_others}))
            .is_ok(),
        "same board: readable"
    );

    // A session without work reads only what is assigned to its role, and cannot take what is meant for another one.
    let idle = w.client(worker, None, None);
    assert!(
        idle.call("get_card", json!({"card_id": private}))
            .unwrap_err()
            .contains("assigned to your role")
    );
    assert!(
        idle.call("get_card", json!({"card_id": for_others}))
            .is_err()
    );
    assert!(idle.call("get_card", json!({"card_id": mine})).is_ok());
    assert!(
        idle.call("list_cards", json!({}))
            .unwrap_err()
            .contains("no board")
    );
    let tester_card = w.client(worker, Some(for_others), None);
    let wrong_role = tester_card.call("claim_task", json!({})).unwrap_err();
    assert!(
        wrong_role.contains("meant for the role tester"),
        "{wrong_role}"
    );

    // A reviewer who was not started for a card cannot judge cards of other boards either.
    let judging = w.client(reviewer, None, None);
    assert!(
        judging
            .call(
                "review_verdict",
                json!({"card_id": private, "verdict": "approve"})
            )
            .is_err()
    );
}

#[test]
fn a_notification_runs_nothing_and_an_answer_gets_none() {
    let mut w = world();
    let a = w.session("a", "builder");
    let b = w.session("b", "builder");
    let c = w.client(a, None, None);
    // `tools/call` without an id is a notification: it must not send the message.
    let line = json!({"jsonrpc": "2.0", "method": "tools/call",
                      "params": {"name": "send_message", "arguments": {"to": format!("session:{b}"), "text": "x"}}})
    .to_string();
    assert!(handle_line(&c.ctx, &line).is_none());
    assert_eq!(
        w.client(b, None, None)
            .call("read_inbox", json!({}))
            .unwrap()["count"],
        0
    );
    // A client's answer to something is not answered.
    assert!(handle_line(&c.ctx, r#"{"jsonrpc":"2.0","id":9,"result":{}}"#).is_none());
    assert!(
        handle_line(
            &c.ctx,
            r#"{"jsonrpc":"2.0","id":9,"error":{"code":1,"message":"x"}}"#
        )
        .is_none()
    );
    // A request without a method is a bad request.
    let bad: Value =
        serde_json::from_str(&handle_line(&c.ctx, r#"{"jsonrpc":"2.0","id":9}"#).unwrap()).unwrap();
    assert_eq!(bad["error"]["code"], -32600);
}

#[test]
fn a_refused_summary_moves_nothing_and_a_reply_cannot_be_steered() {
    let mut w = world();
    let worker = w.session("worker", "builder");
    let peer = w.session("peer", "builder");
    let card = w.card_for(w.open, "k", "builder");
    let c = w.client(worker, Some(card), None);
    c.call("claim_task", json!({})).unwrap();
    let err = c
        .call("report_done", json!({"summary": "x".repeat(5000)}))
        .unwrap_err();
    assert!(err.contains("longer than"), "{err}");
    assert_eq!(
        c.call("get_card", json!({})).unwrap()["card"]["state"],
        "working",
        "still in work, retry possible"
    );
    c.call("report_done", json!({"summary": "short"})).unwrap();

    // `in_reply_to` is no argument of the tool: naming a message changes nothing.
    let sent = c
        .call(
            "send_message",
            json!({"to": format!("session:{peer}"), "text": "hi", "in_reply_to": 1}),
        )
        .unwrap();
    assert_eq!(sent["outcome"], "delivered");
    let seen = w
        .client(peer, None, None)
        .call("read_inbox", json!({}))
        .unwrap();
    assert_eq!(seen["messages"][0]["in_reply_to"], Value::Null);
}

#[test]
fn a_session_proposes_a_limited_number_of_cards() {
    let mut w = world();
    let planner = w.session("plan", "planner");
    let plan = w.plan();
    let c = w.client(planner, None, Some(plan));
    for n in 0..flow::MAX_PROPOSALS_PER_ACTOR {
        c.call(
            "create_card",
            json!({"title": format!("p{n}"), "kind": "doc"}),
        )
        .unwrap();
    }
    let err = c
        .call(
            "create_card",
            json!({"title": "one too many", "kind": "doc"}),
        )
        .unwrap_err();
    assert!(err.contains("proposed"), "{err}");
}

// -------------------------------------------------------------- framing ---

#[test]
fn the_stream_loop_answers_requests_and_stays_silent_for_notifications() {
    let mut w = world();
    let a = w.session("a", "builder");
    let c = w.client(a, None, None);
    let input = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "",
        r#"{"jsonrpc":"2.0","id":"two","method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"nope"}"#,
        "not json",
        r#"[{"jsonrpc":"2.0","id":4,"method":"ping"}]"#,
        r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#,
    ]
    .join("\n");
    let mut output = Vec::new();
    run(&c.ctx, Cursor::new(input), &mut output).unwrap();
    let answers: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(answers.len(), 6, "{answers:?}");
    assert_eq!(answers[0]["id"], 1);
    assert_eq!(answers[1]["id"], "two");
    assert_eq!(answers[2]["error"]["code"], -32601);
    assert_eq!(answers[3]["error"]["code"], -32700);
    assert_eq!(
        answers[4]["error"]["code"], -32600,
        "batches are not served"
    );
    assert_eq!(answers[5]["id"], 5, "the loop carried on after the errors");
}

#[test]
fn an_oversized_line_is_refused_and_the_next_one_still_works() {
    let mut w = world();
    let a = w.session("a", "builder");
    let c = w.client(a, None, None);
    let huge = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"ping","params":{{"x":"{}"}}}}"#,
        "a".repeat(MAX_LINE_BYTES)
    );
    let input = format!(
        "{huge}\n{}\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#
    );
    let mut output = Vec::new();
    run(&c.ctx, Cursor::new(input), &mut output).unwrap();
    let answers: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(answers.len(), 2);
    assert_eq!(answers[0]["error"]["code"], -32600);
    assert_eq!(answers[1]["id"], 2);
}

#[test]
fn a_server_needs_a_real_session() {
    let w = world();
    let err = Context::new(&w.core, w.roots.clone(), &roles(), 4242, None, None)
        .err()
        .expect("no such session");
    assert!(matches!(err, ContextError::UnknownSession(4242)));
}

/// What a harness would pass the server: the variables of the MCP configuration, nothing from the shell.
fn vars(pairs: &[(&str, String)]) -> impl Fn(&str) -> Option<String> {
    let pairs: Vec<(String, String)> = pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect();
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }
}

#[test]
fn a_server_starts_only_with_the_secret_of_the_latest_start() {
    let mut w = world();
    let reviewer = w.session("rev", "reviewer");
    let worker = w.session("dev", "worker");
    let secret = axiomata_ide::session_token::issue(&w.roots, reviewer).unwrap();
    let id = reviewer.to_string();

    let start = |pairs: &[(&str, String)]| {
        Context::from_vars(&w.core, w.roots.clone(), vars(pairs))
            .err()
            .map(|err| err.to_string())
    };
    // The right id with the right secret is the only way in.
    assert!(
        Context::from_vars(
            &w.core,
            w.roots.clone(),
            vars(&[
                ("AXIOMATA_AGENT_ID", id.clone()),
                ("AXIOMATA_AGENT_TOKEN", secret.clone())
            ])
        )
        .is_ok()
    );
    // The forgery the secret exists for: the worker exports the reviewer's id, with no secret or its own.
    let worker_secret = axiomata_ide::session_token::issue(&w.roots, worker).unwrap();
    for token in [None, Some(String::new()), Some(worker_secret)] {
        let mut pairs = vec![("AXIOMATA_AGENT_ID", id.clone())];
        if let Some(token) = token {
            pairs.push(("AXIOMATA_AGENT_TOKEN", token));
        }
        let err = start(&pairs).expect("must be refused");
        assert!(err.contains("session secret"), "{err}");
    }
    // An id without any session behind it is refused the same way (no hash file) — nothing to probe for.
    assert!(
        start(&[
            ("AXIOMATA_AGENT_ID", "999".into()),
            ("AXIOMATA_AGENT_TOKEN", secret.clone())
        ])
        .is_some()
    );
    // A restart of the session invalidates the secret of the earlier start.
    axiomata_ide::session_token::issue(&w.roots, reviewer).unwrap();
    assert!(start(&[("AXIOMATA_AGENT_ID", id), ("AXIOMATA_AGENT_TOKEN", secret)]).is_some());
    // No id at all is still its own message.
    assert!(start(&[]).unwrap().contains("AXIOMATA_AGENT_ID is not set"));
}

#[test]
fn the_tool_names_shown_to_the_owner_are_the_ones_the_server_offers() {
    let mut w = world();
    let mut seen = Vec::new();
    for (name, role_name) in [
        ("a", "builder"),
        ("b", "reviewer"),
        ("c", "planner"),
        ("d", "tester"),
        ("e", "ghost"),
    ] {
        let id = w.session(name, role_name);
        let client = w.client(id, None, None);
        let mut served = client.tool_names();
        let mut shown: Vec<String> = client
            .ctx
            .caps
            .tool_names()
            .into_iter()
            .map(str::to_owned)
            .collect();
        served.sort();
        shown.sort();
        assert_eq!(shown, served, "role {role_name}");
        seen.push(served.len());
    }
    assert!(
        seen.iter().any(|n| n != &seen[0]),
        "the roles differ in what they get"
    );
}
