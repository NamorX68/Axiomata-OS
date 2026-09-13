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
 * control byte instead of literal text. `null` means "not one of these",
 * i.e. let the caller's `input`-event path handle it instead (printable
 * characters, IME composition, paste — see `terminal.svelte`'s own
 * `handleInput`).
 */
export function keyToBytes(key: string, ctrlKey: boolean): Uint8Array | null {
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
      return new Uint8Array([0x09]);
    case "Escape":
      // A single C0 byte, unlike arrow keys/etc. (which need a full CSI
      // sequence, e.g. `\x1b[A` — still not handled here) — cheap to
      // support and critical for vim's own mode switching (found live:
      // neovim opened and worked, but Escape did nothing without this).
      return new Uint8Array([0x1b]);
    default:
      return null;
  }
}
