import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { get } from "svelte/store";

import { registerBuiltins } from "../modules";
import type { RunRecord, RunSummary } from "./backend";
import { EMPTY_MAIL_DIGEST, loadLatestMailDigest, loadTopics, mailNotePath, openMailSummary, parseMailDigest, saveTopics, summaryPreview, TOPICS_PATH, type MailItem } from "./mail";
import { staged } from "./staging";

const DIGEST_JSON = JSON.stringify({
  emails: [
    { id: "m-1", sender: "Chef", subject: "Bitte um Rückmeldung", date: "2026-09-05T08:00:00Z", reason: "important", topic: null, summary: "Braucht bis Freitag eine Entscheidung zum Budget." },
    { id: "m-2", sender: "Foto-Newsletter", subject: "Neue Kamera-Tests", date: "2026-09-04T10:00:00Z", reason: "topic", topic: "Fotografie", summary: "Vergleich dreier Vollformatkameras, Testsieger ist die Sony A7.", },
  ],
});

describe("parseMailDigest", () => {
  it("parses a well-formed digest", () => {
    const d = parseMailDigest(DIGEST_JSON);
    expect(d.emails).toHaveLength(2);
    expect(d.emails[0].reason).toBe("important");
    expect(d.emails[1]).toEqual({
      id: "m-2",
      sender: "Foto-Newsletter",
      subject: "Neue Kamera-Tests",
      date: "2026-09-04T10:00:00Z",
      reason: "topic",
      topic: "Fotografie",
      summary: "Vergleich dreier Vollformatkameras, Testsieger ist die Sony A7.",
    });
  });

  it("strips a ```json fence the model added despite being told not to", () => {
    const fenced = "```json\n" + DIGEST_JSON + "\n```";
    expect(parseMailDigest(fenced)).toEqual(parseMailDigest(DIGEST_JSON));
  });

  it("throws the skill's own error message when it reports no mail tool", () => {
    const noTool = JSON.stringify({ emails: [], error: "no mail tool available" });
    expect(() => parseMailDigest(noTool)).toThrow("no mail tool available");
  });

  it("throws on unparseable output", () => {
    expect(() => parseMailDigest("nope")).toThrow(/not valid JSON/);
  });

  it("drops an entry with an invalid reason instead of throwing on the whole digest", () => {
    const mixed = JSON.stringify({
      emails: [
        { id: "m-1", sender: "a", subject: "ok", date: "2026-09-05T08:00:00Z", reason: "important", topic: null, summary: "s" },
        { id: "m-2", sender: "b", subject: "bad reason", date: "2026-09-05T08:00:00Z", reason: "urgent!!", topic: null, summary: "s" },
        { subject: "missing everything else" },
      ],
    });
    const d = parseMailDigest(mixed);
    expect(d.emails).toHaveLength(1);
    expect(d.emails[0].subject).toBe("ok");
  });
});

describe("summaryPreview", () => {
  it("returns short text unchanged", () => {
    expect(summaryPreview("short summary")).toBe("short summary");
  });

  it("truncates at a word boundary with an ellipsis", () => {
    const long = "word ".repeat(40).trim();
    const preview = summaryPreview(long, 20);
    expect(preview.endsWith("…")).toBe(true);
    expect(preview.length).toBeLessThanOrEqual(21);
    expect(preview).not.toContain("  ");
  });
});

describe("mailNotePath", () => {
  const item: MailItem = {
    id: "ABCD-1234-5678",
    sender: "x",
    subject: "Re: Enterprise plan inquiry — 40 seats!!",
    date: "2026-09-05T08:00:00Z",
    reason: "important",
    topic: null,
    summary: "s",
  };

  it("is deterministic for the same item", () => {
    expect(mailNotePath(item)).toBe(mailNotePath({ ...item }));
  });

  it("is filesystem-safe and lives under Mail/", () => {
    const path = mailNotePath(item);
    expect(path).toMatch(/^Mail\/2026-09-05-re-enterprise-plan-inquiry-40-seats-[a-zA-Z0-9]+\.md$/);
  });

  it("differs for two emails with different ids on the same day/subject", () => {
    const other = { ...item, id: "ZZZZ-9999" };
    expect(mailNotePath(item)).not.toBe(mailNotePath(other));
  });

  it("transliterates German umlauts instead of collapsing them to a dash", () => {
    const path = mailNotePath({ ...item, subject: "Bitte um Rückmeldung: Büro-Umzug für Käufer" });
    expect(path).toContain("rueckmeldung");
    expect(path).toContain("buero-umzug");
    expect(path).toContain("kaeufer");
    expect(path).not.toMatch(/[üöäß]/);
  });

  it("still differs for structured ids sharing a long common prefix", () => {
    // A prefix-slice of the raw id (the original implementation) would
    // collapse these to the same suffix once separators are stripped —
    // an architecture review found real mail-tool ids are commonly shaped
    // like this. The hash-based suffix must not repeat the same mistake.
    const a = { ...item, id: "account1::INBOX::<msg-0001@example.com>" };
    const b = { ...item, id: "account1::INBOX::<msg-0002@example.com>" };
    expect(mailNotePath(a)).not.toBe(mailNotePath(b));
  });
});

describe("loadTopics / saveTopics", () => {
  function fakeWorkspace(initial: Record<string, string> = {}) {
    const files = new Map(Object.entries(initial));
    const invoke = (async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
      const rel = (args as { rel: string }).rel;
      if (cmd === "read_workspace_file") {
        const content = files.get(rel);
        if (content === undefined) throw new Error(`I/O error at ${rel}: No such file or directory`);
        return { path: rel, content, modified: null } as unknown as T;
      }
      if (cmd === "write_workspace_file") {
        files.set(rel, String((args as { content: string }).content));
        return undefined as T;
      }
      throw new Error(`unexpected cmd ${cmd}`);
    }) as <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    return { invoke, files };
  }

  it("returns no topics when the file has never been saved", async () => {
    const { invoke } = fakeWorkspace();
    expect(await loadTopics(invoke)).toEqual([]);
  });

  it("round-trips a saved topic list, ignoring blank lines and the header comment", async () => {
    const { invoke, files } = fakeWorkspace();
    await saveTopics(invoke, ["Fotografie", "Development", "  KI/AI/LLM  ", ""]);
    expect(files.get(TOPICS_PATH)).toContain("Fotografie");
    expect(await loadTopics(invoke)).toEqual(["Fotografie", "Development", "KI/AI/LLM"]);
  });

  it("treats a hand-edited comment line as not a topic", async () => {
    const { invoke } = fakeWorkspace({ [TOPICS_PATH]: "# my topics\nFotografie\n\n# another comment\nDevelopment\n" });
    expect(await loadTopics(invoke)).toEqual(["Fotografie", "Development"]);
  });
});

describe("loadLatestMailDigest", () => {
  const summary = (over: Partial<RunSummary>): RunSummary => ({
    id: 1,
    skill_name: "mail-digest",
    backend: "claude-code",
    status: "success",
    exit_code: 0,
    duration_ms: 100,
    error: null,
    started_at: "2026-09-05T09:00:00Z",
    source: "manual",
    ...over,
  });

  function fakeInvoke(runs: RunSummary[], records: Record<number, RunRecord>) {
    return (async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
      if (cmd === "list_runs") return runs as unknown as T;
      if (cmd === "get_run") return (records[(args as { id: number }).id] ?? null) as unknown as T;
      throw new Error(`unexpected cmd ${cmd}`);
    }) as <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  }

  it("returns an empty digest with no error when the skill has never run", async () => {
    const result = await loadLatestMailDigest(fakeInvoke([], {}));
    expect(result).toEqual({ run: null, digest: EMPTY_MAIL_DIGEST, error: null });
  });

  it("parses the latest successful run", async () => {
    const records = { 1: { ...summary({}), stdout: DIGEST_JSON, stderr: "", finished_at: "" } };
    const result = await loadLatestMailDigest(fakeInvoke([summary({})], records));
    expect(result.digest.emails).toHaveLength(2);
    expect(result.error).toBeNull();
  });

  it("surfaces a failed run's own error", async () => {
    const runs = [summary({ status: "failed", error: "agent timed out" })];
    const result = await loadLatestMailDigest(fakeInvoke(runs, {}));
    expect(result.error).toBe("agent timed out");
  });
});

describe("openMailSummary", () => {
  beforeAll(() => registerBuiltins());
  beforeEach(() => staged.set([]));

  it("writes the full summary as a note and opens it in the file viewer", async () => {
    const writes: Record<string, unknown>[] = [];
    const invoke = (async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
      if (cmd !== "write_workspace_file") throw new Error(`unexpected cmd ${cmd}`);
      writes.push(args ?? {});
      return undefined as T;
    }) as <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

    const item: MailItem = {
      id: "m-1",
      sender: "Chef",
      subject: "Bitte um Rückmeldung",
      date: "2026-09-05T08:00:00Z",
      reason: "important",
      topic: null,
      summary: "Braucht bis Freitag eine Entscheidung zum Budget.",
    };
    await openMailSummary(invoke, item);

    expect(writes).toHaveLength(1);
    const written = String(writes[0].content);
    expect(written).toContain("# Bitte um Rückmeldung");
    expect(written).toContain("**Von:** Chef");
    expect(written).toContain("**Grund:** Wichtig");
    expect(written).toContain(item.summary);

    const panels = get(staged);
    expect(panels).toHaveLength(1);
    expect(panels[0].type).toBe("md-file");
    expect(panels[0].config.path).toBe(writes[0].rel);
  });

  it("labels a topic match with the topic name instead of \"Wichtig\"", async () => {
    const invoke = (async <T>(): Promise<T> => undefined as T) as <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    const writes: string[] = [];
    const recording = (async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
      if (cmd === "write_workspace_file") writes.push(String((args as { content: string }).content));
      return undefined as T;
    }) as typeof invoke;
    await openMailSummary(recording, {
      id: "m-2",
      sender: "Foto-Newsletter",
      subject: "Neue Kamera-Tests",
      date: "2026-09-04T10:00:00Z",
      reason: "topic",
      topic: "Fotografie",
      summary: "s",
    });
    expect(writes[0]).toContain("**Grund:** Thema: Fotografie");
  });

  it("collapses embedded newlines in subject/sender/topic so hostile mail headers can't inject extra Markdown structure", async () => {
    const writes: string[] = [];
    const recording = (async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
      if (cmd === "write_workspace_file") writes.push(String((args as { content: string }).content));
      return undefined as T;
    }) as <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    await openMailSummary(recording, {
      id: "m-3",
      sender: "Attacker\n\n**Von:** Chef (spoofed)",
      subject: "Invoice\n# Fake urgent heading",
      date: "2026-09-05T08:00:00Z",
      reason: "topic",
      topic: "Finance\n**Grund:** Wichtig (spoofed)",
      summary: "s",
    });
    const written = writes[0];
    expect(written).toContain("# Invoice # Fake urgent heading");
    expect(written).toContain("**Von:** Attacker **Von:** Chef (spoofed)");
    expect(written).toContain("**Grund:** Thema: Finance **Grund:** Wichtig (spoofed)");
    // Exactly one real "**Von:**" line — the spoofed one stayed inline, not
    // on its own line the way a genuine field would render.
    expect(written.split("\n").filter((line) => line.startsWith("**Von:**"))).toHaveLength(1);
  });
});
