/**
 * The one run over the vault (`docs/plans/tags-links.md` Q9, TL6) as Settings → Workspace → Tags words it: what a
 * step reads and is expected to cost, said before the owner starts it.
 */

import { tTags } from "../i18n/tags";
import type { VaultEstimate } from "./backend";
import { formatBytes, formatNumber, formatUsd } from "./format";

/** Tokens as people read them: `1.2 M`, `38 k`, `950` — the number in the app's language. */
export function tokenCount(n: number): string {
  if (n >= 1_000_000) {
    return `${formatNumber(n / 1_000_000, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} M`;
  }
  if (n >= 1_000) return `${formatNumber(Math.round(n / 1_000))} k`;
  return String(n);
}

/** What a step reads and costs, on one line: "252 notes (1.1 MB) · about 1.2 M tokens in, 38 k out · about $0.36". */
export function estimateLine(estimate: VaultEstimate): string {
  const notes = tTags("notes", { n: estimate.notes });
  const tokens = tTags("estimateTokens", {
    input: tokenCount(estimate.input_tokens),
    output: tokenCount(estimate.output_tokens),
  });
  // Below a cent the price keeps four places, so a cheap step does not read as free.
  const places = estimate.cost_usd !== null && estimate.cost_usd < 0.01 ? 4 : 2;
  const cost =
    estimate.cost_usd === null
      ? estimate.model
        ? tTags("estimateNoPrice", { model: estimate.model })
        : tTags("estimateNoPriceSkill")
      : tTags("estimateCost", {
          cost: formatUsd(estimate.cost_usd, places),
        });
  return `${notes} (${formatBytes(estimate.bytes)}) · ${tokens} · ${cost}`;
}
