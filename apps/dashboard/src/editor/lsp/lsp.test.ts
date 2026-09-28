import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { EditorDocument } from "../document";
import { cursor, pos, range } from "../position";
import {
  APPLY_EDIT_GRACE_MS,
  CHANGE_DELAY,
  documentUri,
  EXITED,
  fileUri,
  hoverMarkdown,
  LspClient,
  parseLocations,
  uriPath,
} from "./client";
import { DiagnosticSet, parseDiagnostics } from "./diagnostics";
import { lspLanguageId } from "./languages";
import { Rpc, RpcError, type Transport } from "./rpc";

/** An in-memory server end: records what the client sent, and can answer. */
class FakeServer implements Transport {
  sent: Record<string, unknown>[] = [];
  private listener: ((m: string) => void) | null = null;
  /** Answers `initialize` with these capabilities automatically. */
  constructor(private readonly capabilities: Record<string, unknown> | null = { textDocumentSync: 2 }) {}

  send(message: string): void {
    const parsed = JSON.parse(message) as Record<string, unknown>;
    this.sent.push(parsed);
    if (parsed.method === "initialize" && this.capabilities) {
      queueMicrotask(() => this.reply(parsed.id as number, { capabilities: this.capabilities }));
    }
  }
  onMessage(listener: (m: string) => void): () => void {
    this.listener = listener;
    return () => (this.listener = null);
  }
  push(message: unknown): void {
    this.listener?.(JSON.stringify(message));
  }
  reply(id: number | string, result: unknown): void {
    this.push({ jsonrpc: "2.0", id, result });
  }
  of(method: string): Record<string, unknown>[] {
    return this.sent.filter((m) => m.method === method);
  }
}

/** Lets queued promise callbacks run (the timers may be fake). */
async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await Promise.resolve();
}

/** Replaces `r` with `text` the way a keystroke does. */
function type(d: EditorDocument, r: ReturnType<typeof range>, text: string): void {
  d.edit([{ range: r, text }], cursor(r.start));
}

function doc(text: string): EditorDocument {
  return new EditorDocument(text, { indentFallback: { kind: "spaces", size: 4 } });
}

describe("Rpc", () => {
  it("matches responses to requests and rejects errors", async () => {
    const server = new FakeServer(null);
    const rpc = new Rpc(server);
    const a = rpc.request<number>("a");
    const b = rpc.request("b");
    server.reply(2, null);
    server.push({ jsonrpc: "2.0", id: 1, result: 42 });
    expect(await a).toBe(42);
    expect(await b).toBeNull();
    const failing = rpc.request("c");
    server.push({ jsonrpc: "2.0", id: 3, error: { code: -32602, message: "bad params" } });
    await expect(failing).rejects.toBeInstanceOf(RpcError);
  });

  it("answers server requests: a handler, empty answers, configuration, and unknown ones", async () => {
    const server = new FakeServer(null);
    const rpc = new Rpc(server);
    rpc.onRequest("custom/ask", () => ({ ok: true }));
    server.push({ jsonrpc: "2.0", id: 10, method: "custom/ask" });
    server.push({ jsonrpc: "2.0", id: 11, method: "window/workDoneProgress/create", params: {} });
    server.push({ jsonrpc: "2.0", id: 12, method: "workspace/configuration", params: { items: [{}, {}] } });
    server.push({ jsonrpc: "2.0", id: 13, method: "made/up" });
    await settle();
    const byId = new Map(server.sent.map((m) => [m.id, m]));
    expect(byId.get(10)?.result).toEqual({ ok: true });
    expect(byId.get(11)?.result).toBeNull();
    expect(byId.get(12)?.result).toEqual([null, null]);
    expect((byId.get(13)?.error as { code: number }).code).toBe(-32601);
  });

  it("rejects what is pending when closed and ignores junk", async () => {
    const server = new FakeServer(null);
    const rpc = new Rpc(server);
    const pending = rpc.request("slow");
    server.push("not json" as unknown);
    server.push({ jsonrpc: "2.0", id: 999, result: 1 });
    rpc.close();
    await expect(pending).rejects.toBeInstanceOf(RpcError);
    await expect(rpc.request("after")).rejects.toBeInstanceOf(RpcError);
  });
});

describe("LspClient", () => {
  beforeEach(() => vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] }));
  afterEach(() => vi.useRealTimers());

  it("initializes against the root and opens a document with its text", async () => {
    const server = new FakeServer();
    const client = new LspClient(server, { rootPath: "/Users/me/My Project", server: "rust-analyzer" });
    expect(await client.whenReady()).toBe(true);
    expect(client.state).toBe("ready");
    const init = server.of("initialize")[0].params as { rootUri: string; capabilities: object };
    expect(init.rootUri).toBe("file:///Users/me/My%20Project");
    expect(server.of("initialized")).toHaveLength(1);

    const d = doc("fn main() {}\n");
    client.open(d, documentUri("/Users/me/My Project", "src/main.rs"), "rust");
    await settle();
    const opened = server.of("textDocument/didOpen")[0].params as { textDocument: Record<string, unknown> };
    expect(opened.textDocument).toMatchObject({
      uri: "file:///Users/me/My%20Project/src/main.rs",
      languageId: "rust",
      version: 1,
      text: "fn main() {}\n",
    });
  });

  it("sends edits as ranges after a pause, in order, with a rising version", async () => {
    const server = new FakeServer({ textDocumentSync: { openClose: true, change: 2 } });
    const client = new LspClient(server, { rootPath: "/r", server: "x" });
    await client.whenReady();
    const d = doc("abc\ndef");
    client.open(d, "file:///r/a", "rust");
    await settle();
    type(d, range(pos(0, 1), pos(0, 2)), "XY");
    type(d, range(pos(1, 0), pos(1, 0)), "!");
    expect(server.of("textDocument/didChange")).toHaveLength(0);
    vi.advanceTimersByTime(CHANGE_DELAY);
    const change = server.of("textDocument/didChange")[0].params as {
      textDocument: { version: number };
      contentChanges: unknown[];
    };
    expect(change.textDocument.version).toBe(2);
    expect(change.contentChanges).toEqual([
      { range: { start: { line: 0, character: 1 }, end: { line: 0, character: 2 } }, text: "XY" },
      { range: { start: { line: 1, character: 0 }, end: { line: 1, character: 0 } }, text: "!" },
    ]);
  });

  it("sends the whole text to a server that only takes full sync", async () => {
    const server = new FakeServer({ textDocumentSync: 1 });
    const client = new LspClient(server, { rootPath: "/r", server: "x" });
    await client.whenReady();
    const d = doc("one");
    client.open(d, "file:///r/a", "rust");
    await settle();
    type(d, range(pos(0, 3), pos(0, 3)), " two");
    vi.advanceTimersByTime(CHANGE_DELAY);
    const change = server.of("textDocument/didChange")[0].params as { contentChanges: unknown[] };
    expect(change.contentChanges).toEqual([{ text: "one two" }]);
  });

  it("flushes pending edits before a request, and closes with didClose", async () => {
    const server = new FakeServer();
    const client = new LspClient(server, { rootPath: "/r", server: "x" });
    await client.whenReady();
    const d = doc("x");
    const close = client.open(d, "file:///r/a", "rust");
    await settle();
    type(d, range(pos(0, 1), pos(0, 1)), "y");
    void client.request("textDocument/hover", {});
    expect(server.of("textDocument/didChange")).toHaveLength(1);
    close();
    await settle();
    expect(server.of("textDocument/didClose")).toHaveLength(1);
  });

  it("counts two editors on one file and tells the server only when the last one closes", async () => {
    const server = new FakeServer();
    const client = new LspClient(server, { rootPath: "/r", server: "x" });
    await client.whenReady();
    const d = doc("x");
    const first = client.open(d, "file:///r/a", "rust");
    const second = client.open(d, "file:///r/a", "rust");
    await settle();
    expect(server.of("textDocument/didOpen")).toHaveLength(1);
    first();
    first();
    await settle();
    expect(server.of("textDocument/didClose")).toHaveLength(0);
    type(d, range(pos(0, 1), pos(0, 1)), "y");
    vi.advanceTimersByTime(CHANGE_DELAY);
    expect(server.of("textDocument/didChange")).toHaveLength(1);
    second();
    await settle();
    expect(server.of("textDocument/didClose")).toHaveLength(1);
  });

  it("keeps published diagnostics per document and forgets them when the server exits", async () => {
    const server = new FakeServer();
    const client = new LspClient(server, { rootPath: "/r", server: "x" });
    await client.whenReady();
    const seen: string[] = [];
    client.onDiagnostics((uri) => seen.push(uri));
    server.push({
      jsonrpc: "2.0",
      method: "textDocument/publishDiagnostics",
      params: {
        uri: "file:///r/a",
        diagnostics: [
          { range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } }, severity: 1, message: "e" },
        ],
      },
    });
    expect(client.diagnosticsFor("file:///r/a")).toHaveLength(1);
    server.push({ jsonrpc: "2.0", method: EXITED, params: {} });
    expect(client.state).toBe("exited");
    expect(client.diagnosticsFor("file:///r/a")).toHaveLength(0);
    expect(seen).toEqual(["file:///r/a", "file:///r/a"]);
  });

  it("reports a server whose initialize fails", async () => {
    const server = new FakeServer(null);
    const client = new LspClient(server, { rootPath: "/r", server: "x" });
    const init = server.of("initialize")[0];
    server.push({ jsonrpc: "2.0", id: init.id, error: { code: -32603, message: "no workspace" } });
    expect(await client.whenReady()).toBe(false);
    expect(client.state).toBe("failed");
    expect(client.failure).toContain("no workspace");
  });
});

describe("diagnostics", () => {
  const raw = [
    { range: { start: { line: 2, character: 4 }, end: { line: 2, character: 9 } }, severity: 2, message: "unused" },
    { range: { start: { line: 0, character: 3 }, end: { line: 1, character: 2 } }, severity: 1, message: "type" },
    { range: { start: { line: 3, character: 5 }, end: { line: 3, character: 5 } }, message: "missing ;" },
    { range: { start: { line: 4 } }, message: "broken" },
    { range: { start: { line: 2, character: 0 }, end: { line: 2, character: 1 } }, severity: 4, message: "hint",
      code: { value: "E1" }, source: "ra" },
  ];
  const lines = ["let a = 1", "b", "    unused", "x = 1"];
  const set = new DiagnosticSet(parseDiagnostics(raw), (l) => lines[l]?.length ?? 0);

  it("parses, sorts and skips malformed entries", () => {
    expect(set.size).toBe(4);
    expect(set.all.map((d) => d.message)).toEqual(["type", "hint", "unused", "missing ;"]);
    expect(set.all[1]).toMatchObject({ code: "E1", source: "ra", severity: 4 });
    expect(set.all[3].severity).toBe(1);
  });

  it("splits a range over its lines and keeps the worst severity per line", () => {
    expect(set.line(0)?.marks).toEqual([{ from: 3, to: 9, severity: 1 }]);
    expect(set.line(1)?.marks).toEqual([{ from: 0, to: 1, severity: 1 }]);
    expect(set.line(2)?.worst).toBe(2);
    expect(set.line(2)?.message).toBe("unused");
    // An empty range at the end of a line marks the last character.
    expect(set.line(3)?.marks).toEqual([{ from: 4, to: 5, severity: 1 }]);
  });

  it("finds what covers a position and steps to the next and previous problem", () => {
    expect(set.at(pos(2, 5)).map((d) => d.message)).toEqual(["unused"]);
    expect(set.at(pos(3, 5)).map((d) => d.message)).toEqual(["missing ;"]);
    expect(set.next(pos(0, 3))?.message).toBe("hint");
    expect(set.next(pos(3, 9))?.message).toBe("type");
    expect(set.previous(pos(2, 0))?.message).toBe("type");
    expect(set.previous(pos(0, 0))?.message).toBe("missing ;");
    expect(set.counts()).toEqual({ error: 2, warning: 1, info: 0, hint: 1 });
  });
});

describe("uris and language ids", () => {
  it("encodes every path segment", () => {
    expect(fileUri("/a b/ü#?.rs")).toBe("file:///a%20b/%C3%BC%23%3F.rs");
    expect(documentUri("/root/", "src/x.rs")).toBe("file:///root/src/x.rs");
  });

  it("maps editor languages onto protocol ids", () => {
    expect(lspLanguageId("tsx")).toBe("typescriptreact");
    expect(lspLanguageId("bash")).toBe("shellscript");
    expect(lspLanguageId("markdown_inline")).toBeNull();
    expect(lspLanguageId(null)).toBeNull();
  });
});

describe("hover and definition", () => {
  it("turns every shape of hover contents into Markdown", () => {
    expect(hoverMarkdown({ kind: "markdown", value: "**x**" })).toBe("**x**");
    expect(hoverMarkdown({ kind: "plaintext", value: "a*b" })).toBe("a\\*b");
    expect(hoverMarkdown({ language: "rust", value: "fn f()" })).toBe("```rust\nfn f()\n```");
    expect(hoverMarkdown(["one", { language: "ts", value: "x: number" }, ""])).toBe(
      "one\n\n```ts\nx: number\n```",
    );
    expect(hoverMarkdown(null)).toBe("");
  });

  it("fences code longer than any backtick run inside it", () => {
    expect(hoverMarkdown({ language: "md", value: "a ``` b" })).toBe("````md\na ``` b\n````");
  });

  it("reads every place of any location answer", () => {
    const range = { start: { line: 4, character: 2 }, end: { line: 4, character: 9 } };
    expect(parseLocations({ uri: "file:///a.rs", range })).toEqual([
      { uri: "file:///a.rs", at: { line: 4, col: 2 }, end: { line: 4, col: 9 } },
    ]);
    expect(parseLocations([{ uri: "file:///b.rs", range }, { uri: "file:///c.rs", range }]).map((l) => l.uri)).toEqual([
      "file:///b.rs",
      "file:///c.rs",
    ]);
    expect(parseLocations([{ targetUri: "file:///d.rs", targetRange: range, targetSelectionRange: range }])).toEqual([
      { uri: "file:///d.rs", at: { line: 4, col: 2 }, end: { line: 4, col: 9 } },
    ]);
    expect(parseLocations([{ uri: "file:///e.rs" }, 3, null])).toEqual([]);
    expect(parseLocations(null)).toEqual([]);
  });

  it("decodes file URIs back to paths", () => {
    expect(uriPath("file:///Users/me/a%20b/%C3%BC.rs")).toBe("/Users/me/a b/ü.rs");
    expect(uriPath("https://x")).toBeNull();
    expect(uriPath("file:///bad%zz")).toBeNull();
  });

  it("asks the server with the document and position, and answers null on failure", async () => {
    const server = new FakeServer();
    const client = new LspClient(server, { rootPath: "/r", server: "x" });
    await client.whenReady();
    const hovering = client.hover("file:///r/a", pos(1, 3));
    await settle();
    const asked = server.of("textDocument/hover")[0];
    expect(asked.params).toEqual({ textDocument: { uri: "file:///r/a" }, position: { line: 1, character: 3 } });
    server.reply(asked.id as number, { contents: { kind: "markdown", value: "fn main()" } });
    expect(await hovering).toBe("fn main()");

    const defining = client.definition("file:///r/a", pos(0, 0));
    await settle();
    const def = server.of("textDocument/definition")[0];
    server.push({ jsonrpc: "2.0", id: def.id, error: { code: -32603, message: "no" } });
    expect(await defining).toBeNull();
  });

  it("asks for uses with the declaration, and for implementations and type definitions", async () => {
    const server = new FakeServer();
    const client = new LspClient(server, { rootPath: "/r", server: "x" });
    await client.whenReady();
    const range = { start: { line: 1, character: 0 }, end: { line: 1, character: 3 } };
    for (const kind of ["references", "implementation", "typeDefinition"] as const) {
      const asking = client.locations(kind, "file:///r/a", pos(2, 5));
      await settle();
      const asked = server.of(`textDocument/${kind}`)[0];
      const params = asked.params as Record<string, unknown>;
      expect(params.position).toEqual({ line: 2, character: 5 });
      expect(params.context).toEqual(kind === "references" ? { includeDeclaration: true } : undefined);
      server.reply(asked.id as number, [{ uri: "file:///r/b", range }]);
      expect((await asking).map((l) => l.uri)).toEqual(["file:///r/b"]);
    }
  });
});

describe("code actions (ED6.7)", () => {
  const withActions = { textDocumentSync: 2, codeActionProvider: { resolveProvider: true } };
  /** The answer the client wrote back to the server's request `id`. */
  const answerTo = (server: FakeServer, id: string) => server.sent.find((m) => m.id === id && m.method === undefined);

  it("asks for a range with the problems as context, and resolves a chosen action's edit", async () => {
    const server = new FakeServer(withActions);
    const client = new LspClient(server, { rootPath: "/r", server: "x" });
    await client.whenReady();
    expect(client.codeActions).toBe(true);
    const init = server.of("initialize")[0].params as { capabilities: { workspace: Record<string, unknown> } };
    expect(init.capabilities.workspace.applyEdit).toBe(true);

    const problem = { range: { start: { line: 1, character: 0 }, end: { line: 1, character: 2 } }, message: "m" };
    const asking = client.codeActionsAt("file:///r/a", pos(1, 0), pos(1, 2), [problem]);
    await settle();
    const asked = server.of("textDocument/codeAction")[0];
    expect(asked.params).toEqual({
      textDocument: { uri: "file:///r/a" },
      range: { start: { line: 1, character: 0 }, end: { line: 1, character: 2 } },
      context: { diagnostics: [problem], triggerKind: 1 },
    });
    server.reply(asked.id as number, [{ title: "Fix it", kind: "quickfix", data: 7 }]);
    const [item] = await asking;
    expect(item.edit).toBeNull();

    const resolving = client.resolveCodeAction(item);
    await settle();
    const resolve = server.of("codeAction/resolve")[0];
    expect(resolve.params).toEqual({ title: "Fix it", kind: "quickfix", data: 7 });
    server.reply(resolve.id as number, { title: "Fix it", kind: "quickfix", edit: { changes: {} } });
    expect((await resolving).edit).toEqual({ changes: {} });
  });

  it("applies a server's edit only while a command of ours runs, and a moment after", async () => {
    const now = vi.spyOn(Date, "now").mockReturnValue(1000);
    const server = new FakeServer(withActions);
    const client = new LspClient(server, { rootPath: "/r", server: "x" });
    await client.whenReady();
    const applied: unknown[] = [];
    const apply = async (edit: unknown) => {
      applied.push(edit);
      return true;
    };
    const applyEdit = (id: string) =>
      server.push({ jsonrpc: "2.0", id, method: "workspace/applyEdit", params: { edit: { id } } });

    // Unasked: refused.
    applyEdit("before");
    await settle();
    expect(answerTo(server, "before")?.result).toMatchObject({ applied: false });

    const running = client.executeCommand({ title: "Organize", command: "organize" }, apply);
    await settle();
    const execute = server.of("workspace/executeCommand")[0];
    // No `arguments` offered, none sent: Rust compares the echo exactly (L18).
    expect(execute.params).toEqual({ command: "organize" });
    applyEdit("during");
    await settle();
    expect(answerTo(server, "during")?.result).toEqual({ applied: true });
    // A second command meanwhile would not know whose edit is whose: refused.
    await expect(client.executeCommand({ title: "Other", command: "other" }, apply)).rejects.toThrow(/still running/);
    expect(server.of("workspace/executeCommand")).toHaveLength(1);
    server.reply(execute.id as number, null);
    await running;

    now.mockReturnValue(1000 + APPLY_EDIT_GRACE_MS - 1);
    applyEdit("grace");
    await settle();
    expect(answerTo(server, "grace")?.result).toEqual({ applied: true });
    now.mockReturnValue(1000 + APPLY_EDIT_GRACE_MS + 1);
    applyEdit("late");
    await settle();
    expect(answerTo(server, "late")?.result).toMatchObject({ applied: false });
    expect(applied).toEqual([{ id: "during" }, { id: "grace" }]);
    now.mockRestore();
  });

  it("throws when a command fails, with the server's reason", async () => {
    const server = new FakeServer(withActions);
    const client = new LspClient(server, { rootPath: "/r", server: "x" });
    await client.whenReady();
    const running = client.executeCommand({ title: "x", command: "x", arguments: [1] }, async () => true);
    await settle();
    const execute = server.of("workspace/executeCommand")[0];
    expect(execute.params).toEqual({ command: "x", arguments: [1] });
    server.push({ jsonrpc: "2.0", id: execute.id, error: { code: -32603, message: "refused by Rust" } });
    await expect(running).rejects.toThrow("refused by Rust");
  });
});
