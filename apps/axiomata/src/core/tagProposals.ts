/**
 * Tags waiting for the owner (`docs/plans/tags-links.md` Q6, Q10, TL4): an agent that found no fitting tag on the
 * list proposed one. The count is the waiting badge in the Dock (beside the settings) and on the Workspace tab; the
 * list itself is answered in Settings → Workspace → Tags.
 *
 * Proposals are made by agents in other processes (a skill run's `axiomata-cli notes tag`, the note placement), so
 * there is no event to listen to: the count is asked for again every {@link POLL_MS} and right after the owner
 * answered one here.
 */

import { writable, type Writable } from "svelte/store";

import { tTags } from "../i18n/tags";
import type { TagEntry, TagProposal } from "./backend";

/** How often the count is asked for again: one small query, often enough that a finished run shows soon after. */
export const POLL_MS = 60_000;

/** The proposals waiting, as last read. */
export const tagProposals: Writable<TagProposal[]> = writable([]);

/** What the badge says. */
export function waitingLabel(count: number): string {
  return tTags("waiting", { n: count });
}

/** Reads the proposals once into {@link tagProposals}; a failed read leaves the last list. */
export async function refreshTagProposals(
  fetchNow: () => Promise<TagProposal[]>,
  store: Writable<TagProposal[]> = tagProposals,
): Promise<void> {
  try {
    store.set(await fetchNow());
  } catch {
    // The next poll brings the list.
  }
}

/** Keeps {@link tagProposals} fresh: a read now, then one every `intervalMs`. Returns the function that stops it. */
export function watchTagProposals(
  fetchNow: () => Promise<TagProposal[]>,
  intervalMs = POLL_MS,
  store: Writable<TagProposal[]> = tagProposals,
): () => void {
  void refreshTagProposals(fetchNow, store);
  const timer = setInterval(() => void refreshTagProposals(fetchNow, store), intervalMs);
  return () => clearInterval(timer);
}

/** What is wrong with a tag the owner typed, or `null` when it is one (the rules of `tags::detect::normalise`). */
export function tagProblem(raw: string, list: readonly TagEntry[], except?: string): string | null {
  const tag = normaliseTag(raw);
  if (tag === "") return tTags("problemEmpty");
  if (tag.length > 64) return tTags("problemLength");
  if (!/^[\p{L}\p{N}_\-/]+$/u.test(tag)) return tTags("problemChars");
  if (/^[\d/]+$/.test(tag)) return tTags("problemDigits");
  if (!/^[\p{L}\p{N}]/u.test(tag)) return tTags("problemStart");
  if (tag.startsWith("/") || tag.endsWith("/") || tag.includes("//")) return tTags("problemSlash");
  if (tag !== except && list.some((entry) => entry.tag === tag)) return tTags("problemTaken", { tag });
  return null;
}

/** `raw` as the list stores a tag: trimmed, one leading `#` dropped, lower case. */
export function normaliseTag(raw: string): string {
  return raw.trim().replace(/^#/, "").toLowerCase();
}
