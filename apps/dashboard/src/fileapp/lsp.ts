/**
 * The file app's language servers (`docs/plans/editor.md`, ED6): one
 * `LspClient` per (root, server) for this page, reached through the Tauri
 * commands in `src-tauri/src/lsp.rs`. The page names a root and a language;
 * which program runs is Rust's decision (L2).
 *
 * * **One client per server, shared.** TypeScript and TSX files of one root
 *   talk to one server; Rust hands the same page the running server back, and
 *   this module the same client.
 * * **Rust counts open documents** (`lsp_opened`/`lsp_closed`) so an unused
 *   server stops after ten minutes (L4).
 * * **A missing server is a quiet hint**, once per server per page, with the
 *   install command (L10) — not an error.
 * * **A server that exits is forgotten**; the next file of its language
 *   starts it again.
 */

import { Channel } from "@tauri-apps/api/core";

import { insideTauri, invokeBackend } from "../core/backend";
import { toast } from "../core/toast";
import type { EditorDocument } from "../editor/document";
import { documentUri, LspClient, uriPath, type Location } from "../editor/lsp/client";
import { FOREIGN_ROOT } from "./session";
import { lspLanguageId } from "../editor/lsp/languages";
import type { Transport } from "../editor/lsp/rpc";

/** This page's token: a reload is a new client, and Rust restarts its servers. */
const PAGE = crypto.randomUUID();

/** What `lsp_start` answers (`axiomata_files::lsp::Started`). */
type Started =
  | { kind: "running"; handle: number; server: string; root_path: string }
  | { kind: "none" }
  | { kind: "disabled"; server: string }
  | { kind: "missing"; server: string; install: string };

/** A running server this page talks to. */
export interface LspConnection {
  client: LspClient;
  handle: number;
  rootPath: string;
  server: string;
}

const byLanguage = new Map<string, Promise<LspConnection | null>>();
/**
 * Starts in one root run one after another: two languages of one server
 * (`.ts` and `.tsx`) started at once would otherwise each make a client, and
 * the second would never hear from the server — Rust answers it with the
 * running server, whose messages go to the first page channel.
 */
const startsByRoot = new Map<string, Promise<unknown>>();
const byServer = new Map<string, LspConnection>();
const hinted = new Set<string>();

/** The connection for `language` files in `root`, started on first use; `null` when there is none. */
export function lspFor(root: string, language: string): Promise<LspConnection | null> {
  const key = `${root}\0${language}`;
  let found = byLanguage.get(key);
  if (!found) {
    const before = startsByRoot.get(root) ?? Promise.resolve();
    found = before.then(() => connect(root, language));
    startsByRoot.set(
      root,
      found.catch(() => null),
    );
    byLanguage.set(key, found);
  }
  return found;
}

async function connect(root: string, language: string): Promise<LspConnection | null> {
  const listeners = new Set<(message: string) => void>();
  // The browser mock has no Tauri channel; it gets a plain object with the same `onmessage`.
  const channel = insideTauri() ? new Channel<string>() : { onmessage: (_: string) => {} };
  channel.onmessage = (message: string) => {
    for (const listener of listeners) listener(message);
  };
  let started: Started;
  try {
    started = await invokeBackend<Started>("lsp_start", { root, language, page: PAGE, onMessage: channel });
  } catch (err) {
    console.warn("language server did not start", err);
    return null;
  }
  if (started.kind === "missing") {
    if (!hinted.has(started.server)) {
      hinted.add(started.server);
      toast(`No ${started.server} language server found — install it with: ${started.install}`, "info");
    }
    return null;
  }
  if (started.kind !== "running") return null;

  const serverKey = `${root}\0${started.server}`;
  const shared = byServer.get(serverKey);
  if (shared && shared.handle === started.handle && shared.client.state !== "exited") return shared;

  const transport: Transport = {
    send: (message) => {
      void invokeBackend<void>("lsp_send", { handle: started.handle, page: PAGE, message }).catch(() => {});
    },
    onMessage: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
  const client = new LspClient(transport, { rootPath: started.root_path, server: started.server });
  const connection: LspConnection = {
    client,
    handle: started.handle,
    rootPath: started.root_path,
    server: started.server,
  };
  byServer.set(serverKey, connection);
  client.onState((state) => {
    if (state !== "exited" && state !== "failed") return;
    // Forget it everywhere, so the next file of its language starts a fresh one.
    byServer.delete(serverKey);
    for (const [key, pending] of byLanguage) {
      void pending.then((c) => {
        if (c === connection) byLanguage.delete(key);
      });
    }
  });
  return connection;
}

/** A document open on a server: its URI, and how to let go of it. */
export interface LspDocument {
  connection: LspConnection;
  uri: string;
  close(): void;
}

/**
 * Opens `doc` (the file `rel` in `root`) on the server for `language`, or
 * resolves to `null` when there is none. `close` hands it back.
 */
export async function openOnServer(
  root: string,
  rel: string,
  language: string | null,
  doc: EditorDocument,
): Promise<LspDocument | null> {
  const languageId = lspLanguageId(language);
  if (!language || !languageId) return null;
  const connection = await lspFor(root, language);
  if (!connection) return null;
  const uri = documentUri(connection.rootPath, rel);
  const closeOnServer = connection.client.open(doc, uri, languageId);
  void invokeBackend<void>("lsp_opened", { handle: connection.handle, page: PAGE }).catch(() => {});
  let closed = false;
  return {
    connection,
    uri,
    close: () => {
      if (closed) return;
      closed = true;
      closeOnServer();
      void invokeBackend<void>("lsp_closed", { handle: connection.handle, page: PAGE }).catch(() => {});
    },
  };
}

/**
 * Where a definition is, as a file the editor can open: under the document's
 * own root when it lies there, otherwise the server's read-only root
 * `lsp:<handle>` with the absolute path (L11) — Rust lets it read only what
 * the server named. `null` for a place that is not a file.
 */
export function definitionFile(
  root: string,
  opened: LspDocument,
  location: Location,
): { root: string; rel: string } | null {
  const path = uriPath(location.uri);
  if (!path) return null;
  const base = `${opened.connection.rootPath.replace(/\/+$/, "")}/`;
  if (path.startsWith(base)) return { root, rel: path.slice(base.length) };
  return { root: `${FOREIGN_ROOT}${opened.connection.handle}`, rel: path };
}
