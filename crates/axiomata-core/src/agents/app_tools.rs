//! The app's own tools for a Glut chat turn (`docs/plans/chat-knows-app.md`, CH3/CH5).
//!
//! Glut runs a chat turn's shell in its sandbox, which hides `~/.axiomata`, and it starts the CLI directly only for a
//! command the turn *declares* — a chat turn declares none (Glut's `Gate::declares_command`). So the chat would reach
//! neither `axiomata-cli app …` nor `module-action`. These tools give it the same in the app's own process:
//! `axiomata_app` reads the handbook and the live views (`crate::app_state`), the same text the CLI prints, and
//! `axiomata_action` calls the chat's shell actions through the app's action queue (CH5).
//!
//! `axiomata_app` is judged as a read ([`Access::Read`]): allowed in every mode, plan mode included, and still subject
//! to the owner's deny rules. It is offered only to a chat turn that declares no tools of its own — an instruct turn's
//! declaration stays its whole set. `axiomata_action` is judged by the turn's mode like any other tool; what it may
//! call is a fixed list, and the app asks the owner before a skill, a routine or a card. Neither writes the database.

use std::sync::Mutex;

use glut::ToolSpec;
use glut::permissions::Access;
use glut::tools::{CallContext, Tool, ToolFuture};
use serde_json::{Value, json};

use crate::app_state::{self, Request, Topic};
use crate::config::Config;
use crate::{handbook, paths};

/// The read tool's name, as the model calls it.
pub const APP_TOOL: &str = "axiomata_app";

/// The operating tool's name.
pub const ACTION_TOOL: &str = "axiomata_action";

/// The shell actions the operating tool may call (`apps/axiomata/src/modules/chatActions.ts` and `openFile`): views
/// and display at once, closing a view at once only when nothing would be lost (else after the owner's yes), a skill,
/// a routine and a card only after the owner's yes in the app — the app decides that by each action's `chat` mark,
/// whoever calls. Nothing that deletes or steers is in the list (the Studio's panes are never closed), and a tile's own
/// actions stay where they were (the Claude Code chat's `module-action`).
pub const CHAT_SHELL_ACTIONS: [&str; 8] = [
    "open_view",
    "open_views",
    "close_view",
    "set_display",
    "openFile",
    "start_skill",
    "save_routine",
    "start_card",
];

/// How long the operating tool waits for the app to take the action — an asked one answers at once that it asked.
const ACTION_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// The tools a chat turn on Glut gets beside its own.
pub fn chat_tools(config: &Config) -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(AppTool {
            config: config.clone(),
        }),
        Box::new(ActionTool),
    ]
}

/// `axiomata_action`: one of [`CHAT_SHELL_ACTIONS`], through the app's action queue (`crate::bridge`), as the CLI's
/// `module-action shell …` sends it — the sandbox hides that queue from the turn's shell.
struct ActionTool;

/// The action and its params, read from a call's arguments; an action outside the list is refused.
fn action_call(arguments: &Value) -> Result<(String, Value), String> {
    let action = arguments["action"].as_str().unwrap_or_default();
    if !CHAT_SHELL_ACTIONS.contains(&action) {
        return Err(format!(
            "the chat operates the app with these actions only: {}",
            CHAT_SHELL_ACTIONS.join(", ")
        ));
    }
    let params = match &arguments["params"] {
        Value::Null => json!({}),
        Value::Object(_) => arguments["params"].clone(),
        _ => return Err("params must be a JSON object".to_owned()),
    };
    Ok((action.to_owned(), params))
}

/// Hands the action to the running app and waits for its answer.
fn dispatch(action: String, params: Value) -> Result<String, String> {
    let request = crate::bridge::ActionRequest {
        id: crate::bridge::new_action_id(),
        instance_id: "shell".to_owned(),
        action,
        params,
        created_at: chrono::Utc::now(),
        origin: Some(crate::agent_entry::HARNESS_RUN_CHAT.to_owned()),
    };
    crate::bridge::enqueue(&request).map_err(|err| err.to_string())?;
    let response = crate::bridge::wait_for_response(&request.id, ACTION_WAIT)
        .map_err(|err| err.to_string())?;
    if response.ok {
        Ok(response.result.unwrap_or(Value::Null).to_string())
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "the action failed".to_owned()))
    }
}

impl Tool for ActionTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: ACTION_TOOL.to_owned(),
            description: format!(
                "Operate the Axiomata app for the owner: one of the Shell actions of your instructions ({}), with its \
                 params as a JSON object. open_view, open_views and set_display act at once; close_view closes at \
                 once when nothing would be lost, otherwise it asks the owner; start_skill, save_routine and \
                 start_card ask the owner in the app and run only on their yes — say that you asked. Deleting, keys, \
                 the Studio's agents and panes and the owner's board steps are not yours.",
                CHAT_SHELL_ACTIONS.join(", ")
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": CHAT_SHELL_ACTIONS },
                    "params": { "type": "object" }
                },
                "required": ["action"]
            }),
        }
    }

    fn access(&self, _arguments: &Value) -> Access {
        // Judged by the turn's mode as any other tool (security audit): with pre-approved tools it runs, otherwise the
        // chat — which nobody answers mid-turn — is refused, and plan mode never operates. The app's own yes for a
        // skill, a routine or a card comes on top.
        Access::Other
    }

    fn call(&self, arguments: Value, _context: CallContext) -> ToolFuture<'_> {
        Box::pin(async move {
            let (action, params) = action_call(&arguments)?;
            // The queue is files and a polling wait: off the run's thread.
            tokio::task::spawn_blocking(move || dispatch(action, params))
                .await
                .map_err(|err| format!("the action failed: {err}"))?
        })
    }
}

/// `axiomata_app`: the handbook and the live views, read-only.
struct AppTool {
    config: Config,
}

/// What a call asks for, read from its arguments.
#[derive(Debug, PartialEq, Eq)]
enum Query {
    /// The table of contents, or one page.
    Handbook(Option<String>),
    View(Request),
}

impl Query {
    fn of(arguments: &Value) -> Result<Self, String> {
        let topic = arguments["topic"].as_str().unwrap_or_default();
        if topic == "handbook" {
            let name = arguments["name"]
                .as_str()
                .map(str::to_owned)
                .filter(|name| !name.trim().is_empty());
            return Ok(Query::Handbook(name));
        }
        let topic = Topic::parse(topic).ok_or_else(|| {
            format!(
                "unknown topic {topic:?}; one of: handbook, {}",
                topic_names().join(", ")
            )
        })?;
        let limit = arguments["limit"]
            .as_u64()
            .and_then(|limit| usize::try_from(limit).ok())
            .unwrap_or(app_state::DEFAULT_RUNS);
        Ok(Query::View(Request {
            topic,
            board: arguments["board"].as_i64(),
            limit,
        }))
    }
}

fn topic_names() -> Vec<&'static str> {
    Topic::ALL.into_iter().map(Topic::name).collect()
}

/// The answer to `query`: the handbook from the build, a view from the database and the config.
fn answer(config: &Config, query: Query) -> Result<String, String> {
    match query {
        // In the owner's language, as the chat answers in it (`docs/plans/i18n.md`, Q4, Q16).
        Query::Handbook(None) => Ok(handbook::contents_in(handbook::owner_language())),
        Query::Handbook(Some(name)) => handbook::page_in(&name, handbook::owner_language())
            .map(|page| page.text.to_owned())
            .ok_or_else(|| {
                let known: Vec<&str> = handbook::PAGES.iter().map(|page| page.name).collect();
                format!(
                    "the handbook has no page {name:?}; its pages: {}",
                    known.join(", ")
                )
            }),
        Query::View(request) => {
            // A connection of the call's own, opened read-only: the app's shared one is not reachable from here, and
            // a read must not migrate or write anything (security audit).
            let conn = rusqlite::Connection::open_with_flags(
                paths::db_path(),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|err| format!("the app's database cannot be read: {err}"))?;
            app_state::read_text(&Mutex::new(conn), config, request).map_err(|err| err.to_string())
        }
    }
}

impl Tool for AppTool {
    fn spec(&self) -> ToolSpec {
        let mut topics = vec!["handbook"];
        topics.extend(topic_names());
        ToolSpec {
            name: APP_TOOL.to_owned(),
            description: "Axiomata's handbook and its live state, read-only. topic=handbook without a name lists the \
                          pages, with a name gives that page. The other topics read the app now: studio (projects, \
                          sessions, who waits), spend, runs, routines, board (with board=<id> its cards), storage, \
                          settings (keys only as set/not set), tags. Answer questions about the app from these only."
                .to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": { "type": "string", "enum": topics },
                    "name": { "type": "string", "description": "handbook: the page's name, e.g. studio" },
                    "board": { "type": "integer", "description": "board: a board's id, to list its cards" },
                    "limit": { "type": "integer", "description": "runs: how many of the newest (at most 100)" }
                },
                "required": ["topic"]
            }),
        }
    }

    fn access(&self, arguments: &Value) -> Access {
        // A read, judged as one (security audit): allowed in every mode, plan mode included, while the owner's own
        // `~/.glut` rules still speak — a deny of `axiomata_app` or of these reads turns it off. The path names no
        // file; it says what is read.
        let topic = arguments["topic"].as_str().unwrap_or("?");
        Access::Read {
            path: format!("axiomata-app/{topic}"),
        }
    }

    fn call(&self, arguments: Value, _context: CallContext) -> ToolFuture<'_> {
        let config = self.config.clone();
        Box::pin(async move {
            let query = Query::of(&arguments)?;
            // The database and the storage walk block: off the run's thread.
            tokio::task::spawn_blocking(move || answer(&config, query))
                .await
                .map_err(|err| format!("the read failed: {err}"))?
        })
    }

    fn replay_safe(&self, _arguments: &Value) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_is_read_from_its_arguments() {
        assert_eq!(
            Query::of(&json!({"topic": "handbook"})),
            Ok(Query::Handbook(None))
        );
        assert_eq!(
            Query::of(&json!({"topic": "handbook", "name": "studio"})),
            Ok(Query::Handbook(Some("studio".into())))
        );
        assert_eq!(
            Query::of(&json!({"topic": "board", "board": 3})),
            Ok(Query::View(Request {
                board: Some(3),
                ..Request::new(Topic::Board)
            }))
        );
        assert!(
            Query::of(&json!({"topic": "secrets"}))
                .unwrap_err()
                .contains("unknown topic")
        );
    }

    #[test]
    fn the_handbook_is_answered_from_the_build() {
        let config = Config::default();
        // In the owner's language, whatever this machine's is; the lock keeps another test's AXIOMATA_HOME out.
        let _guard = crate::test_support::ENV_MUTEX
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let language = handbook::owner_language();
        assert_eq!(
            answer(&config, Query::Handbook(None)).unwrap(),
            handbook::contents_in(language)
        );
        assert_eq!(
            answer(&config, Query::Handbook(Some("kanban".into()))).unwrap(),
            handbook::page_in("kanban", language).unwrap().text
        );
        assert!(answer(&config, Query::Handbook(Some("../config.toml".into()))).is_err());
    }

    #[test]
    fn every_view_reads_through_a_read_only_connection() {
        use crate::test_support::{ENV_MUTEX, unique_temp_dir};
        let _guard = ENV_MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let home = unique_temp_dir("axiomata-test-app-tools");
        std::fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe { std::env::set_var(paths::AXIOMATA_HOME_ENV, &home) };
        let config = Config {
            workspace_root: home.join("vault"),
            ..Config::default()
        };
        std::fs::create_dir_all(&config.workspace_root).unwrap();
        let config_text = toml::to_string(&config).unwrap();
        std::fs::write(paths::config_path(), config_text).unwrap();
        crate::AxiomataCore::init().unwrap();
        for topic in Topic::ALL {
            let text = answer(&config, Query::View(Request::new(topic)));
            assert!(
                text.as_deref()
                    .is_ok_and(|text| text.starts_with(app_state::DATA_NOTE)),
                "{topic:?}: {text:?}"
            );
        }
        assert!(
            !config.workspace_root.join("TAGS.md").exists(),
            "a read writes nothing into the vault"
        );
        unsafe { std::env::remove_var(paths::AXIOMATA_HOME_ENV) };
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_action_tool_calls_only_the_chats_shell_actions() {
        assert_eq!(
            action_call(&json!({"action": "open_view", "params": {"view": "kanban"}})),
            Ok(("open_view".into(), json!({"view": "kanban"})))
        );
        assert_eq!(
            action_call(&json!({"action": "set_display"})).unwrap().1,
            json!({})
        );
        for refused in ["kanban_move_card", "delete", "run", ""] {
            assert!(
                action_call(&json!({"action": refused})).is_err(),
                "{refused}"
            );
        }
        assert!(action_call(&json!({"action": "open_view", "params": "x"})).is_err());
        assert_eq!(ActionTool.access(&json!({})), Access::Other);
    }

    #[test]
    fn the_tool_reads_only_the_apps_own_state() {
        let tool = AppTool {
            config: Config::default(),
        };
        assert_eq!(
            tool.access(&json!({"topic": "settings"})),
            Access::Read {
                path: "axiomata-app/settings".into()
            }
        );
        assert!(tool.replay_safe(&json!({})));
        assert_eq!(tool.spec().name, APP_TOOL);
    }
}
