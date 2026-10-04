/**
 * JSON-RPC 2.0 over whatever carries a language server's messages
 * (`docs/plans/editor.md`, ED6, L1). The app's transport is a Tauri channel to
 * a server Rust started; tests use an in-memory pair. Messages are whole JSON
 * texts — framing is Rust's.
 *
 * Three kinds of incoming message: a **response** settles the request with
 * its id, a **notification** goes to the listeners for its method, and a
 * **request from the server** is answered by the handler for its method — or
 * with `null` where the protocol allows an empty answer (see {@link Rpc.onRequest}),
 * so a server asking for configuration nobody gives never waits forever.
 */

/** Carries messages to and from one server. */
export interface Transport {
  send(message: string): void;
  /** Subscribes to incoming messages; returns the unsubscribe function. */
  onMessage(listener: (message: string) => void): () => void;
}

/** A failed request: the server's error object. */
export class RpcError extends Error {
  constructor(
    readonly code: number,
    message: string,
    readonly data?: unknown,
  ) {
    super(message);
  }
}

/** The protocol's "request cancelled" and "content modified" codes: stale, not wrong. */
export const REQUEST_CANCELLED = -32800;
export const CONTENT_MODIFIED = -32801;
const METHOD_NOT_FOUND = -32601;

type Pending = { resolve: (value: unknown) => void; reject: (err: Error) => void };

interface Message {
  jsonrpc?: string;
  id?: number | string | null;
  method?: string;
  params?: unknown;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
}

export class Rpc {
  private next = 1;
  private pending = new Map<number | string, Pending>();
  private notifications = new Map<string, Set<(params: unknown) => void>>();
  private requests = new Map<string, (params: unknown) => unknown>();
  private readonly stop: () => void;
  private closed = false;

  constructor(private readonly transport: Transport) {
    this.stop = transport.onMessage((text) => this.receive(text));
  }

  /** Sends a request; settles with the server's result or rejects with an {@link RpcError}. */
  request<T>(method: string, params?: unknown): Promise<T> {
    if (this.closed) return Promise.reject(new RpcError(REQUEST_CANCELLED, "the language server is gone"));
    const id = this.next++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      this.write({ jsonrpc: "2.0", id, method, params });
    });
  }

  /** Sends a notification. */
  notify(method: string, params?: unknown): void {
    if (!this.closed) this.write({ jsonrpc: "2.0", method, params });
  }

  /** Subscribes to a notification from the server; returns the unsubscribe function. */
  onNotification(method: string, listener: (params: unknown) => void): () => void {
    let set = this.notifications.get(method);
    if (!set) this.notifications.set(method, (set = new Set()));
    set.add(listener);
    return () => set.delete(listener);
  }

  /**
   * Answers the server's requests for `method`. Without a handler a request is
   * answered with `null` — right for `window/workDoneProgress/create`,
   * `client/registerCapability` and the like — except `workspace/configuration`,
   * which gets one `null` per item it asked for, and anything unknown, which
   * gets "method not found".
   */
  onRequest(method: string, handler: (params: unknown) => unknown): void {
    this.requests.set(method, handler);
  }

  /** Stops listening; every pending request is rejected. */
  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.stop();
    for (const pending of this.pending.values()) {
      pending.reject(new RpcError(REQUEST_CANCELLED, "the language server is gone"));
    }
    this.pending.clear();
  }

  private write(message: Message): void {
    this.transport.send(JSON.stringify(message));
  }

  private receive(text: string): void {
    let message: Message;
    try {
      message = JSON.parse(text) as Message;
    } catch {
      return;
    }
    if (typeof message !== "object" || message === null) return;
    const hasId = message.id !== undefined && message.id !== null;
    if (message.method === undefined && hasId) {
      const pending = this.pending.get(message.id!);
      if (!pending) return; // an answer nobody waits for (Rust's own shutdown, say)
      this.pending.delete(message.id!);
      if (message.error) pending.reject(new RpcError(message.error.code, message.error.message, message.error.data));
      else pending.resolve(message.result ?? null);
      return;
    }
    if (message.method === undefined) return;
    if (hasId) {
      this.answer(message.id!, message.method, message.params);
      return;
    }
    for (const listener of this.notifications.get(message.method) ?? []) listener(message.params);
  }

  private answer(id: number | string, method: string, params: unknown): void {
    const handler = this.requests.get(method);
    if (handler) {
      Promise.resolve()
        .then(() => handler(params))
        .then(
          (result) => this.write({ jsonrpc: "2.0", id, result: result ?? null }),
          (err: unknown) =>
            this.write({ jsonrpc: "2.0", id, error: { code: -32603, message: String(err) } }),
        );
      return;
    }
    if (method === "workspace/configuration") {
      const items = (params as { items?: unknown[] } | undefined)?.items ?? [];
      this.write({ jsonrpc: "2.0", id, result: items.map(() => null) });
      return;
    }
    if (EMPTY_ANSWER.has(method)) {
      this.write({ jsonrpc: "2.0", id, result: null });
      return;
    }
    this.write({ jsonrpc: "2.0", id, error: { code: METHOD_NOT_FOUND, message: `${method} is not supported` } });
  }
}

/** Server requests a client may answer with nothing. */
const EMPTY_ANSWER = new Set([
  "window/workDoneProgress/create",
  "client/registerCapability",
  "client/unregisterCapability",
  "window/showMessageRequest",
  "workspace/diagnostic/refresh",
  "workspace/semanticTokens/refresh",
  "workspace/inlayHint/refresh",
  "workspace/codeLens/refresh",
]);
