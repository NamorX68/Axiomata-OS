/**
 * The tags of a note's leading frontmatter (`tags:` or `tag:`), the way the Rust scan reads them
 * (`crates/axiomata-core/src/tags/detect.rs`, `tags_key`): a block list of `- item` lines, a one-line flow list
 * `[a, b]`, or comma/space-separated text; quotes and a leading `#` dropped, lower case, only what is a tag. The
 * Markdown reading view shows them as chips at the top (owner, 2026-10-29, concept note Q6), since the frontmatter
 * itself is not drawn there.
 */

import { leadingFrontmatter } from "./markdown";
import { normaliseTag, tagProblem } from "./tagProposals";

/** The most tags read from one note. */
export const MAX_CHIPS = 500;

/** The frontmatter's tags, normalised, each once, in the order written; `[]` without a frontmatter or a `tags:` key. */
export function frontmatterTags(source: string): string[] {
  const front = leadingFrontmatter(source);
  if (!front) return [];
  // Without the opening fence, and without the closing one (the block's last line): a `---` would read as a list item.
  const body = front.replace(/\r?\n$/, "").split(/\r?\n/).slice(1, -1);
  const at = body.findIndex((line) => /^tags?[ \t]*:/i.test(line));
  if (at < 0) return [];
  const value = body[at].slice(body[at].indexOf(":") + 1).trim();
  const raw: string[] = [];
  if (value === "") {
    for (const line of body.slice(at + 1)) {
      const item = line.trim();
      if (item === "") continue;
      // A block list's items are indented (or start the line with `-`); anything else is the next key.
      if (!item.startsWith("-") || !(/^\s/.test(line) || line.startsWith("-"))) break;
      raw.push(item.slice(1));
    }
  } else {
    const inner = value.startsWith("[") ? value.slice(1).replace(/\][ \t]*$/, "") : value;
    raw.push(...inner.split(/[,\s]+/));
  }
  const out: string[] = [];
  for (const item of raw) {
    // A note from anywhere could list thousands: the chips stop where the vault's list does (`list::MAX_TAGS`).
    if (out.length >= MAX_CHIPS) break;
    const text = item.trim().replace(/^["']|["']$/g, "").trim();
    if (tagProblem(text, []) !== null) continue;
    const tag = normaliseTag(text);
    if (!out.includes(tag)) out.push(tag);
  }
  return out;
}
