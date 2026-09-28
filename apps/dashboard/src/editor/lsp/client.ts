/**
 * One language server as the editor sees it (`docs/plans/editor.md`, ED6, L1):
 * the `initialize` handshake, the documents open on it, and what it publishes
 * about them. Part of the engine — no DOM, no Svelte, no app imports; the
 * transport is handed in.
 *
 * **Document sync.** A document is opened with its whole text and then follows
 * every replacement the document reports (`EditorDocument.onTextChange`). The
 * changes are collected and sent together after a short pause
 * ({@link CHANGE_DELAY}), as ranges when the server takes them (incremental
 * sync) and as the whole text otherwise. Positions are lines and UTF-16
 * columns on both sides, so nothing is converted.
 */

import type { EditorDocument, TextChange } from "../document";
import { range, type Pos } from "../position";
import { type CompletionAnswer, type CompletionItem, parseCompletion, parseItem } from "./completion";
import { type Diagnostic, parseDiagnostics } from "./diagnostics";
import { parseSignatureHelp, type SignatureView } from "./signature";
import type { TextEdit } from "../textEdits";
import { Rpc, type Transport } from "./rpc";

/** Quiet time after the last keystroke before the server hears about the edits. */
export const CHANGE_DELAY = 150;

/** The protocol's sync kinds. */
const SYNC_NONE = 0;
const SYNC_FULL = 1;
const SYNC_INCREMENTAL = 2;

/** Sent by Rust when the server's output ended (it crashed or was stopped). */
export const EXITED = "$/axiomata/exited";

export type ClientState = "starting" | "ready" | "failed" | "exited";

export interface ClientOptions {
  /** The root folder on disk — the workspace the server works in. */
  rootPath: string;
  /** The server's id (for messages): `rust-analyzer`, `typescript`, … */
  server: string;
}

type ContentChange = { range: ReturnType<typeof lspRange>; text: string } | { text: string };

interface OpenDocument {
  uri: string;
  doc: EditorDocument;
  /** How many editors have it open (two panes on one file share it). */
  users: number;
  version: number;
  queued: ContentChange[];
  full: boolean;
  timer: ReturnType<typeof setTimeout> | null;
  unsubscribe: () => void;
}

/** A `file://` URI for an absolute path, each segment percent-encoded. */
export function fileUri(path: string): string {
  return `file://${path.split("/").map(encodeURIComponent).join("/")}`;
}

/** The path of a `file://` URI, or `null` for anything else. */
export function uriPath(uri: string): string | null {
  if (!uri.startsWith("file://")) return null;
  try {
    const path = decodeURIComponent(uri.slice("file://".length));
    return path.startsWith("/") ? path : null;
  } catch {
    return null;
  }
}

/** A place a server names: a file and a position in it. */
export interface Location {
  uri: string;
  at: Pos;
  /** Where the named symbol ends, when the server says. */
  end?: Pos;
}

/** The requests that answer with places (the method is `textDocument/<kind>`). */
export type LocationKind = "definition" | "implementation" | "typeDefinition" | "references";

/** A path under a root as a `file://` URI. */
export function documentUri(rootPath: string, rel: string): string {
  return fileUri(`${rootPath.replace(/\/+$/, "")}/${rel}`);
}

function lspRange(start: { line: number; col: number }, end: { line: number; col: number }) {
  return {
    start: { line: start.line, character: start.col },
    end: { line: end.line, character: end.col },
  };
}

export class LspClient {
  state: ClientState = "starting";
  /** Why the server failed to start, when it did. */
  failure: string | null = null;
  private readonly rpc: Rpc;
  private readonly ready: Promise<boolean>;
  private syncKind = SYNC_FULL;
  /** Whether the server completes at all, and the characters after which it offers (`.`, `:`). */
  completes = false;
  completionTriggers: readonly string[] = [];
  private resolvesCompletion = false;
  /** Whether the server renames, and checks a place first (`prepareRename`). */
  renames = false;
  private preparesRename = false;
  /** Characters after which the server offers a signature (`(`, `,`), and ones that refresh an open one. */
  signatureTriggers: readonly string[] = [];
  signatureRetriggers: readonly string[] = [];
  private readonly docs = new Map<string, OpenDocument>();
  private readonly diagnostics = new Map<string, Diagnostic[]>();
  private readonly diagnosticListeners = new Set<(uri: string) => void>();
  private readonly stateListeners = new Set<(state: ClientState) => void>();

  constructor(
    transport: Transport,
    readonly options: ClientOptions,
  ) {
    this.rpc = new Rpc(transport);
    this.rpc.onNotification("textDocument/publishDiagnostics", (params) => this.published(params));
    this.rpc.onNotification(EXITED, () => this.exited());
    this.ready = this.initialize();
  }

  /** Waits for the handshake; `false` when the server did not come up. */
  whenReady(): Promise<boolean> {
    return this.ready;
  }

  /**
   * Opens `doc` on the server as `uri` and keeps it in sync until the returned
   * function closes it. The same URI opened again shares the server document
   * (two panes on one file): it is counted, and only the last close tells the
   * server. The first opener's document is the one followed.
   */
  open(doc: EditorDocument, uri: string, languageId: string): () => void {
    let closed = false;
    let counted = false;
    const opening = this.ready.then((ok) => {
      if (!ok || closed) return;
      counted = true;
      const already = this.docs.get(uri);
      if (already) {
        already.users++;
        return;
      }
      this.rpc.notify("textDocument/didOpen", {
        // As on disk (line endings, final newline): what the server would read itself.
        textDocument: { uri, languageId, version: 1, text: doc.textForSave() },
      });
      const open: OpenDocument = {
        uri,
        doc,
        users: 1,
        version: 1,
        queued: [],
        full: false,
        timer: null,
        unsubscribe: doc.onTextChange((change) => this.changed(open, change)),
      };
      this.docs.set(uri, open);
    });
    return () => {
      if (closed) return;
      closed = true;
      void opening.then(() => {
        if (counted) this.close(uri);
      });
    };
  }

  /** What the server last published for `uri`. */
  diagnosticsFor(uri: string): readonly Diagnostic[] {
    return this.diagnostics.get(uri) ?? [];
  }

  /** Subscribes to newly published diagnostics; the listener gets the URI. */
  onDiagnostics(listener: (uri: string) => void): () => void {
    this.diagnosticListeners.add(listener);
    return () => this.diagnosticListeners.delete(listener);
  }

  /** Subscribes to the client's state changing (ready, failed, exited). */
  onState(listener: (state: ClientState) => void): () => void {
    this.stateListeners.add(listener);
    return () => this.stateListeners.delete(listener);
  }

  /** Sends a request after flushing pending edits, so the server answers about the current text. */
  async request<T>(method: string, params: unknown): Promise<T> {
    for (const open of this.docs.values()) this.flush(open);
    return this.rpc.request<T>(method, params);
  }

  /**
   * What the server says about the symbol at `at` (ED6.2), as Markdown —
   * `null` when it has nothing, or the request failed.
   */
  async hover(uri: string, at: Pos): Promise<string | null> {
    try {
      const result = await this.request<{ contents?: unknown } | null>("textDocument/hover", {
        textDocument: { uri },
        position: { line: at.line, character: at.col },
      });
      const text = hoverMarkdown(result?.contents).trim();
      return text === "" ? null : text;
    } catch {
      return null;
    }
  }

  /**
   * Where the symbol at `at` is defined (ED6.2) — the first place the server
   * names, `null` when it names none or the request failed.
   */
  async definition(uri: string, at: Pos): Promise<Location | null> {
    return (await this.locations("definition", uri, at))[0] ?? null;
  }

  /**
   * The places a location request names for the symbol at `at` (ED6.2/ED6.3):
   * its definition, its implementations, its type's definition, or every use
   * (`references`, the declaration included). Empty when there are none, the
   * server does not answer this request, or it failed.
   */
  async locations(kind: LocationKind, uri: string, at: Pos): Promise<Location[]> {
    const params: Record<string, unknown> = {
      textDocument: { uri },
      position: { line: at.line, character: at.col },
    };
    if (kind === "references") params.context = { includeDeclaration: true };
    try {
      return parseLocations(await this.request<unknown>(`textDocument/${kind}`, params));
    } catch {
      return [];
    }
  }

  /**
   * What the server offers at `at` (ED6.4). `trigger` is the character just
   * typed when it is one of the server's trigger characters; `again` when the
   * last answer was incomplete. Nothing when the request failed.
   */
  async completion(uri: string, at: Pos, trigger: string | null, again = false): Promise<CompletionAnswer> {
    // The protocol's trigger kinds: invoked, a trigger character, re-asked for an incomplete list.
    const context = trigger ? { triggerKind: 2, triggerCharacter: trigger } : { triggerKind: again ? 3 : 1 };
    try {
      const result = await this.request<unknown>("textDocument/completion", {
        textDocument: { uri },
        position: { line: at.line, character: at.col },
        context,
      });
      return parseCompletion(result);
    } catch {
      return { items: [], incomplete: false };
    }
  }

  /**
   * An item with what the server fills in only when asked (documentation, an
   * auto-import's edits); the item as it was when it has nothing more.
   */
  async resolveCompletion(item: CompletionItem): Promise<CompletionItem> {
    if (!this.resolvesCompletion) return item;
    try {
      const resolved = parseItem(await this.request<unknown>("completionItem/resolve", item.raw));
      if (!resolved) return item;
      // The edit decided when the list came stays; what the resolve adds is taken.
      return {
        ...item,
        detail: item.detail || resolved.detail,
        documentation: resolved.documentation ?? item.documentation,
        additionalEdits: resolved.additionalEdits.length > 0 ? resolved.additionalEdits : item.additionalEdits,
      };
    } catch {
      return item;
    }
  }

  /**
   * The server's formatting of `uri` (ED6.5, the fallback where no formatter
   * of our own fits — L13): its edits, or `null` when it does not format.
   */
  async formatting(uri: string, options: { tabSize: number; insertSpaces: boolean }): Promise<TextEdit[] | null> {
    try {
      const result = await this.request<unknown>("textDocument/formatting", { textDocument: { uri }, options });
      return Array.isArray(result) ? parseTextEdits(result) : null;
    } catch {
      return null;
    }
  }

  /**
   * The signature of the call at `at` (ED6.6): `null` when there is none (the
   * cursor left the call) or the request failed. `trigger` is the character
   * just typed, `retrigger` whether one is showing already.
   */
  async signatureHelp(uri: string, at: Pos, trigger: string | null, retrigger: boolean): Promise<SignatureView | null> {
    try {
      const result = await this.request<unknown>("textDocument/signatureHelp", {
        textDocument: { uri },
        position: { line: at.line, character: at.col },
        context: {
          // The protocol's trigger kinds: invoked, a trigger character, the content changed.
          triggerKind: trigger ? 2 : retrigger ? 3 : 1,
          ...(trigger ? { triggerCharacter: trigger } : {}),
          isRetrigger: retrigger,
        },
      });
      return parseSignatureHelp(result);
    } catch {
      return null;
    }
  }

  /**
   * Whether the symbol at `at` can be renamed, and its current name (ED6.5,
   * L17): `null` when the server says there is nothing to rename there. A
   * server without the check answers with the word at `at` (`fallback`).
   */
  async prepareRename(uri: string, at: Pos, fallback: string): Promise<{ name: string } | null> {
    if (!this.preparesRename) return fallback ? { name: fallback } : null;
    try {
      const result = await this.request<unknown>("textDocument/prepareRename", {
        textDocument: { uri },
        position: { line: at.line, character: at.col },
      });
      if (result === null || result === undefined) return null;
      const r = result as { placeholder?: unknown };
      return { name: typeof r.placeholder === "string" ? r.placeholder : fallback };
    } catch {
      return null;
    }
  }

  /**
   * Renames the symbol at `at` to `newName` (ED6.5, L16): the edits per file
   * URI. Throws with the server's reason when it refuses, and when it would
   * move or create files — which is not offered (L16).
   */
  async rename(uri: string, at: Pos, newName: string): Promise<Map<string, TextEdit[]>> {
    const result = await this.request<unknown>("textDocument/rename", {
      textDocument: { uri },
      position: { line: at.line, character: at.col },
      newName,
    });
    return parseWorkspaceEdit(result);
  }

  /** The document open on this server as `uri`, if one is (another editor's, perhaps). */
  documentFor(uri: string): EditorDocument | null {
    return this.docs.get(uri)?.doc ?? null;
  }

  /** Stops following every document; the server itself is Rust's to stop. */
  dispose(): void {
    for (const open of this.docs.values()) {
      open.unsubscribe();
      if (open.timer) clearTimeout(open.timer);
    }
    this.docs.clear();
    this.rpc.close();
  }

  private async initialize(): Promise<boolean> {
    const rootUri = fileUri(this.options.rootPath);
    try {
      const result = await this.rpc.request<{
        capabilities?: {
          textDocumentSync?: unknown;
          completionProvider?: unknown;
          renameProvider?: unknown;
          signatureHelpProvider?: unknown;
        };
      }>("initialize", {
        processId: null,
        clientInfo: { name: "Axiomata-OS" },
        rootUri,
        rootPath: this.options.rootPath,
        workspaceFolders: [{ uri: rootUri, name: this.options.rootPath.split("/").pop() || rootUri }],
        capabilities: {
          general: { positionEncodings: ["utf-16"] },
          textDocument: {
            synchronization: { dynamicRegistration: false, willSave: false, didSave: false },
            publishDiagnostics: { relatedInformation: false, versionSupport: true },
            hover: { contentFormat: ["markdown", "plaintext"] },
            definition: { linkSupport: true },
            implementation: { linkSupport: true },
            typeDefinition: { linkSupport: true },
            references: {},
            completion: {
              contextSupport: true,
              completionItem: {
                snippetSupport: true,
                documentationFormat: ["markdown", "plaintext"],
                labelDetailsSupport: true,
                insertReplaceSupport: true,
                resolveSupport: { properties: ["documentation", "detail", "additionalTextEdits"] },
              },
              completionList: { itemDefaults: ["editRange", "insertTextFormat"] },
            },
            formatting: {},
            signatureHelp: {
              contextSupport: true,
              signatureInformation: {
                documentationFormat: ["markdown", "plaintext"],
                parameterInformation: { labelOffsetSupport: true },
                activeParameterSupport: true,
              },
            },
            rename: { prepareSupport: true },
          },
          // A rename's edits come as text only: moving or creating files is not offered (L16).
          workspace: {
            workspaceFolders: true,
            configuration: false,
            workspaceEdit: { documentChanges: true, resourceOperations: [] },
          },
          window: { workDoneProgress: false },
        },
      });
      this.syncKind = syncKindOf(result?.capabilities?.textDocumentSync);
      const provider = result?.capabilities?.completionProvider as
        | { triggerCharacters?: unknown; resolveProvider?: unknown }
        | undefined;
      this.completes = typeof provider === "object" && provider !== null;
      this.completionTriggers = Array.isArray(provider?.triggerCharacters)
        ? provider.triggerCharacters.filter((c): c is string => typeof c === "string")
        : [];
      this.resolvesCompletion = provider?.resolveProvider === true;
      const rename = result?.capabilities?.renameProvider;
      this.renames = rename === true || (typeof rename === "object" && rename !== null);
      this.preparesRename = (rename as { prepareProvider?: unknown } | undefined)?.prepareProvider === true;
      const signature = result?.capabilities?.signatureHelpProvider as
        | { triggerCharacters?: unknown; retriggerCharacters?: unknown }
        | undefined;
      const chars = (list: unknown) =>
        Array.isArray(list) ? list.filter((c): c is string => typeof c === "string") : [];
      this.signatureTriggers = chars(signature?.triggerCharacters);
      this.signatureRetriggers = chars(signature?.retriggerCharacters);
      this.rpc.notify("initialized", {});
      this.setState("ready");
      return true;
    } catch (err) {
      this.failure = err instanceof Error ? err.message : String(err);
      this.setState(this.state === "exited" ? "exited" : "failed");
      return false;
    }
  }

  private changed(open: OpenDocument, change: TextChange): void {
    if (this.syncKind === SYNC_NONE) return;
    if (this.syncKind === SYNC_INCREMENTAL && !open.full) {
      // The store already holds the new text: what went in is start..newEnd.
      const text = open.doc.store.slice(range(change.start, change.newEnd));
      open.queued.push({ range: lspRange(change.start, change.oldEnd), text });
    } else {
      open.full = true;
    }
    if (open.timer) clearTimeout(open.timer);
    open.timer = setTimeout(() => this.flush(open), CHANGE_DELAY);
  }

  private flush(open: OpenDocument): void {
    if (open.timer) {
      clearTimeout(open.timer);
      open.timer = null;
    }
    if (!open.full && open.queued.length === 0) return;
    const contentChanges: ContentChange[] = open.full ? [{ text: open.doc.textForSave() }] : open.queued;
    open.queued = [];
    open.full = false;
    open.version++;
    this.rpc.notify("textDocument/didChange", {
      textDocument: { uri: open.uri, version: open.version },
      contentChanges,
    });
  }

  private close(uri: string): void {
    const open = this.docs.get(uri);
    if (!open) return;
    open.users--;
    if (open.users > 0) return;
    this.flush(open);
    open.unsubscribe();
    this.docs.delete(uri);
    this.rpc.notify("textDocument/didClose", { textDocument: { uri } });
    this.diagnostics.delete(uri);
    for (const listener of this.diagnosticListeners) listener(uri);
  }

  private published(params: unknown): void {
    const p = params as { uri?: unknown; diagnostics?: unknown } | null;
    if (typeof p?.uri !== "string") return;
    this.diagnostics.set(p.uri, parseDiagnostics(p.diagnostics));
    for (const listener of this.diagnosticListeners) listener(p.uri);
  }

  private exited(): void {
    this.setState("exited");
    this.rpc.close();
    for (const uri of this.diagnostics.keys()) {
      this.diagnostics.delete(uri);
      for (const listener of this.diagnosticListeners) listener(uri);
    }
  }

  private setState(state: ClientState): void {
    this.state = state;
    for (const listener of this.stateListeners) listener(state);
  }
}

/** The server's `textDocumentSync`: a kind, or options carrying one. */
function syncKindOf(value: unknown): number {
  if (typeof value === "number") return value;
  if (typeof value === "object" && value !== null) {
    const change = (value as { change?: unknown }).change;
    if (typeof change === "number") return change;
    // Options without `change` mean the server wants no edits.
    return SYNC_NONE;
  }
  return SYNC_FULL;
}

/**
 * A hover's `contents` as Markdown: `MarkupContent` (`{kind, value}`), a
 * `MarkedString` (a string, or `{language, value}` — a code block), or a list
 * of them.
 */
export function hoverMarkdown(contents: unknown): string {
  if (typeof contents === "string") return contents;
  if (Array.isArray(contents)) return contents.map(hoverMarkdown).filter((t) => t.trim() !== "").join("\n\n");
  if (typeof contents === "object" && contents !== null) {
    const c = contents as { kind?: unknown; language?: unknown; value?: unknown };
    if (typeof c.value !== "string") return "";
    if (typeof c.language === "string") {
      // A fence longer than any backtick run inside, so the code cannot close it early.
      const longest = Math.max(0, ...(c.value.match(/`+/g) ?? []).map((run) => run.length));
      const fence = "`".repeat(Math.max(3, longest + 1));
      return `${fence}${c.language}\n${c.value}\n${fence}`;
    }
    if (c.kind === "plaintext") return c.value.replace(/[\\`*_{}[\]()#+\-.!<>]/g, "\\$&");
    return c.value;
  }
  return "";
}

/** The places of a location answer: a `Location`, a list of them, or `LocationLink`s; malformed ones skipped. */
export function parseLocations(result: unknown): Location[] {
  const items = Array.isArray(result) ? result : result == null ? [] : [result];
  const out: Location[] = [];
  for (const item of items) {
    if (typeof item !== "object" || item === null) continue;
    const r = item as {
      uri?: unknown;
      range?: { start?: RawPos; end?: RawPos };
      targetUri?: unknown;
      targetSelectionRange?: { start?: RawPos; end?: RawPos };
    };
    const uri = typeof r.uri === "string" ? r.uri : typeof r.targetUri === "string" ? r.targetUri : null;
    const range = typeof r.uri === "string" ? r.range : r.targetSelectionRange;
    const at = posOf(range?.start);
    if (!uri || !at) continue;
    const end = posOf(range?.end);
    out.push(end ? { uri, at, end } : { uri, at });
  }
  return out;
}

/**
 * A `WorkspaceEdit` as the text edits per file URI — from `documentChanges`
 * or `changes`. Throws when it would create, rename or delete a file.
 */
export function parseWorkspaceEdit(raw: unknown): Map<string, TextEdit[]> {
  const out = new Map<string, TextEdit[]>();
  const add = (uri: unknown, edits: unknown) => {
    if (typeof uri !== "string" || !Array.isArray(edits)) return;
    out.set(uri, [...(out.get(uri) ?? []), ...parseTextEdits(edits)]);
  };
  const edit = raw as { changes?: Record<string, unknown>; documentChanges?: unknown[] } | null;
  if (Array.isArray(edit?.documentChanges)) {
    for (const change of edit.documentChanges) {
      const c = change as { kind?: unknown; textDocument?: { uri?: unknown }; edits?: unknown };
      if (c?.kind !== undefined) throw new Error("the rename would create, move or delete files");
      add(c?.textDocument?.uri, c?.edits);
    }
  } else if (edit?.changes && typeof edit.changes === "object") {
    for (const [uri, edits] of Object.entries(edit.changes)) add(uri, edits);
  }
  return out;
}

/** A server's `TextEdit[]`, malformed ones skipped. */
export function parseTextEdits(raw: readonly unknown[]): TextEdit[] {
  const out: TextEdit[] = [];
  for (const item of raw) {
    const e = item as { range?: { start?: RawPos; end?: RawPos }; newText?: unknown } | null;
    const start = posOf(e?.range?.start);
    const end = posOf(e?.range?.end);
    if (start && end && typeof e?.newText === "string") out.push({ range: { start, end }, text: e.newText });
  }
  return out;
}

type RawPos = { line?: unknown; character?: unknown };

function posOf(raw: RawPos | undefined): Pos | null {
  return typeof raw?.line === "number" && typeof raw.character === "number"
    ? { line: raw.line, col: raw.character }
    : null;
}
