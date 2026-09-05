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
 * Finds the most recent `mail-digest` run — however it was triggered
 * (Skills Deck, a scheduled Routine, or the tile's own refresh) — and
 * parses its output. Same shared "find the latest run" round trip as
 * `calendar`/`reminders` (`core/skillRun.ts`), just parsed with this
 * module's own contract.
 */
export async function loadLatestMailDigest(invoke: Invoke): Promise<LatestMailDigest> {
  const { run, stdout, error } = await loadLatestSkillRun(invoke, MAIL_SKILL_NAME);
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
 *  viewer. Same shape as `core/markdown.ts`'s private `cut`, duplicated
 *  rather than exported for this one extra caller. */
export function summaryPreview(text: string, max = 90): string {
  const trimmed = text.trim();
  if (trimmed.length <= max) return trimmed;
  const slice = trimmed.slice(0, max);
  const at = slice.lastIndexOf(" ");
  return `${slice.slice(0, at > max * 0.6 ? at : max).trimEnd()}…`;
}

/** A filesystem-safe slug from a subject line: lowercased, non-alphanumeric
 *  runs collapsed to one `-`, trimmed, capped so the whole filename stays
 *  reasonable even for a very long subject. */
/** German umlauts/ß transliterated before the ASCII-only collapse below —
 *  otherwise "Rückmeldung" turns into the much less readable "r-ckmeldung"
 *  instead of "rueckmeldung". Subjects in this app are routinely German. */
const GERMAN_TRANSLITERATIONS: readonly [RegExp, string][] = [
  [/ä/g, "ae"],
  [/ö/g, "oe"],
  [/ü/g, "ue"],
  [/ß/g, "ss"],
];

function slugify(text: string, max = 60): string {
  let normalised = text.toLowerCase();
  for (const [pattern, replacement] of GERMAN_TRANSLITERATIONS) {
    normalised = normalised.replace(pattern, replacement);
  }
  const slug = normalised.replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  return (slug || "untitled").slice(0, max);
}

/** Deterministic note path for one email — re-opening the same email's
 *  summary overwrites its own note rather than accumulating duplicates.
 *  The date prefix keeps notes sorted chronologically in a file browser;
 *  the id suffix (first 8 chars, cheaply unique enough here — collisions
 *  would only merge two different senders' same-day, same-subject emails'
 *  notes, a cosmetic annoyance, not a data-loss risk) disambiguates two
 *  same-day emails with the same subject. */
export function mailNotePath(item: MailItem): string {
  const day = (Date.parse(item.date) ? new Date(item.date) : new Date()).toISOString().slice(0, 10);
  const idSuffix = item.id.replace(/[^a-zA-Z0-9]/g, "").slice(0, 8) || "0";
  return `Mail/${day}-${slugify(item.subject)}-${idSuffix}.md`;
}

/** Writes `item`'s full summary as a workspace note and opens it in the
 *  file viewer as a slide-in panel — the tile itself only ever shows
 *  `summaryPreview`. */
export async function openMailSummary(invoke: Invoke, item: MailItem): Promise<void> {
  const reason = item.reason === "topic" && item.topic ? `Thema: ${item.topic}` : "Wichtig";
  const body = [
    `# ${item.subject}`,
    "",
    `**Von:** ${item.sender}`,
    `**Datum:** ${item.date}`,
    `**Grund:** ${reason}`,
    "",
    item.summary,
    "",
  ].join("\n");
  const path = mailNotePath(item);
  await invoke("write_workspace_file", { rel: path, content: body });
  openStaged("md-file", { path, mode: "read" }, "right");
}
