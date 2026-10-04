/**
 * Where the app's tree-sitter grammars come from (`docs/plans/editor.md`, G9):
 * the checked-in files under `public/grammars/`, fetched on first use. One
 * runtime for the whole app, so each grammar loads once however many files and
 * previews use it.
 */

import runtimeWasm from "web-tree-sitter/web-tree-sitter.wasm?url";

import { GrammarRuntime, type GrammarSource } from "../editor/syntax/runtime";

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
    // The dev server answers a missing file with the app's index.html and 200,
    // which must not be mistaken for a query (an absent *.local.scm, say).
    if (!response.ok || (response.headers.get("content-type") ?? "").includes("text/html")) return null;
    return response.text();
  },
};

export const grammarRuntime = new GrammarRuntime(browserGrammars);
