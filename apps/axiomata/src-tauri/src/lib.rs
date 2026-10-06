use axiomata_core::routines::SchedulerHandle;
use std::path::Path;
use std::sync::OnceLock;
use tauri::Manager;
use tracing::Subscriber;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{InitError, RollingFileAppender, Rotation};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;

mod bootstrap;
mod card_watch;
mod commands;
mod debug;
mod files;
mod git;
mod limit_watch;
mod lsp;
#[cfg(target_os = "macos")]
mod menu;
mod plan_watch;
mod tasks;
mod terminal;

/// File-name prefix of the app log; `tracing-appender` appends the date (`app.log.YYYY-MM-DD`).
/// Distinct from `runs.log`, so the rotation's clean-up (it matches the prefix plus a dot) never
/// touches the run log that lives in the same directory.
const APP_LOG_PREFIX: &str = "app.log";

/// How many daily app-log files are kept; older ones are deleted by the appender on rotation.
const APP_LOG_MAX_FILES: usize = 7;

/// Keeps the file writer's background thread alive until the process ends: dropping the guard
/// stops it and loses the lines still queued.
static APP_LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

/// Builds a `fmt` layer that writes ANSI-free lines to a daily-rotating `app.log.*` file in `dir`
/// (created if missing), keeping [`APP_LOG_MAX_FILES`] files.
///
/// Returns the layer together with the guard of its non-blocking writer.
///
/// # Errors
///
/// Fails when the directory cannot be created or the log file cannot be opened.
fn app_log_layer<S>(dir: &Path) -> Result<(Box<dyn Layer<S> + Send + Sync>, WorkerGuard), InitError>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(APP_LOG_PREFIX)
        .max_log_files(APP_LOG_MAX_FILES)
        .build(dir)?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(writer)
        .boxed();
    Ok((layer, guard))
}

/// Initializes `tracing`'s output so `axiomata_core`'s `tracing::info!`/
/// `warn!` calls (the routine scheduler's tick/reconcile summaries, in
/// particular) actually go somewhere. Lines go to stderr (`cargo tauri dev`) and
/// to the daily-rotating `~/.axiomata/logs/app.log.*` — a bundled `.app` has no
/// terminal. Defaults to `info` for both; override with `RUST_LOG` (e.g.
/// `RUST_LOG=debug`).
///
/// When the log directory is unusable the app still starts, on stderr only.
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let log_dir = axiomata_core::paths::logs_dir();
    let (file_layer, file_error) = match app_log_layer(&log_dir) {
        Ok((layer, guard)) => {
            // `set` only fails if the guard was stored already; the first one stays alive.
            let _ = APP_LOG_GUARD.set(guard);
            (Some(layer), None)
        }
        Err(error) => (None, Some(error)),
    };
    tracing_subscriber::registry()
        .with(file_layer)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(filter)
        .init();
    if let Some(error) = file_error {
        tracing::warn!(dir = %log_dir.display(), %error, "app log file unavailable, logging to stderr only");
    }
}

/// Variables that mark a process as **part of a running Claude Code session**.
///
/// Launched from inside one — `cargo tauri dev` from Claude Code's shell, or
/// its `!` prefix — the app inherits them, and every agent pane passes them on.
/// A Claude Code agent then believes it is a child of that session: it saves no
/// transcript and behaves differently (live test, M7.2 CP6). The app is not a
/// Claude Code session, so nothing it starts should claim to be one.
///
/// A fixed list of session markers, not a `CLAUDE_*` prefix: variables a user
/// sets on purpose (`CLAUDE_CONFIG_DIR`, `CLAUDE_CODE_USE_BEDROCK`, …) stay.
const INHERITED_CLAUDE_SESSION_VARS: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
];

/// Removes [`INHERITED_CLAUDE_SESSION_VARS`] from this process's environment.
fn forget_inherited_claude_session() {
    for name in INHERITED_CLAUDE_SESSION_VARS {
        // SAFETY: called first thing in `run`, before tracing, Tauri or any
        // other thread exists, so nothing can read the environment while it
        // changes — the condition `remove_var`'s safety contract asks for.
        unsafe { std::env::remove_var(name) };
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    forget_inherited_claude_session();
    init_tracing();

    let app = tauri::Builder::default()
        // Our own menu goes in at setup (`menu`): the default one closes the window on ⌘W.
        .enable_macos_default_menu(false)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // Persist the window's size / position / maximized state across
        // restarts (written to `window-state.json` in the OS app-config dir,
        // restored when the window is created).
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_config,
            commands::save_config,
            commands::get_spend_summary,
            commands::get_dashboard_state,
            commands::save_dashboard_state,
            commands::get_terminal_settings,
            commands::save_terminal_settings,
            commands::get_editor_settings,
            commands::save_editor_settings,
            commands::get_editor_vi_state,
            commands::save_editor_vi_state,
            commands::list_installed_apps,
            commands::read_workspace_file,
            commands::read_workspace_image,
            commands::write_workspace_file,
            commands::delete_workspace_file,
            files::file_roots,
            files::file_read,
            files::file_write,
            files::file_delete,
            files::file_list,
            files::file_index,
            files::file_search,
            files::file_search_cancel,
            files::file_mkdir,
            files::file_rename,
            files::file_create,
            files::file_count,
            files::file_delete_tree,
            files::file_read_image,
            files::file_pick,
            files::project_open,
            git::git_status,
            git::git_stage,
            git::git_unstage,
            git::git_stage_all,
            git::git_unstage_all,
            git::git_diff,
            git::git_blob,
            git::git_apply_hunk,
            git::git_commit,
            git::git_fetch,
            git::git_push,
            git::git_init,
            git::git_discard,
            git::git_discard_hunk,
            git::git_branches,
            git::git_outgoing,
            git::git_switch,
            git::git_create_branch,
            tasks::tasks_list,
            tasks::tasks_trust,
            tasks::task_command_line,
            tasks::tasks_save,
            tasks::tasks_remove,
            debug::debug_configs,
            debug::debug_save,
            debug::debug_remove,
            debug::debug_trust,
            debug::debug_start,
            debug::debug_stop,
            debug::debug_control,
            debug::debug_stack,
            debug::debug_scopes,
            debug::debug_variables,
            debug::debug_evaluate,
            debug::debug_set_breakpoints,
            files::project_new,
            files::file_watch,
            files::file_unwatch,
            lsp::lsp_start,
            lsp::lsp_send,
            lsp::lsp_opened,
            lsp::lsp_closed,
            lsp::file_format,
            files::editor_recovery_save,
            files::editor_recovery_load,
            files::editor_recovery_delete,
            commands::create_note,
            commands::assistant_send,
            commands::write_module_manifest,
            commands::poll_module_actions,
            commands::complete_module_action,
            commands::load_custom_css,
            commands::get_workspace_graph,
            commands::search_workspace,
            commands::list_skills,
            commands::list_skipped_skills,
            commands::list_runs,
            commands::get_run,
            commands::run_skill,
            commands::sync_memory,
            commands::get_memory_status,
            commands::list_routines,
            commands::add_routine,
            commands::set_routine_enabled,
            commands::update_routine,
            commands::delete_routine,
            commands::routine_history,
            commands::list_boards,
            commands::get_board,
            commands::create_board,
            commands::rename_board,
            commands::delete_board,
            commands::count_board_cards,
            commands::list_board_columns,
            commands::list_board_cards,
            commands::create_card,
            commands::update_card,
            commands::move_card,
            commands::delete_card,
            commands::set_card_archived,
            commands::list_board_plans,
            commands::create_board_plan,
            commands::update_board_plan,
            commands::approve_board_plan,
            commands::close_board_plan,
            commands::delete_board_plan,
            commands::list_board_dependencies,
            commands::add_card_dependency,
            commands::remove_card_dependency,
            commands::start_card_session,
            commands::release_card,
            commands::start_review_session,
            commands::take_over_card,
            commands::take_over_plan,
            commands::plan_spend,
            commands::session_activity,
            commands::integrate_card,
            commands::redo_card,
            commands::plan_cards_left_for_owner,
            commands::plan_changed_proposals,
            commands::plan_goal_suggestion,
            commands::plan_grilled,
            commands::apply_goal_suggestion,
            commands::discard_goal_suggestion,
            commands::resume_plan,
            commands::open_card_sessions,
            commands::list_card_events,
            commands::card_usage,
            commands::start_plan_session,
            commands::add_card_note,
            commands::approve_card_proposal,
            commands::mark_card,
            commands::create_board_column,
            commands::update_board_column,
            commands::delete_board_column,
            commands::move_board_column,
            commands::list_ide_projects,
            commands::rename_ide_project,
            files::project_set_root,
            commands::set_ide_project_layout,
            commands::open_ide_project,
            commands::delete_ide_project,
            commands::list_ide_agents,
            commands::create_ide_agent_on_engine,
            commands::update_ide_agent_on_engine,
            commands::list_engines,
            commands::save_engine,
            commands::delete_engine,
            commands::list_roles,
            commands::save_role,
            commands::delete_role,
            commands::project_roles,
            commands::confirm_project_roles,
            commands::delete_ide_agent,
            commands::prepare_ide_agent,
            commands::ide_agent_new_session,
            commands::ide_mailbox_nudge,
            commands::ide_mailbox_nudged,
            commands::ide_mailbox_messages,
            commands::ide_mailbox_unread,
            commands::ide_mailbox_send,
            commands::ide_agent_states,
            commands::ide_agent_changes,
            commands::ide_agent_file_diff,
            commands::ide_agent_base_file,
            commands::clipboard_read,
            commands::installed_fonts,
            commands::ui_displays,
            commands::clipboard_write,
            commands::ide_agent_discard_hunk,
            commands::ide_agent_last_subject,
            commands::ide_agent_discard,
            commands::ide_agent_commit,
            commands::ide_agent_take_over,
            commands::ide_agent_has_changes,
            commands::discard_ide_agent_worktree,
            terminal::terminal_spawn,
            terminal::terminal_write,
            terminal::terminal_resize,
            terminal::terminal_close,
            terminal::terminal_scrollback,
        ])
        .setup(|app| {
            // Core init, the startup memory sync, and the routine scheduler —
            // see `bootstrap` for why this is one call instead of `.setup()`
            // inlining all three.
            let services = bootstrap::bootstrap();
            app.manage(services.core);
            app.manage(services.scheduler);
            app.manage(terminal::TerminalSessions::default());
            app.manage(files::FileWatch::start(app.handle()));
            app.manage(files::Searches::default());
            app.manage(lsp::LspState::new());
            app.manage(debug::DebugState::default());
            card_watch::start(app.handle());
            limit_watch::start(app.handle());
            plan_watch::start(app.handle());
            #[cfg(target_os = "macos")]
            app.set_menu(menu::app_menu(app.handle())?)?;
            Ok(())
        })
        // A reload (dev HMR, a crash-reload) leaves the new page with no
        // knowledge of the old page's file subscriptions: drop them.
        .on_page_load(|webview, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Started
                && let Some(watch) = webview.try_state::<files::FileWatch>()
            {
                watch.page_reloaded();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building the tauri application");

    app.run(|app_handle, event| {
        // `Drop` alone cannot be relied on to run the scheduler's shutdown:
        // Tauri's default exit path can end the process without unwinding
        // (see the M3 review finding this fixes). Request the stop
        // explicitly here instead, as soon as an exit is requested.
        if let tauri::RunEvent::ExitRequested { .. } = event
            && let Some(scheduler) = app_handle.try_state::<SchedulerHandle>()
        {
            scheduler.shutdown();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::Registry;

    /// A fresh directory under the system temp dir; `app_log_layer` must create it itself.
    fn unique_log_dir() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("axiomata-app-log-{}-{nanos}", std::process::id()))
    }

    fn files_in(dir: &Path) -> Vec<std::path::PathBuf> {
        std::fs::read_dir(dir)
            .expect("log dir is readable")
            .map(|entry| entry.expect("dir entry").path())
            .collect()
    }

    #[test]
    fn app_log_layer_writes_a_plain_line_into_a_dated_app_log_file() {
        let dir = unique_log_dir();
        let (layer, guard) = app_log_layer::<Registry>(&dir).expect("layer builds");
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("hello from the app log test");
        });
        // Dropping the guard flushes the non-blocking writer's queue.
        drop(guard);

        let files = files_in(&dir);
        assert_eq!(files.len(), 1, "exactly one log file: {files:?}");
        let name = files[0]
            .file_name()
            .and_then(|n| n.to_str())
            .expect("utf-8 name");
        assert!(name.starts_with("app.log."), "dated name, got {name}");

        let content = std::fs::read_to_string(&files[0]).expect("log file is readable");
        assert!(
            content.contains("hello from the app log test"),
            "line is logged: {content}"
        );
        assert!(
            !content.contains('\u{1b}'),
            "no ANSI escape sequences: {content:?}"
        );

        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn app_log_layer_leaves_other_files_in_the_directory_alone() {
        let dir = unique_log_dir();
        std::fs::create_dir_all(&dir).expect("create dir");
        let runs_log = dir.join("runs.log");
        std::fs::write(&runs_log, "{}\n").expect("seed runs.log");

        let (layer, guard) = app_log_layer::<Registry>(&dir).expect("layer builds");
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || tracing::info!("x"));
        drop(guard);

        assert_eq!(
            std::fs::read_to_string(&runs_log).expect("runs.log survives"),
            "{}\n"
        );

        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn app_log_layer_fails_instead_of_panicking_when_the_directory_cannot_exist() {
        // A regular file where the directory should be: `create_dir_all` cannot succeed.
        let blocker = unique_log_dir();
        std::fs::write(&blocker, "not a directory").expect("create blocker file");

        let result = app_log_layer::<Registry>(&blocker.join("logs"));

        assert!(result.is_err());
        std::fs::remove_file(&blocker).expect("clean up");
    }
}
