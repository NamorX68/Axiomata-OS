/**
 * Parses the `mail-digest` skill's JSON output and provides the pure logic
 * behind the `mail` module — the `.svelte` shell stays thin, matching
 * `core/calendar.ts`/`core/reminders.ts`'s convention.
 *
 * Unlike those two, `mail-digest` is deliberately curated, not a full inbox
 * listing: it only ever reports messages the agent judged important, plus
 * ones matching the owner's configured topics — see `TOPICS_PATH`'s doc
 * comment for how topics get to the skill without it taking a runtime
 * parameter (skills can't). Same no-live-poll shape as `calendar`/
 * `reminders` otherwise: mail data sits behind an MCP tool only an agent
 * can reach, so every refresh is a real `mail-digest` run, never a timer.
 */

import type { RunSummary } from "./backend";
import { cut } from "./markdown";
import { loadLatestSkillRun, stripCodeFence, type Invoke } from "./skillRun";
import { openStaged } from "./staging";

/** Why one message made it into the digest at all. */
export type MailReason = "important" | "topic";

/** One curated email. `id` is the mail tool's own identifier — not needed
 *  for anything today (mail has no write actions yet), kept because every
 *  other digest item in this app carries one and a future write action
 *  will need it. */
export interface MailItem {
  id: string;
  sender: string;
  subject: string;
  /** ISO 8601 timestamp of when the message was received. */
  date: string;
  reason: MailReason;
  /** The configured topic string that matched, when `reason` is `"topic"`;
   *  `null` when `reason` is `"important"`. */
  topic: string | null;
  /** A short, agent-written summary of the message's actual content. */
  summary: string;
}

/** The skill's whole JSON payload. */
export interface MailDigest {
  emails: MailItem[];
}

/** Name of the skill the `mail` module reads its data from. */
export const MAIL_SKILL_NAME = "mail-digest";

/** An empty digest — the module's state before any run has ever happened. */
export const EMPTY_MAIL_DIGEST: MailDigest = { emails: [] };

/** One bucket in the tile's "today's mix" segmented bar. */
export interface MailMixSegment {
  label: string;
  count: number;
  color: string;
}

/**
 * Buckets a digest into "Important" (if any) plus one bucket per topic that
 * actually matched something, for the tile's segmented mix bar — Important
 * first, then topics by descending count. Purely a display aggregation;
 * doesn't change which emails are shown in the itemised lists below it.
 */
export function mailMix(digest: MailDigest): MailMixSegment[] {
  const importantCount = digest.emails.filter((e) => e.reason === "important").length;
  const byTopic = new Map<string, number>();
  for (const e of digest.emails) {
    if (e.reason === "topic" && e.topic) byTopic.set(e.topic, (byTopic.get(e.topic) ?? 0) + 1);
  }
  const topicSegments = [...byTopic.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([topic, count]) => ({ label: topic, count, color: topicColor(topic) }));
  return importantCount > 0
    ? [{ label: "Important", count: importantCount, color: "var(--ax-accent)" }, ...topicSegments]
    : topicSegments;
}

/** Stable colour for a topic name — a cheap string hash into a fixed hue,
 *  tuned to read reasonably on both light and dark themes. Not the Second
 *  Brain graph's area-colour mechanism (that's tied to its own
 *  palette/theme plumbing); this is a much smaller, standalone need. */
function topicColor(topic: string): string {
  let hash = 0;
  for (let i = 0; i < topic.length; i++) hash = (hash * 31 + topic.charCodeAt(i)) >>> 0;
  return `hsl(${hash % 360} 55% 55%)`;
}

/**
 * Where the owner's configured topics live — a plain workspace file, one
 * topic per line, that `mail-digest`'s SOP reads directly (its cwd is the
 * workspace root, so this needs no MCP tool, just the agent's ordinary file
 * access). This is how topics stay configurable without the skill taking a
 * runtime parameter: `run_skill` only ever takes a bare name, so nothing
 * can thread a dynamic value into a `SKILL.md`'s SOP directly — the skill
 * instead reads *ambient state* it already has filesystem access to, the
 * same trick `axiomata_core::notes`/`importer` already lean on for other
 * agent-read-not-just-write file access. This keeps `mail-digest` a real,
 * fixed-SOP skill — schedulable by a Routine later exactly like
 * `calendar-digest`/`reminders-digest` — rather than a one-shot instruct
 * turn built in code, which a Routine has no way to fire.
 *
 * The leading `.` keeps it out of the Second Brain graph: the memory
 * walker already skips hidden files (`memory/walker.rs`'s
 * `WalkBuilder::new(root).hidden(true)`) — only the real per-email summary
 * notes `openMailSummary` writes under `Mail/` show up as notes.
 */
export const TOPICS_PATH = "Mail/.topics.md";

const REASONS: readonly MailReason[] = ["important", "topic"];

/**
 * Parses a `mail-digest` run's captured stdout into a `MailDigest`. Same
 * defensive shape as `parseCalendarDigest`/`parseReminderDigest`: strips a
 * stray ` ```json ` fence, surfaces the skill's own `{"error": "..."}`
 * text, and drops malformed entries instead of failing the whole digest
 * over one bad message.
 */
export function parseMailDigest(stdout: string): MailDigest {
  const stripped = stripCodeFence(stdout);
  let raw: unknown;
  try {
    raw = JSON.parse(stripped);
  } catch (err) {
    throw new Error(`mail-digest output was not valid JSON: ${(err as Error).message}`);
  }
  if (!raw || typeof raw !== "object") {
    throw new Error("mail-digest output was not a JSON object");
  }
  const obj = raw as Record<string, unknown>;
  if (typeof obj.error === "string") {
    throw new Error(obj.error);
  }
  const emails = Array.isArray(obj.emails) ? obj.emails.filter(isMailItem) : [];
  return { emails };
}

function isMailItem(v: unknown): v is MailItem {
  if (!v || typeof v !== "object") return false;
  const o = v as Record<string, unknown>;
  return (
    typeof o.id === "string" &&
    typeof o.sender === "string" &&
    typeof o.subject === "string" &&
    typeof o.date === "string" &&
    typeof o.reason === "string" &&
    REASONS.includes(o.reason as MailReason) &&
    (o.topic === null || typeof o.topic === "string") &&
    typeof o.summary === "string"
  );
}

export interface LatestMailDigest {
  /** The run this digest came from, or `null` if `mail-digest` has never
   *  run at all. */
  run: RunSummary | null;
  digest: MailDigest;
  /** The failed run's own error, or a parse failure's message; `null` on a
   *  clean success (including the "never run yet" case). */
  error: string | null;
}

/**
 * Finds the most recent run of `skillName` (default `mail-digest`, or this
 * instance's `config.skillName` override — see `resolveSkillName`) —
 * however it was triggered (Skills Deck, a scheduled Routine, or the
 * tile's own refresh) — and parses its output. Same shared "find the
 * latest run" round trip as `calendar`/`reminders` (`core/skillRun.ts`),
 * just parsed with this module's own contract.
 */
export async function loadLatestMailDigest(invoke: Invoke, skillName: string = MAIL_SKILL_NAME): Promise<LatestMailDigest> {
  const { run, stdout, error } = await loadLatestSkillRun(invoke, skillName);
  if (error || stdout === null) return { run, digest: EMPTY_MAIL_DIGEST, error };
  try {
    return { run, digest: parseMailDigest(stdout), error: null };
  } catch (err) {
    return { run, digest: EMPTY_MAIL_DIGEST, error: err instanceof Error ? err.message : String(err) };
  }
}

/**
 * Reads the owner's configured topics, one per non-empty line. A missing
 * `TOPICS_PATH` (nobody has ever saved any topics) is not an error — it
 * just means no topics are configured yet, the same "important only" state
 * `mail-digest`'s SOP already treats a missing file as.
 */
export async function loadTopics(invoke: Invoke): Promise<string[]> {
  try {
    const file = await invoke<{ content: string }>("read_workspace_file", { rel: TOPICS_PATH });
    return file.content
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line.length > 0 && !line.startsWith("#"));
  } catch {
    return [];
  }
}

/**
 * Saves the owner's configured topics, one per line, with a short header
 * comment for anyone who finds the file browsing the workspace directly —
 * `mail-digest`'s SOP treats a leading `#` line as a comment, not a topic
 * (see `loadTopics`'s filter).
 */
export async function saveTopics(invoke: Invoke, topics: string[]): Promise<void> {
  const lines = [
    "# One topic per line. Read by the mail-digest skill on its next run —",
    "# not applied retroactively to anything already fetched. Edit freely.",
    ...topics.map((t) => t.trim()).filter((t) => t.length > 0),
  ];
  await invoke("write_workspace_file", { rel: TOPICS_PATH, content: `${lines.join("\n")}\n` });
}

/** Word-boundary truncation for the tile's one-line summary preview — the
 *  full `summary` only shows after `openMailSummary` opens it in the file
 *  viewer. Reuses `core/markdown.ts`'s `cut` (an architecture review flagged
 *  the two as duplicate logic that should be extracted, same as
 *  `skillRun.ts`'s "second use" convention). */
export function summaryPreview(text: string, max = 90): string {
  return cut(text.trim(), max);
}

/** German umlauts/ß transliterated before the ASCII-only collapse below —
 *  otherwise "Rückmeldung" turns into the much less readable "r-ckmeldung"
 *  instead of "rueckmeldung". Subjects in this app are routinely German. */
const GERMAN_TRANSLITERATIONS: readonly [RegExp, string][] = [
  [/ä/g, "ae"],
  [/ö/g, "oe"],
  [/ü/g, "ue"],
  [/ß/g, "ss"],
];

/** A filesystem-safe slug from a subject line: lowercased, non-alphanumeric
 *  runs collapsed to one `-`, trimmed, capped so the whole filename stays
 *  reasonable even for a very long subject. */
function slugify(text: string, max = 60): string {
  let normalised = text.toLowerCase();
  for (const [pattern, replacement] of GERMAN_TRANSLITERATIONS) {
    normalised = normalised.replace(pattern, replacement);
  }
  const slug = normalised.replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  return (slug || "untitled").slice(0, max);
}

/**
 * Cheap, dependency-free FNV-1a hash, hex-encoded. Used only to spread a
 * mail id's entropy evenly across a short disambiguating suffix (see
 * `mailNotePath`) — not for anything security-sensitive, so a non-crypto
 * hash is the right tool.
 */
function fnv1aHex(text: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

/** Deterministic note path for one email — re-opening the same email's
 *  summary overwrites its own note rather than accumulating duplicates.
 *  The date prefix keeps notes sorted chronologically in a file browser;
 *  the id suffix disambiguates two same-day emails with the same subject.
 *  Hashed rather than a prefix-slice of the raw id: an architecture review
 *  found real mail-tool ids are often structured (`account::mailbox::...`)
 *  with shared prefixes once separators are stripped, which would make a
 *  prefix-slice collide across an entire account/mailbox — a hash spreads
 *  entropy from the whole id instead of depending on where in it the
 *  distinguishing bits happen to live. Collisions are still only a
 *  cosmetic annoyance (two notes merge into one), never a data-loss risk. */
export function mailNotePath(item: MailItem): string {
  const day = (Number.isNaN(Date.parse(item.date)) ? new Date() : new Date(item.date)).toISOString().slice(0, 10);
  return `Mail/${day}-${slugify(item.subject)}-${fnv1aHex(item.id).slice(0, 8)}.md`;
}

/** Collapses embedded newlines to spaces so a hostile subject/sender/topic
 *  — real email header text, reachable by anyone who emails the owner —
 *  can't inject extra Markdown block structure (a fake heading, a spoofed
 *  "**From:**" line) into the note body. Inline formatting (bold, a link)
 *  can still come through; that's fine, DOMPurify's allow-list already
 *  bounds what those can render as. Flagged by a security review. */
function singleLine(text: string): string {
  return text.replace(/[\r\n]+/g, " ").trim();
}

/** Writes `item`'s full summary as a workspace note and opens it in the
 *  file viewer as a slide-in panel — the tile itself only ever shows
 *  `summaryPreview`. */
export async function openMailSummary(invoke: Invoke, item: MailItem): Promise<void> {
  const reason = item.reason === "topic" && item.topic ? `Topic: ${singleLine(item.topic)}` : "Important";
  const body = [
    `# ${singleLine(item.subject)}`,
    "",
    `**From:** ${singleLine(item.sender)}`,
    `**Date:** ${item.date}`,
    `**Reason:** ${reason}`,
    "",
    item.summary,
    "",
  ].join("\n");
  const path = mailNotePath(item);
  await invoke("write_workspace_file", { rel: path, content: body });
  openStaged("md-file", { path, mode: "read" }, "right");
}
