//! What Axiomata needs from macOS itself, without an extra dependency.
//!
//! Today the general pasteboard for the editor's Vi mode (`docs/plans/editor.md`,
//! ED3, V3): `p` pastes what was last copied anywhere, `y` copies for every other
//! app — which the webview cannot do on its own without a paste event, and
//! `navigator.clipboard` would show a "Paste" confirmation on every read. The
//! system's own `pbcopy`/`pbpaste` do it, as subprocesses, text only.
//!
//! And the installed fonts for the editor's and the terminal's font pickers
//! (ED5, T10), straight from CoreText.

pub mod clipboard;
pub mod fonts;
