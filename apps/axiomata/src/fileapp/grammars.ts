/**
 * Where the app's tree-sitter grammars come from (`docs/plans/editor.md`, G9):
 * the checked-in files under `public/grammars/`, fetched on first use. One
 * runtime for the whole app, so each grammar loads once however many files and
 * previews use it.
 */

import runtimeWasm from "web-tree-sitter/web-tree-sitter.wasm?url";

import { GrammarRuntime, type GrammarSource } from "../editor/syntax/runtime";
import { queryTextOrNull } from "./grammarFetch";

const BASE = `${import.meta.env.BASE_URL}grammars/`;

const browserGrammars: GrammarSource = {
  runtimeWasm: () => runtimeWasm,
  async grammar(name) {
    const response = await fetch(`${BASE}${name}.wasm`);
    if (!response.ok) throw new Error(`grammar ${name}: HTTP ${response.status}`);
    return new Uint8Array(await response.arrayBuffer());
  },
  async query(path) {
    const response = await fetch(`${BASE}${path}`);
    // A missing file may come back as the app's index.html with 200 — by its body, not its Content-Type (see there).
    if (!response.ok) return null;
    return queryTextOrNull(await response.text());
  },
};

export const grammarRuntime = new GrammarRuntime(browserGrammars);
