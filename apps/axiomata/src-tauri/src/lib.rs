use axiomata_core::routines::SchedulerHandle;
use tauri::Manager;

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
mod tasks;
mod terminal;

/// Initializes `tracing`'s output so `axiomata_core`'s `tracing::info!`/
/// `warn!` calls (the routine scheduler's tick/reconcile summaries, in
/// particular) actually go somewhere — `tracing-subscriber` was a declared
/// workspace dependency that nothing ever called `.init()` on. Defaults to
/// `info`; override with `RUST_LOG` (e.g. `RUST_LOG=debug`).
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
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
            commands::open_review_sessions,
            commands::list_card_events,
            commands::card_usage,
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
