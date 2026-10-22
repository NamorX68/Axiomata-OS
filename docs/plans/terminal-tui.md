# TUI0b — the Axiomata terminal grows up

Owner, 2026-10-22: after TUI1e, "direkt mit TUI0b weiter machen". The package is defined in Glut's master plan
(`../Axiomata-Glut/docs/plans/glut-tui.md`, "TUI0 — measured"): the Studio's panes (`axiomata-terminal` + the
webview renderer) run a full-screen TUI only in part. What the audit found missing, in order of weight: no reply path,
`?25` ignored, SGR 2/3/7/9 dropped, only the wheel reported, no F-keys/Alt/kitty keys and every Enter the same, no
spacer cells for wide characters, no `?2026`/focus/OSC 8 URLs/OSC 52, and the whole grid sent every 8 ms. It helps
Claude Code's and Opencode's TUIs in the same panes as much as Glut's.

Reference behaviour: the owner's Ghostty (main) and Warp, measured with `glut-probe` (`docs/probe/` in Axiomata-Glut).

## Decisions (defaults taken; the owner may overrule)

- **D1 — answers come from the screen model.** `Screen` queues replies (DA1, DA2, XTVERSION, DECRQM, CPR, DSR, the
  kitty keyboard query, OSC 10/11 and the window size in cells); the glue writes them to the PTY right after each
  feed. OSC 10/11 answer with the colours the pane draws with (handed down from the theme), never a guess.
- **D2 — Option stays Option.** On a German layout Option makes `@ { } [ ] \ |`; it acts as Alt only where it
  composes nothing (Enter, Backspace, arrows, Tab, F-keys, and letters whose composed character equals the plain
  one). Ghostty's default is the same (`macos-option-as-alt = false`).
- **D3 — the program gets the mouse when it asks, Shift keeps it for the pane.** With `?1000/1002/1003` on, clicks,
  drags (1002) and motion (1003) go to the program in its encoding; Shift+drag still selects in the pane, as in
  Ghostty and iTerm2.
- **D4 — OSC 52 writes only, and only from the pane in front.** A program may put text on the clipboard (bounded,
  1 MB) while its pane has the focus, at most once a second, and the pane says so ("the program copied N
  characters"); a read request is never answered — the clipboard is the owner's. A pasted ESC is dropped inside a
  bracketed paste, so clipboard text cannot end the bracket and type commands (security review, 2026-10-22).
- **D5 — links open on Cmd+click.** OSC 8 keeps its URL per cell (an id into the screen's link table); Cmd+click opens
  `http(s)` URLs and `mailto:` without its query with the system, anything else is ignored; the target is taken when
  the button goes down and must still be the one when it comes up. Showing the URL on hover is left for later.
- **D6 — `?2026` holds the frame.** While a program has synchronized output on, the pane is not sent a new frame —
  for at most 150 ms, so a program that forgets to end it cannot freeze the pane.
- **D8 — nothing a program does blocks other panes.** The terminal's answers and the pane's input are written to the
  PTY after the session registry's lock is let go; an OSC longer than 2 MB is cut off and never acted on.
- **D7 — only changed rows travel.** The screen marks the rows it touched; a frame carries those rows (all of them
  after a resize, a screen switch or a scroll), the renderer keeps the rest.

## Checkpoints

- **TB1 — the emulator (crate `axiomata-terminal`).** The reply path (D1); `?25`, `?1004`, `?2026`, the mouse modes
  kept apart (`?1000/1002/1003/1006`), cursor shape (DECSCUSR); SGR dim, italic, reverse, invisible, strike, double
  and styled underlines (`4:0`–`4:5`, `21`), underline colour (`58`/`59`) and their resets; spacer cells for wide
  characters (overwriting half of one blanks both) and a combining mark kept on its cell; OSC 8 URLs (D5), OSC 52
  (D4); the kitty keyboard flags (`CSI > u`, `CSI < u`, `CSI = u`, the query) as state the input reads; damage per row
  (D7). Cells serialize lean: false flags and default colours are left out.
- **TB2 — the glue (`src-tauri/src/terminal.rs`).** Replies to the PTY; the theme colours for OSC 10/11; the frame
  held under `?2026` (D6); only damaged rows sent (D7); clipboard writes and the link table to the pane; the modes the
  input needs (mouse, focus, kitty flags, cursor visible and shape) in every frame.
- **TB3 — the renderer (`TerminalScreen.ts`).** The new attributes, a hidden cursor and its shape, wide characters
  over two cells, combining marks, rows merged from partial frames, OSC 52 to the clipboard, Cmd+click on a link.
- **TB4 — the input (`terminalInput.ts`, `terminal.svelte`).** F1–F12, Alt (D2), modifiers on arrows/Home/End/PgUp/
  PgDn/Delete, Shift/Alt/Ctrl+Enter told apart, the kitty keyboard protocol's flag 1 (disambiguate — what Glut, Claude
  Code and Opencode ask for; 2 and 8 would need key releases and every key as a code, left for when a program needs
  them, and the query says only 1 is in force) when the program turns it on; mouse clicks, drags and motion reported
  (D3); focus in/out under `?1004`.
- **TB5 — measured.** `glut-probe` in a Studio pane next to the Ghostty report, then `glut`, Claude Code and Opencode in
  panes; the owner's test.

## Status

- TB1–TB4 built 2026-10-22 (crate 109 tests, glue 16, frontend: `terminalInput`, `terminalMouse`, `TerminalScreen`,
  `terminalWheel` tests), after a security review (D4's focus and rate limit, the paste and OSC bounds, D8) and a
  code review (wide pairs mended after erases and resizes, URLs with `;`, `?1000l` ends any tracking, ⌘-click on links
  in panes that have the mouse). Open: TB5, measured in a Studio pane by the owner; the URL shown on hover.
