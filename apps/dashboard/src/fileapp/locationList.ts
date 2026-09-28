/**
 * A language server's list of places (`docs/plans/editor.md`, ED6.3): the uses
 * of a symbol, or its implementations when there is more than one. Shown in
 * the project search's result list (`ProjectSearch.svelte`) — grouped by file,
 * a line of text per place — in the file app's Search column and the IDE's
 * Search pane alike.
 *
 * * **Built from the server's answer** (`buildLocationList`): places are
 *   grouped by file, each file is read once for its lines (through the file
 *   service, or the server's read-only root for a file outside every root).
 * * **Handed to the IDE's Search pane through `pendingLocations`**: the file
 *   pane that asked cannot reach the pane that shows it, so the list waits
 *   here until a Search pane takes it.
 */

import { writable } from "svelte/store";

import type { Location } from "../editor/lsp/client";
import type { LineMatch } from "./backend";

/** Most files read for their lines; the places of further files are listed without text. */
export const MAX_FILES_READ = 200;
/** How much of a long line is shown around the place. */
const WINDOW = 160;

/** One file's places. */
export interface LocatedFile {
  root: string;
  rel: string;
  matches: LineMatch[];
}

export interface LocationList {
  /** What the list is, e.g. "References to `parse`". */
  title: string;
  files: LocatedFile[];
  /** How many places in all. */
  count: number;
}

/** A place as a file of some root; `null` for one that is not a file. */
export type ToFile = (location: Location) => { root: string; rel: string } | null;

/**
 * Groups `locations` by file, in the order the server gave them, and reads
 * each file's lines once (`readText`; a file that cannot be read keeps its
 * places, without text).
 */
export async function buildLocationList(
  title: string,
  locations: Location[],
  toFile: ToFile,
  readText: (root: string, rel: string) => Promise<string>,
): Promise<LocationList> {
  const byFile = new Map<string, { root: string; rel: string; places: Location[] }>();
  for (const location of locations) {
    const file = toFile(location);
    if (!file) continue;
    const key = `${file.root}\0${file.rel}`;
    let entry = byFile.get(key);
    if (!entry) byFile.set(key, (entry = { ...file, places: [] }));
    entry.places.push(location);
  }
  const entries = [...byFile.values()];
  const files = await Promise.all(
    entries.map(async (entry, index): Promise<LocatedFile> => {
      let lines: string[] = [];
      if (index < MAX_FILES_READ) {
        try {
          lines = (await readText(entry.root, entry.rel)).split(/\r?\n/);
        } catch {
          // Listed without text: the place is still worth a click.
        }
      }
      const places = [...entry.places].sort((a, b) => a.at.line - b.at.line || a.at.col - b.at.col);
      return { root: entry.root, rel: entry.rel, matches: places.map((p) => lineMatch(p, lines[p.at.line] ?? "")) };
    }),
  );
  return { title, files, count: files.reduce((n, f) => n + f.matches.length, 0) };
}

/** A place as the result list draws it: its line (a window of a long one), the symbol marked. */
function lineMatch(place: Location, line: string): LineMatch {
  const col = place.at.col;
  const end = place.end && place.end.line === place.at.line ? Math.max(col, place.end.col) : col;
  const from = line.length > WINDOW ? Math.max(0, col - WINDOW / 4) : 0;
  return { line: place.at.line, col, end, text: line.slice(from, from + WINDOW), from };
}

/** A list waiting for the IDE's Search pane. */
export const pendingLocations = writable<LocationList | null>(null);
