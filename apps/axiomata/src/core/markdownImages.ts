/**
 * Resolves relative image references in Markdown source to inline `data:`
 * URIs before rendering. Kept separate from `core/markdown.ts`'s
 * `renderMarkdown` (which stays synchronous, shared with the chat panel —
 * chat replies don't reference workspace-relative image paths) so only the
 * Markdown viewer pays for the async resolution step.
 */

import type { WorkspaceImage } from "./backend";
import { resolveRelativeLink } from "./htmllink";
import type { Invoke } from "./skillRun";

/** Matches `![alt](target)` — deliberately not `[alt](target "title")`'s
 *  optional title text, same simplicity as the Rust graph walker's own
 *  link-target regexes (`link_targets` in `graph.rs`). */
const IMAGE_RE = /!\[([^\]]*)\]\(([^)\s]+)\)/g;

function isAlreadyResolvable(target: string): boolean {
  return /^(?:https?:|data:)/i.test(target);
}

/**
 * Rewrites every `![alt](relative/path.jpg)` in `source` whose target isn't
 * already an `http(s):`/`data:` URI into `![alt](data:<mime>;base64,...)`,
 * resolved relative to `notePath`'s own folder (the same
 * `core/htmllink.ts` `resolveRelativeLink` the HTML preview already uses for
 * HTML same-folder links). A target that fails to resolve (missing file,
 * unsupported type, over the size cap) is left exactly as written — the
 * same "broken image" outcome as before this existed, not a
 * render-blocking error.
 *
 * Every distinct target is fetched at most once (even if the same image is
 * referenced twice in one note) and all fetches run in parallel.
 */
export async function resolveMarkdownImages(source: string, notePath: string, invoke: Invoke): Promise<string> {
  return resolveMarkdownImagesWith(source, notePath, (rel) => invoke<WorkspaceImage>("read_workspace_image", { rel }));
}

/**
 * `resolveMarkdownImages` with the image reader passed in — the file app reads
 * through the file service with the note's own root (`file_read_image`), so a
 * note in a project folder finds its images too, not only one in the
 * workspace.
 */
export async function resolveMarkdownImagesWith(
  source: string,
  notePath: string,
  read: (rel: string) => Promise<{ mime: string; base64: string }>,
): Promise<string> {
  const targets = new Set<string>();
  for (const m of source.matchAll(IMAGE_RE)) {
    const target = m[2];
    if (!isAlreadyResolvable(target)) targets.add(target);
  }
  if (targets.size === 0) return source;

  const dataUris = new Map<string, string>();
  await Promise.all(
    [...targets].map(async (target) => {
      const rel = resolveRelativeLink(notePath, target);
      if (rel === null) return;
      try {
        const img = await read(rel);
        dataUris.set(target, `data:${img.mime};base64,${img.base64}`);
      } catch {
        // Left unresolved on purpose — see the doc comment above.
      }
    }),
  );
  if (dataUris.size === 0) return source;

  return source.replace(IMAGE_RE, (full, alt: string, target: string) => {
    const dataUri = dataUris.get(target);
    return dataUri ? `![${alt}](${dataUri})` : full;
  });
}
