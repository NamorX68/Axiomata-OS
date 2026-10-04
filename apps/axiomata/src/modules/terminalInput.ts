/**
 * Pure keyboard-to-byte mapping for the terminal module (Checkpoint 3
 * review follow-up). Split out of `terminal.svelte`'s `handleKeydown` for
 * the same reason `TerminalScreen.ts` already pulled colour resolution out
 * of `draw()`: this is real, non-trivial domain logic (the Ctrl+letter
 * arithmetic, the fixed C0 control-byte mappings) that was previously
 * trapped inside a Svelte event handler taking a live `KeyboardEvent`,
 * with no way to unit-test it directly.
 */

/**
 * Maps one key press to the raw byte(s) to forward to the shell — the keys
 * that either produce no native `input` event or need a specific C0
 * control byte or CSI escape sequence instead of literal text (Checkpoint
 * 5i added the latter — see the `ArrowUp` case below for the full story).
 * `null` means "not one of these", i.e. let the caller's `input`-event path
 * handle it instead (printable characters, IME composition, paste — see
 * `terminal.svelte`'s own `handleInput`).
 *
 * `shiftKey` defaults to `false` so every pre-existing call site (this
 * file's own tests included) keeps compiling unchanged — only `Tab` reads
 * it at all (see that case below).
 */
export function keyToBytes(key: string, ctrlKey: boolean, shiftKey: boolean = false): Uint8Array | null {
  if (ctrlKey && key.length === 1) {
    const code = key.toUpperCase().charCodeAt(0);
    if (code >= 65 && code <= 90) {
      return new Uint8Array([code - 64]); // Ctrl+A..Z -> 0x01..0x1a
    }
  }
  switch (key) {
    case "Enter":
      return new Uint8Array([0x0d]);
    case "Backspace":
      return new Uint8Array([0x7f]);
    case "Tab":
      // Owner-reported: Shift+Tab (Claude Code's own mode-cycling shortcut —
      // plan mode / auto-accept-edits / etc.) did nothing in this terminal.
      // Root cause: plain Tab's fixed 0x09 byte was returned unconditionally,
      // `shiftKey` never even looked at. `CSI Z` (Cursor Backward Tabulation)
      // is the standard xterm sequence a `shiftKey`-modified Tab produces —
      // not "reverse-tab" in the literal terminal sense here, but the de
      // facto convention readline/zle and most full-screen TUIs (Claude
      // Code's own CLI included) already listen for specifically to detect
      // Shift+Tab, since a browser's `KeyboardEvent` has no C0 byte of its
      // own for a *shifted* Tab the way it does for plain Tab.
      return shiftKey ? new Uint8Array([0x1b, 0x5b, 0x5a]) : new Uint8Array([0x09]); // \x1b[Z or \t
    case "Escape":
      // A single C0 byte, unlike arrow keys/etc. below (a full CSI
      // sequence, e.g. `\x1b[A`) — cheap to support and critical for vim's
      // own mode switching (found live: neovim opened and worked, but
      // Escape did nothing without this).
      return new Uint8Array([0x1b]);
    // Checkpoint 5i (owner-reported: shell autosuggestion — a greyed
    // suggested completion already rendered correctly, e.g. typing `ls`
    // showing `ls -altr` with `-altr` dim, since that's just the shell's
    // own program output — but pressing → to accept it, like Ghostty, did
    // nothing at all: these nine keys fell through to the `default: null`
    // case below and were never forwarded to the PTY, at all, full stop.
    // Not autosuggestion-specific — this silently broke shell history
    // recall (↑/↓), in-line cursor movement (←/→), and Home/End/PageUp/
    // PageDown/Delete for every session, the whole time; it just hadn't
    // been noticed yet. Sequences below are the standard xterm "normal
    // cursor key mode" (DECCKM reset) encoding — what a shell's own line
    // editor (zsh's zle, GNU readline) always expects regardless of any
    // program-requested mode switch, and confirmed (architecture review)
    // against `infocmp xterm-256color` / xterm's own `ctlseqs.txt` — not
    // the rxvt/VT220-style `\x1b[1~`/`\x1b[4~` convention some other
    // terminals use for Home/End, which would have been the wrong choice
    // given the PTY is always spawned with `TERM=xterm-256color`
    // (`crates/axiomata-terminal/src/pty.rs`). Known limitation, not
    // addressed here: a full-screen program that has switched into DECCKM
    // "application cursor key" mode (`\x1b[?1h` — vim and other TUIs
    // commonly do) technically expects `\x1bO`-prefixed sequences instead
    // of `\x1b[`-prefixed ones for the *cursor-key group* specifically —
    // per xterm's own terminfo, that's the four arrows AND Home/End
    // (`kcuu1`/`khome`/etc. all have an `\x1bO...` app-mode counterpart);
    // PageUp/PageDown/Delete are NOT part of that group and always use
    // the same `\x1b[…~` form regardless of mode, so only the six
    // cursor-key entries carry this caveat. This engine doesn't track
    // DECCKM at all yet (`crates/axiomata-terminal`'s `set_private_mode`
    // silently no-ops on mode 1), so those six always use normal-mode
    // encoding even inside such a program. In practice this matches what
    // most terminfo databases already accept for arrows specifically
    // (vim in particular is documented to recognize both forms
    // defensively), so this is expected to work for ordinary vim/htop/
    // less use — if a specific full-screen program is ever found where
    // arrows or Home/End misbehave, that's the concrete case to finally
    // add DECCKM tracking for, not a reason to withhold this fix
    // (shell-prompt use, the overwhelming common case, needs it right now
    // and isn't affected by DECCKM at all).
    case "ArrowUp":
      return new Uint8Array([0x1b, 0x5b, 0x41]); // \x1b[A
    case "ArrowDown":
      return new Uint8Array([0x1b, 0x5b, 0x42]); // \x1b[B
    case "ArrowRight":
      return new Uint8Array([0x1b, 0x5b, 0x43]); // \x1b[C
    case "ArrowLeft":
      return new Uint8Array([0x1b, 0x5b, 0x44]); // \x1b[D
    case "Home":
      return new Uint8Array([0x1b, 0x5b, 0x48]); // \x1b[H
    case "End":
      return new Uint8Array([0x1b, 0x5b, 0x46]); // \x1b[F
    case "PageUp":
      return new Uint8Array([0x1b, 0x5b, 0x35, 0x7e]); // \x1b[5~
    case "PageDown":
      return new Uint8Array([0x1b, 0x5b, 0x36, 0x7e]); // \x1b[6~
    case "Delete":
      return new Uint8Array([0x1b, 0x5b, 0x33, 0x7e]); // \x1b[3~ (forward-delete, distinct from Backspace)
    default:
      return null;
  }
}
