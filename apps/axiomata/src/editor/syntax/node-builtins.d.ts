// The two Node built-ins `highlighter.test.ts` reads the checked-in grammars
// with. Declared here, narrowly, rather than installing @types/node, which would
// put Node's typings (a different `setTimeout`, `process`, …) over the whole
// browser frontend.
declare module "node:fs/promises" {
  export function readFile(path: string): Promise<Uint8Array>;
  export function readFile(path: string, encoding: "utf8"): Promise<string>;
}

declare module "node:url" {
  export function fileURLToPath(url: URL): string;
}
