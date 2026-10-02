/** Terminal escape sequences in a program's output — a console shows text, not a screen. */

// CSI (colours, cursor moves), OSC (titles, links), and the two-byte escapes.
// eslint-disable-next-line no-control-regex
const ESCAPES = /\u001b\[[0-9;?<>=!]*[ -/]*[@-~]|\u001b\][^\u0007\u001b]*(?:\u0007|\u001b\\)|\u001b[@-Z\\-_]/g;

export function stripAnsi(text: string): string {
  return text.replace(ESCAPES, "");
}

/** Switching to the alternate screen (or hiding the cursor to paint one) is what a TUI does first. */
export function isFullScreen(text: string): boolean {
  return text.includes("\u001b[?1049h") || text.includes("\u001b[?47h");
}
