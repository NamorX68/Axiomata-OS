/**
 * Loading tree-sitter and its grammars (`docs/plans/editor.md`, G1, G4, G9).
 *
 * Nothing loads until a file of that language is opened: the runtime, each
 * grammar and its queries are fetched on first use and cached. Where the files
 * come from is the embedder's business (`GrammarSource`) — the app fetches
 * them from `public/grammars/`, the tests read them from disk — so this module
 * stays free of the DOM and of the app.
 *
 * A query file that does not fit its grammar (a node type the grammar lacks)
 * would make the whole query fail to compile; such a file is skipped with a
 * warning and the rest still colours the text.
 */

import { Language, Parser, Query } from "web-tree-sitter";

import { languageSpec, type LanguageSpec } from "./languages";

export interface GrammarSource {
  /** Where `web-tree-sitter.wasm` itself is (a URL or, in tests, a path). */
  runtimeWasm(): string;
  /** A grammar's WebAssembly, by grammar name. */
  grammar(name: string): Promise<Uint8Array>;
  /** A query file's text, by its path under the grammars directory; `null` if absent. */
  query(path: string): Promise<string | null>;
}

export interface LoadedLanguage {
  spec: LanguageSpec;
  language: Language;
  highlights: Query | null;
  injections: Query | null;
}

export class GrammarRuntime {
  private init: Promise<void> | null = null;
  private languages = new Map<string, Promise<LoadedLanguage | null>>();
  private ready = new Map<string, LoadedLanguage | null>();

  constructor(private readonly source: GrammarSource) {}

  /** Loads (once) and returns the language `id`; `null` if it cannot be loaded. */
  load(id: string): Promise<LoadedLanguage | null> {
    let pending = this.languages.get(id);
    if (!pending) {
      pending = this.loadNow(id).then((loaded) => {
        this.ready.set(id, loaded);
        return loaded;
      });
      this.languages.set(id, pending);
    }
    return pending;
  }

  /**
   * The language `id` if it has already loaded, else `undefined` — and a load
   * is started, so a later call (after `onLoaded`) finds it.
   */
  peek(id: string): LoadedLanguage | null | undefined {
    if (this.ready.has(id)) return this.ready.get(id);
    void this.load(id);
    return undefined;
  }

  /** A fresh parser (each highlighter owns one; they are not shared). */
  async parser(): Promise<Parser> {
    await this.initialise();
    return new Parser();
  }

  private initialise(): Promise<void> {
    this.init ??= Parser.init({ locateFile: () => this.source.runtimeWasm() });
    return this.init;
  }

  private async loadNow(id: string): Promise<LoadedLanguage | null> {
    const spec = languageSpec(id);
    if (!spec) return null;
    try {
      await this.initialise();
      const language = await Language.load(await this.source.grammar(spec.grammar));
      const highlightFiles = [...spec.highlights, `${spec.grammar}/highlights.local.scm`];
      return {
        spec,
        language,
        highlights: await this.compile(language, highlightFiles),
        injections: spec.injections ? await this.compile(language, [spec.injections]) : null,
      };
    } catch (err) {
      console.warn(`editor: could not load the ${id} grammar`, err);
      return null;
    }
  }

  /** Joins the query files that compile against `language`; `null` if none do. */
  private async compile(language: Language, files: string[]): Promise<Query | null> {
    const parts: string[] = [];
    for (const file of files) {
      const text = await this.source.query(file);
      if (!text) continue;
      try {
        new Query(language, text).delete();
        parts.push(text);
      } catch (err) {
        console.warn(`editor: skipping query ${file}`, err);
      }
    }
    return parts.length > 0 ? new Query(language, parts.join("\n")) : null;
  }
}
