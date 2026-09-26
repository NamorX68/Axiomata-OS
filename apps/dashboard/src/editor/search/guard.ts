/**
 * The search guard (`docs/plans/editor.md`, ED5, T5): no regular expression
 * runs on the main thread before a worker has run it on the same text in time.
 *
 * JavaScript cannot stop a regular expression once it runs, and one with
 * nested repetition (`(a+)+$`) can take minutes on an unlucky line — on the
 * main thread that freezes the whole app. So every pattern is first run by a
 * worker, against a fixed version of the text (a rope snapshot, T1), with a
 * time limit: if it does not answer in {@link VERIFY_TIMEOUT_MS} the worker is
 * killed and a fresh one started. The verdict is kept per text version and
 * pattern:
 *
 * * **ok**: the pattern finishes on this whole text, and the main thread may
 *   run it again on the same version — it costs no more than it did in the
 *   worker. The verdict also carries the matches (the first
 *   {@link MAX_REPORTED}, and how many there are), so a count or a jump
 *   usually needs no second run at all.
 * * **too expensive / invalid**: the caller says so and does not run it.
 *
 * Until a verdict is there, a caller either waits (`require` throws
 * {@link SearchPending}, which Vi's machine turns into a paused key queue, as
 * it does for the clipboard) or draws nothing and redraws when
 * {@link SearchGuard.onVerdict} fires.
 *
 * A loop over lines or repeats (`:g`, `3@:`) is vouched for once before it
 * starts and does not wait inside: its own edits make new text versions no
 * worker has seen, and stopping half way would leave half the edits made.
 * That is the one place a pattern runs on text the worker has not seen — text
 * that differs from vouched text only by the loop's own edits.
 */

import type { Rope } from "../rope";
import { allMatches } from "./matches";

/** How long a pattern may run in the worker before it counts as too expensive. */
export const VERIFY_TIMEOUT_MS = 1000;
/** Most matches a verdict reports; a search beyond them still works, it just runs again. */
export const MAX_REPORTED = 100_000;

/** One pattern to run on one text. */
export interface VerifyRequest {
  id: number;
  text: string;
  source: string;
  flags: string;
  limit: number;
}

export type VerifyReply =
  | { id: number; ok: true; offsets: Int32Array; count: number; truncated: boolean }
  | { id: number; ok: false; message: string };

/** What the guard knows about a pattern on one version of a text. */
export type Verdict =
  | { ok: true; offsets: Int32Array; count: number; truncated: boolean }
  | { ok: false; reason: "timeout" | "error"; message: string };

/** The part of a `Worker` the guard uses — a fake one in tests. */
export interface WorkerLike {
  postMessage(message: VerifyRequest): void;
  terminate(): void;
  onmessage: ((event: { data: VerifyReply }) => void) | null;
  onerror: ((event: unknown) => void) | null;
}

/** Thrown by {@link SearchGuard.require} while a verdict is on its way: try again once `done` settles. */
export class SearchPending {
  constructor(readonly done: Promise<void>) {}
}

/** The worker's whole job, exported so the worker entry and the tests run the same code. */
export function verify(request: VerifyRequest): VerifyReply {
  try {
    const re = new RegExp(request.source, request.flags);
    const found = allMatches(request.text, re, request.limit);
    return { id: request.id, ok: true, ...found };
  } catch (err) {
    return { id: request.id, ok: false, message: err instanceof Error ? err.message : String(err) };
  }
}

interface Job {
  id: number;
  rope: Rope;
  key: string;
  source: string;
  flags: string;
  done: Promise<void>;
  resolve: () => void;
}

function keyOf(re: RegExp): string {
  return `${re.flags}\u0000${re.source}`;
}

export class SearchGuard {
  private worker: WorkerLike | null = null;
  private readonly verdicts = new WeakMap<Rope, Map<string, Verdict>>();
  private queue: Job[] = [];
  private running: Job | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private nextId = 1;
  private readonly listeners = new Set<() => void>();
  private disposed = false;

  constructor(
    private readonly spawn: () => WorkerLike,
    private readonly timeoutMs = VERIFY_TIMEOUT_MS,
  ) {}

  /** The verdict for `re` on this version of the text, or `null` if none has come yet. */
  verdict(rope: Rope, re: RegExp): Verdict | null {
    return this.verdicts.get(rope)?.get(keyOf(re)) ?? null;
  }

  /** The verdict, or {@link SearchPending} thrown after asking for it. */
  require(rope: Rope, re: RegExp): Verdict {
    const known = this.verdict(rope, re);
    if (known) return known;
    throw new SearchPending(this.request(rope, re));
  }

  /**
   * Asks for a verdict; settles once it is there — or once a newer version
   * of the text took the job's place in the queue, in which case the caller
   * looks again (and finds the text it now has).
   */
  request(rope: Rope, re: RegExp): Promise<void> {
    const key = keyOf(re);
    if (this.verdict(rope, re)) return Promise.resolve();
    const same = [this.running, ...this.queue].find((j) => j && j.rope === rope && j.key === key);
    if (same) return same.done;
    // Waiting jobs for the same pattern on an older text are not worth running any more.
    this.queue = this.queue.filter((j) => {
      if (j.key !== key) return true;
      j.resolve();
      return false;
    });
    let resolve!: () => void;
    const done = new Promise<void>((r) => (resolve = r));
    this.queue.push({ id: this.nextId++, rope, key, source: re.source, flags: re.flags, done, resolve });
    this.pump();
    return done;
  }

  /** Called after every verdict that arrives — to redraw highlights, a count. */
  onVerdict(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  dispose(): void {
    this.disposed = true;
    this.stopTimer();
    this.worker?.terminate();
    this.worker = null;
    for (const job of [this.running, ...this.queue]) job?.resolve();
    this.running = null;
    this.queue = [];
  }

  private pump(): void {
    if (this.running || this.disposed) return;
    const job = this.queue.shift();
    if (!job) return;
    this.running = job;
    const worker = this.ensureWorker();
    this.timer = setTimeout(() => this.onTimeout(job), this.timeoutMs);
    worker.postMessage({
      id: job.id,
      text: job.rope.text(),
      source: job.source,
      flags: job.flags,
      limit: MAX_REPORTED,
    });
  }

  private ensureWorker(): WorkerLike {
    if (this.worker) return this.worker;
    const worker = this.spawn();
    worker.onmessage = (event) => this.onReply(event.data);
    worker.onerror = () => {
      this.dropWorker();
      const job = this.running;
      if (job) this.finish(job, { ok: false, reason: "error", message: "The search worker failed" });
    };
    this.worker = worker;
    return worker;
  }

  private onReply(reply: VerifyReply): void {
    const job = this.running;
    if (!job || reply.id !== job.id) return;
    const verdict: Verdict = reply.ok
      ? { ok: true, offsets: reply.offsets, count: reply.count, truncated: reply.truncated }
      : { ok: false, reason: "error", message: reply.message };
    this.finish(job, verdict);
  }

  private onTimeout(job: Job): void {
    if (this.running !== job) return;
    // Replaced before the next job goes out: the stuck one would never answer it.
    this.dropWorker();
    const seconds = this.timeoutMs / 1000;
    this.finish(job, { ok: false, reason: "timeout", message: `Pattern too expensive: stopped after ${seconds} s` });
  }

  private finish(job: Job, verdict: Verdict): void {
    this.stopTimer();
    let forRope = this.verdicts.get(job.rope);
    if (!forRope) {
      forRope = new Map();
      this.verdicts.set(job.rope, forRope);
    }
    forRope.set(job.key, verdict);
    this.running = null;
    job.resolve();
    for (const listener of this.listeners) listener();
    this.pump();
  }

  /** A worker stuck in a pattern cannot be interrupted, only replaced: the next job starts a fresh one. */
  private dropWorker(): void {
    this.worker?.terminate();
    this.worker = null;
  }

  private stopTimer(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
}
