/**
 * Pure parsing for Checkpoint 5b's "eigene Umgebungsvariablen" setting — a
 * plain `KEY=value`-per-line textarea (see `terminal-settings.svelte`)
 * rather than a structured key/value table, since the effort of a real
 * table isn't justified for a single Terminal module's settings page.
 * Pulled out into its own file, same as `terminalInput.ts`/
 * `terminalScrollback.ts`, so the parsing rules get real unit test coverage
 * instead of only being reachable through a live textarea.
 */

/**
 * Parses a `KEY=value`-per-line textarea into an ordered list of
 * `[key, value]` pairs, in the order the lines appeared — `PtySession::spawn`
 * (the Rust side) applies them in that same order, after `TERM`.
 *
 * Blank lines and lines with no `=` (or an empty key) are silently skipped
 * rather than rejected outright: a stray blank line at the end of a
 * textarea is normal, not a mistake worth surfacing as an error. Only the
 * *first* `=` on a line splits key from value, so a value that itself
 * contains `=` (e.g. `FOO=a=b`) is kept intact rather than truncated at the
 * first one. Each line's own leading/trailing whitespace is stripped before
 * splitting (so a stray indent doesn't become part of the value), and the
 * key side is trimmed again after the split — but whitespace immediately
 * after the `=` is kept as part of the value verbatim.
 */
export function parseEnvLines(text: string): [string, string][] {
  const pairs: [string, string][] = [];
  for (const rawLine of text.split("\n")) {
    const line = rawLine.trim();
    if (!line) continue;
    const eq = line.indexOf("=");
    if (eq <= 0) continue; // no "=" at all, or an empty key ("=value")
    const key = line.slice(0, eq).trim();
    const value = line.slice(eq + 1);
    if (!key) continue;
    pairs.push([key, value]);
  }
  return pairs;
}

/**
 * Layers one set of `KEY=value` pairs over another, later wins.
 *
 * The two layers are the global terminal setting and whatever the host of a
 * single instance supplies — an IDE agent pane hands over its profile's env
 * (M7.2 CP4). A duplicate key keeps its **original position** while taking the
 * new value: `PtySession::spawn` applies pairs in order, so moving a key would
 * quietly change which of two overlapping definitions ends up in the process.
 */
export function mergeEnv(base: [string, string][], over: [string, string][]): [string, string][] {
  const merged: [string, string][] = base.map((pair) => [...pair] as [string, string]);
  for (const [key, value] of over) {
    const existing = merged.findIndex(([k]) => k === key);
    if (existing === -1) merged.push([key, value]);
    else merged[existing] = [key, value];
  }
  return merged;
}
