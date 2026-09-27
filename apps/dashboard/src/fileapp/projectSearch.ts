/**
 * The project search's state (`docs/plans/editor.md`, ED5, T13, T14), apart
 * from how it is drawn (`ProjectSearch.svelte`): one query at a time, its
 * files arriving while it runs, and what to say about it.
 *
 * * **A newer query wins**: every run has a generation, and batches or a
 *   summary of an older one are dropped — the backend stops the older one
 *   too (same `owner`), but a batch already on its way may still arrive.
 * * **The backend is injected**, so the rules are tested without Tauri.
 */

import type { FileMatches, SearchQuery, SearchSummary } from "./backend";

/** The options beside the query field. Globs are typed comma-separated. */
export interface SearchOptions {
  regex: boolean;
  caseSensitive: boolean;
  wholeWord: boolean;
  include: string;
  exclude: string;
}

export const DEFAULT_OPTIONS: SearchOptions = {
  regex: false,
  caseSensitive: false,
  wholeWord: false,
  include: "",
  exclude: "",
};

export type SearchStatus = "idle" | "running" | "done" | "error";

export interface SearchView {
  status: SearchStatus;
  /** Files with matches, in the order they were found. */
  files: FileMatches[];
  matches: number;
  /** Stopped at the match limit (or the walk's own limits). */
  truncated: boolean;
  error: string | null;
}

export interface SearchPort {
  search(
    root: string,
    query: SearchQuery,
    owner: string,
    onFiles: (files: FileMatches[]) => void,
  ): Promise<SearchSummary>;
  cancel(owner: string): Promise<void>;
}

const IDLE: SearchView = { status: "idle", files: [], matches: 0, truncated: false, error: null };

/** `"*.rs, src/**"` → `["*.rs", "src/**"]`. */
export function parseGlobs(text: string): string[] {
  return text
    .split(",")
    .map((g) => g.trim())
    .filter((g) => g !== "");
}

/** The query the backend takes, from the field's text and the options. */
export function toQuery(pattern: string, options: SearchOptions): SearchQuery {
  return {
    pattern,
    regex: options.regex,
    caseSensitive: options.caseSensitive,
    wholeWord: options.wholeWord,
    include: parseGlobs(options.include),
    exclude: parseGlobs(options.exclude),
  };
}

/** A failed search as a sentence: a bad pattern (or glob) says what is wrong with it. */
function errorText(err: unknown): string {
  const e = err as { kind?: string; message?: string } | null;
  if (e?.kind !== "BadPattern") return e?.message ?? String(err);
  const detail = (e.message ?? "").replace(/^bad search pattern: /, "");
  return detail.startsWith("include/exclude") ? `Invalid ${detail}` : `Invalid pattern: ${detail}`;
}

export class ProjectSearchModel {
  view: SearchView = IDLE;
  private generation = 0;

  constructor(
    private readonly port: SearchPort,
    private readonly owner: string,
    private readonly changed: () => void,
  ) {}

  /** Runs `pattern` over `root`; an empty pattern just clears the results. */
  async start(root: string, pattern: string, options: SearchOptions): Promise<void> {
    const mine = ++this.generation;
    if (pattern === "") {
      this.view = IDLE;
      this.changed();
      await this.port.cancel(this.owner).catch(() => undefined);
      return;
    }
    this.view = { ...IDLE, status: "running" };
    this.changed();
    try {
      const summary = await this.port.search(root, toQuery(pattern, options), this.owner, (files) => {
        if (mine !== this.generation) return;
        const matches = this.view.matches + files.reduce((n, f) => n + f.matches.length, 0);
        this.view = { ...this.view, files: [...this.view.files, ...files], matches };
        this.changed();
      });
      if (mine !== this.generation) return;
      this.view = { ...this.view, status: "done", truncated: summary.truncated };
    } catch (err) {
      if (mine !== this.generation) return;
      this.view = { ...IDLE, status: "error", error: errorText(err) };
    }
    this.changed();
  }

  /** Stops whatever runs (the view goes away). */
  stop(): void {
    this.generation++;
    void this.port.cancel(this.owner).catch(() => undefined);
  }
}

/** What the status line says. */
export function statusText(view: SearchView): string {
  if (view.status === "error") return view.error ?? "The search failed.";
  if (view.status === "idle") return "";
  const n = view.matches;
  const files = view.files.length;
  const found =
    n === 0
      ? "No results"
      : `${n.toLocaleString()} ${n === 1 ? "result" : "results"} in ${files} ${files === 1 ? "file" : "files"}`;
  if (view.status === "running") return n === 0 ? "Searching…" : `${found} — searching…`;
  return view.truncated ? `${found} — stopped at 10,000 results; narrow the search` : found;
}
