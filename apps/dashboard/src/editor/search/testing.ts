/**
 * A stand-in for the search worker in tests (`guard.ts`): answers on the next
 * microtask by running the real `verify` — or never, for a pattern `hang` names.
 */

import { verify, type VerifyReply, type VerifyRequest, type WorkerLike } from "./guard";

export class FakeSearchWorker implements WorkerLike {
  onmessage: ((event: { data: VerifyReply }) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  terminated = false;
  readonly seen: VerifyRequest[] = [];

  constructor(private readonly hang: (req: VerifyRequest) => boolean = () => false) {}

  postMessage(message: VerifyRequest): void {
    this.seen.push(message);
    if (this.hang(message)) return;
    queueMicrotask(() => {
      if (!this.terminated) this.onmessage?.({ data: verify(message) });
    });
  }

  terminate(): void {
    this.terminated = true;
  }
}

/** Lets every pending verdict arrive and every paused key run. */
export async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}
