/**
 * A path printed in a terminal, as a path inside a folder (a project, an agent's worktree) — or `null`.
 *
 * The text comes from whatever runs in the shell, so it is never trusted to say where a file is: an absolute
 * path counts only under `base`, a relative one is resolved against `base` and must stay inside it. The file
 * itself is then read through the file service like any other (`{ root, rel }`), which checks again.
 */

export function relativeInside(printed: string, base: string): string | null {
  const path = printed.replace(/\\/g, "/");
  const root = base.replace(/\\/g, "/").replace(/\/+$/, "");
  let rest: string;
  if (path.startsWith("/") || /^[A-Za-z]:\//.test(path)) {
    if (!root || !path.startsWith(`${root}/`)) return null;
    rest = path.slice(root.length + 1);
  } else if (path.startsWith("~")) {
    return null;
  } else {
    rest = path;
  }
  const parts: string[] = [];
  for (const part of rest.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (parts.length === 0) return null; // out of the folder
      parts.pop();
    } else {
      parts.push(part);
    }
  }
  return parts.length > 0 ? parts.join("/") : null;
}

/**
 * A debugger's file path as a path inside `base` — **absolute paths only**. A debug adapter reports files it
 * cannot place (a standard-library source remapped to `library/core/src/…`, a generated file) with a relative
 * path; resolving that against the project would name a file that is not there, so it counts as outside.
 */
export function absoluteInside(path: string | null, base: string): string | null {
  if (!path) return null;
  const printed = path.replace(/\\/g, "/");
  if (!printed.startsWith("/") && !/^[A-Za-z]:\//.test(printed)) return null;
  return relativeInside(path, base);
}
