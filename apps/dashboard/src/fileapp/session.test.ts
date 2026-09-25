import { describe, expect, it } from "vitest";

import type { EditorRecovery, FileVersion, TextFile } from "../core/backend";
import { run } from "../editor/commands";
import { ctx } from "../editor/testing";
import { DRAFT_REL, DRAFT_ROOT, FileSession, type FileBackend } from "./session";

const INDENT = { kind: "spaces" as const, size: 4 };

/** An in-memory disk with the file service's version and conflict rules. */
class FakeBackend implements FileBackend {
  files = new Map<string, string>();
  recoveries = new Map<string, EditorRecovery>();
  watched = new Set<string>();
  writes = 0;
  /** Every `unwatch` call, even a no-op one (nothing watched under that key) —
   *  `watched.size` alone can't tell "never watched" apart from "watched,
   *  then unwatched", which is exactly what the untitled-session guarantee
   *  ("never watched") needs to distinguish. */
  unwatchCalls = 0;

  static version(content: string): FileVersion {
    return `${content.length}-${[...content].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7)}`;
  }

  async read(root: string, rel: string): Promise<TextFile> {
    const content = this.files.get(`${root}:${rel}`);
    if (content === undefined) throw { kind: "NotFound", message: `${rel} does not exist` };
    return { rel, content, version: FakeBackend.version(content), modified: null, large: content.length > 1000 };
  }

  async write(root: string, rel: string, content: string, expected: FileVersion | null): Promise<FileVersion> {
    const current = this.files.get(`${root}:${rel}`);
    if (expected !== null && (current === undefined || FakeBackend.version(current) !== expected)) {
      throw { kind: "Conflict", message: `${rel} changed since it was read` };
    }
    this.writes++;
    this.files.set(`${root}:${rel}`, content);
    return FakeBackend.version(content);
  }

  async watch(root: string, rel: string): Promise<void> {
    this.watched.add(`${root}:${rel}`);
  }

  async unwatch(root: string, rel: string): Promise<void> {
    this.unwatchCalls++;
    this.watched.delete(`${root}:${rel}`);
  }

  async recoverySave(root: string, rel: string, base: FileVersion | null, content: string): Promise<void> {
    this.recoveries.set(`${root}:${rel}`, { root, rel, base_version: base, content, saved_at: "now" });
  }

  async recoveryLoad(root: string, rel: string): Promise<EditorRecovery | null> {
    return this.recoveries.get(`${root}:${rel}`) ?? null;
  }

  async recoveryDelete(root: string, rel: string): Promise<void> {
    this.recoveries.delete(`${root}:${rel}`);
  }

  /** Someone else writes the file. */
  external(rel: string, content: string | null) {
    const key = `workspace:${rel}`;
    const existed = this.files.has(key);
    if (content === null) this.files.delete(key);
    else this.files.set(key, content);
    return {
      root: "workspace",
      rel,
      kind: content === null ? ("deleted" as const) : existed ? ("modified" as const) : ("created" as const),
      version: content === null ? null : FakeBackend.version(content),
    };
  }
}

async function opened(content = "hello\n") {
  const backend = new FakeBackend();
  backend.files.set("workspace:a.md", content);
  const session = await FileSession.open(backend, "workspace", "a.md", INDENT);
  return { backend, session };
}

function typeInto(session: FileSession, text: string) {
  run(session.doc, { type: "move", motion: "docEnd", extend: false }, ctx());
  run(session.doc, { type: "insert", text }, ctx());
}

describe("opening and saving", () => {
  it("opens, watches, and saves with the expected version", async () => {
    const { backend, session } = await opened();
    expect(backend.watched.has("workspace:a.md")).toBe(true);
    expect(await session.save()).toBe("unchanged");
    typeInto(session, "!");
    expect(await session.save()).toBe("saved");
    expect(backend.files.get("workspace:a.md")).toBe("hello!\n");
    expect(session.doc.dirty).toBe(false);
  });

  it("refuses to overwrite an external change and raises the banner", async () => {
    const { backend, session } = await opened();
    typeInto(session, " mine");
    backend.external("a.md", "agent wrote this\n");
    expect(await session.save()).toBe("conflict");
    expect(session.banner).toEqual({ kind: "external", confirmReload: false });
    expect(backend.files.get("workspace:a.md")).toBe("agent wrote this\n");
  });

  it("keeps a large file read-only", async () => {
    const { session } = await opened("x".repeat(2000));
    typeInto(session, "y");
    expect(await session.save()).toBe("readOnly");
  });

  it("stops watching when closed", async () => {
    const { backend, session } = await opened();
    await session.close();
    expect(backend.watched.size).toBe(0);
    expect(backend.unwatchCalls).toBe(1);
  });
});

describe("external changes (F10)", () => {
  it("ignores the echo of its own save", async () => {
    const { backend, session } = await opened();
    typeInto(session, "!");
    await session.save();
    const version = FakeBackend.version("hello!\n");
    const echo = { root: "workspace", rel: "a.md", kind: "modified" as const, version };
    expect(await session.onExternalChange(echo)).toBe(false);
    expect(session.banner).toBeNull();
    expect(backend.writes).toBe(1);
  });

  it("reloads a clean document silently, with a hint", async () => {
    const { backend, session } = await opened();
    expect(await session.onExternalChange(backend.external("a.md", "new text\n"))).toBe(true);
    expect(session.doc.store.text()).toBe("new text");
    expect(session.banner).toEqual({ kind: "reloaded" });
  });

  it("never touches unsaved edits; reload takes a second click", async () => {
    const { backend, session } = await opened();
    typeInto(session, " mine");
    await session.onExternalChange(backend.external("a.md", "theirs\n"));
    expect(session.banner).toEqual({ kind: "external", confirmReload: false });
    expect(session.doc.store.text()).toBe("hello mine");

    await session.requestReload();
    expect(session.banner).toEqual({ kind: "external", confirmReload: true });
    expect(session.doc.store.text()).toBe("hello mine");
    await session.requestReload();
    expect(session.doc.store.text()).toBe("theirs");
    expect(session.banner).toBeNull();
  });

  it("keep mine: the next save asks, then overwrites", async () => {
    const { backend, session } = await opened();
    typeInto(session, " mine");
    await session.onExternalChange(backend.external("a.md", "theirs\n"));
    session.keepMine();
    expect(await session.save()).toBe("needsConfirm");
    expect(session.banner).toEqual({ kind: "confirmOverwrite" });
    expect(await session.save(true)).toBe("saved");
    expect(backend.files.get("workspace:a.md")).toBe("hello mine\n");
  });

  it("reports a deletion and recreates the file on save", async () => {
    const { backend, session } = await opened();
    await session.onExternalChange(backend.external("a.md", null));
    expect(session.banner).toEqual({ kind: "deleted" });
    expect(await session.save()).toBe("saved");
    expect(backend.files.get("workspace:a.md")).toBe("hello\n");
  });

  it("ignores changes to other files", async () => {
    const { backend, session } = await opened();
    backend.files.set("workspace:b.md", "b");
    expect(await session.onExternalChange(backend.external("b.md", "b2"))).toBe(false);
  });
});

describe("recovery (F8)", () => {
  it("keeps unsaved text aside and drops it on save", async () => {
    const { backend, session } = await opened();
    typeInto(session, " draft");
    await session.persistRecovery();
    expect(backend.recoveries.get("workspace:a.md")?.content).toBe("hello draft\n");
    await session.save();
    expect(backend.recoveries.size).toBe(0);
  });

  it("offers kept text back on the next open, and restores it undoably", async () => {
    const backend = new FakeBackend();
    backend.files.set("workspace:a.md", "hello\n");
    await backend.recoverySave("workspace", "a.md", FakeBackend.version("hello\n"), "hello draft\n");
    const session = await FileSession.open(backend, "workspace", "a.md", INDENT);
    expect(session.banner).toMatchObject({ kind: "recovery", changedSince: false });

    await session.restoreRecovery();
    expect(session.doc.store.text()).toBe("hello draft");
    expect(session.doc.dirty).toBe(true);
    session.doc.undo();
    expect(session.doc.store.text()).toBe("hello");
  });

  it("says when the file changed since, and asks before the next save", async () => {
    const backend = new FakeBackend();
    backend.files.set("workspace:a.md", "changed meanwhile\n");
    await backend.recoverySave("workspace", "a.md", FakeBackend.version("hello\n"), "hello draft\n");
    const session = await FileSession.open(backend, "workspace", "a.md", INDENT);
    expect(session.banner).toMatchObject({ kind: "recovery", changedSince: true });
    await session.restoreRecovery();
    expect(await session.save()).toBe("needsConfirm");
  });

  it("forgets a kept copy that equals the file, and discards on request", async () => {
    const backend = new FakeBackend();
    backend.files.set("workspace:a.md", "same\n");
    await backend.recoverySave("workspace", "a.md", null, "same\n");
    const session = await FileSession.open(backend, "workspace", "a.md", INDENT);
    expect(session.banner).toBeNull();
    expect(backend.recoveries.size).toBe(0);

    await backend.recoverySave("workspace", "a.md", null, "other\n");
    const again = await FileSession.open(backend, "workspace", "a.md", INDENT);
    await again.discardRecovery();
    expect(backend.recoveries.size).toBe(0);
    expect(again.banner).toBeNull();
  });
});

describe("a new note's untitled session (ED4, W4)", () => {
  it("starts empty, is never written or watched, and keeps its draft aside under its own key", async () => {
    const backend = new FakeBackend();
    const s = await FileSession.untitled(backend, INDENT);
    expect(s.untitled).toBe(true);
    expect(s.doc.store.text()).toBe("");
    expect(backend.watched.size).toBe(0);
    run(s.doc, { type: "insert", text: "# Idea" }, ctx());
    expect(await s.save()).toBe("unchanged");
    expect(backend.writes).toBe(0);
    await s.persistRecovery();
    expect(backend.recoveries.get(`${DRAFT_ROOT}:${DRAFT_REL}`)?.content).toBe("# Idea");
    await s.close();
  });

  it("offers last time's draft back", async () => {
    const backend = new FakeBackend();
    await backend.recoverySave(DRAFT_ROOT, DRAFT_REL, null, "draft text");
    const s = await FileSession.untitled(backend, INDENT);
    expect(s.banner).toMatchObject({ kind: "recovery", changedSince: false });
    await s.restoreRecovery();
    expect(s.doc.store.text()).toBe("draft text");
  });

  it("empties the draft on :e! instead of reading a file that is not there", async () => {
    const backend = new FakeBackend();
    const s = await FileSession.untitled(backend, INDENT);
    run(s.doc, { type: "insert", text: "gone" }, ctx());
    await s.discardChanges();
    expect(s.doc.store.text()).toBe("");
    expect(await s.diskText()).toBe("");
  });

  it("persistRecovery on a clean document deletes an earlier draft that was offered but never restored", async () => {
    const backend = new FakeBackend();
    await backend.recoverySave(DRAFT_ROOT, DRAFT_REL, null, "leftover draft");
    const s = await FileSession.untitled(backend, INDENT);
    expect(s.doc.dirty).toBe(false);
    expect(backend.recoveries.size).toBe(1);
    await s.persistRecovery();
    expect(backend.recoveries.size).toBe(0);
  });

  it("close() never unwatches, since an untitled session was never watched", async () => {
    const backend = new FakeBackend();
    const s = await FileSession.untitled(backend, INDENT);
    await s.close();
    expect(backend.unwatchCalls).toBe(0);
  });

  it("never reacts to a change for a real workspace file that happens to share the draft's name", async () => {
    const backend = new FakeBackend();
    const s = await FileSession.untitled(backend, INDENT);
    const collision = { root: "workspace", rel: DRAFT_REL, kind: "modified" as const, version: "v1" };
    expect(await s.onExternalChange(collision)).toBe(false);
    expect(s.banner).toBeNull();
  });
});
