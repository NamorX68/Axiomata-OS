//! The Settings dialog's Anthropic sign-in for the app's own Claude Code (`docs/plans/harness-choice.md`, D7).
//!
//! Four commands, none of which takes a program, a flag or a path: the webview can ask for the state, start a
//! sign-in, hand a running sign-in the code its page shows, and sign out. The command lines live in
//! `axiomata_core::agents::claude_auth`.

use axiomata_core::agents::claude_auth::{self, CodeInput, LoginStatus, SignIn};
use tauri::State;
use tokio::sync::Mutex;

/// The input of the sign-in that is running, if one is: a code goes there.
#[derive(Default)]
pub struct ClaudeSignIn(Mutex<Option<CodeInput>>);

/// Whether the app's Claude Code is signed in.
#[tauri::command]
pub async fn claude_login_status() -> Result<LoginStatus, String> {
    claude_auth::status().await.map_err(|err| err.to_string())
}

/// Starts a sign-in — Claude Code opens the browser — and answers once it ended (or gave up after five minutes) with
/// the state it left. A second sign-in while one runs is refused.
#[tauri::command]
pub async fn claude_sign_in(state: State<'_, ClaudeSignIn>) -> Result<LoginStatus, String> {
    let sign_in = {
        let mut running = state.0.lock().await;
        if running.is_some() {
            return Err("a sign-in is already running".to_owned());
        }
        let (sign_in, input) = SignIn::start().map_err(|err| err.to_string())?;
        *running = Some(input);
        sign_in
    };
    let finished = sign_in.finish().await;
    *state.0.lock().await = None;
    finished.map_err(|err| err.to_string())
}

/// Hands the running sign-in the code its page shows (when the browser could not hand it back by itself).
#[tauri::command]
pub async fn claude_sign_in_code(
    state: State<'_, ClaudeSignIn>,
    code: String,
) -> Result<(), String> {
    let mut running = state.0.lock().await;
    let input = running
        .as_mut()
        .ok_or_else(|| "no sign-in is running".to_owned())?;
    input.send_code(&code).await.map_err(|err| err.to_string())
}

/// Signs the app's Claude Code out — refused while a sign-in runs, whose end would race the sign-out's.
#[tauri::command]
pub async fn claude_sign_out(state: State<'_, ClaudeSignIn>) -> Result<LoginStatus, String> {
    if state.0.lock().await.is_some() {
        return Err("a sign-in is running; finish or wait for it first".to_owned());
    }
    claude_auth::sign_out().await.map_err(|err| err.to_string())
}
