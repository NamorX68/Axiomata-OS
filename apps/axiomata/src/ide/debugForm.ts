/** The Debug view's form for a configuration of your own: what you type, and what is saved. */

import type { DebugConfigInfo } from "./debugBackend";

export interface NewDebugConfig {
  name: string;
  program: string | null;
  module: string | null;
  args: string[];
  cwd: string | null;
}

export interface DebugForm {
  name: string;
  /** A file (`src/app/main.py`) or a module (`ocht`, `pkg.cli`). */
  target: string;
  /** Arguments as one line; quotes group words. */
  args: string;
  cwd: string;
}

export const EMPTY_FORM: DebugForm = { name: "", target: "", args: "", cwd: "" };

/** `a "b c" d` → `["a", "b c", "d"]`. No escapes, no expansion: what is typed is what the program gets. */
export function splitArgs(line: string): string[] {
  const out: string[] = [];
  const pattern = /"([^"]*)"|'([^']*)'|(\S+)/g;
  for (let m = pattern.exec(line); m; m = pattern.exec(line)) out.push(m[1] ?? m[2] ?? m[3]);
  return out;
}

/** A target that names a file ends in `.py` or has a slash; anything else is a module. */
export function isFileTarget(target: string): boolean {
  return /\.py$/i.test(target) || target.includes("/");
}

export function toNewConfig(form: DebugForm): NewDebugConfig {
  const target = form.target.trim();
  const file = isFileTarget(target);
  return {
    name: form.name.trim(),
    program: file ? target : null,
    module: file ? null : target,
    args: splitArgs(form.args),
    cwd: form.cwd.trim() || null,
  };
}

const quoted = (arg: string): string => (/\s/.test(arg) ? `"${arg}"` : arg);

export function formOf(config: DebugConfigInfo): DebugForm {
  return {
    name: config.name,
    target: config.program ?? config.module ?? "",
    args: config.args.map(quoted).join(" "),
    cwd: config.cwd ?? "",
  };
}
