/**
 * The search worker's entry (`docs/plans/editor.md`, ED5, T5): runs one
 * pattern on one text and answers with every match. All it does is
 * `guard.ts`'s {@link verify}; being in a worker is the point — the guard can
 * kill it when a pattern takes too long.
 */

import { verify, type VerifyRequest } from "./guard";

self.onmessage = (event: MessageEvent<VerifyRequest>) => {
  const reply = verify(event.data);
  if (reply.ok) self.postMessage(reply, { transfer: [reply.offsets.buffer] });
  else self.postMessage(reply);
};
