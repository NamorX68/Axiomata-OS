/**
 * The one run over the vault (`docs/plans/tags-links.md` Q9, TL6) as Settings → Workspace → Tags words it: what a
 * step reads and is expected to cost, said before the owner starts it.
 */

import type { VaultEstimate } from "./backend";
import { formatBytes } from "./format";

/** Tokens as people read them: `1.2 M`, `38 k`, `950`. */
export function tokenCount(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)} M`;
  if (n >= 1_000) return `${Math.round(n / 1_000)} k`;
  return String(n);
}

/** What a step reads and costs, on one line: "252 notes (1.1 MB) · about 1.2 M tokens in, 38 k out · about $0.36". */
export function estimateLine(estimate: VaultEstimate): string {
  const notes = estimate.notes === 1 ? "1 note" : `${estimate.notes} notes`;
  const tokens = `about ${tokenCount(estimate.input_tokens)} tokens in, ${tokenCount(estimate.output_tokens)} out`;
  const cost =
    estimate.cost_usd === null
      ? `no price for ${estimate.model ?? "the skill model"} in your price table`
      : `about $${estimate.cost_usd < 0.01 ? estimate.cost_usd.toFixed(4) : estimate.cost_usd.toFixed(2)}`;
  return `${notes} (${formatBytes(estimate.bytes)}) · ${tokens} · ${cost}`;
}
