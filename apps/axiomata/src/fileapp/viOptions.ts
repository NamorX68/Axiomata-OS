/**
 * Vi's `:set` for one open editor (`docs/plans/editor.md`, ED3.3, V6): the view
 * options a `:set` changes — for this editor only, never written to the
 * editor settings, as in Vim (the gear panel is where settings are kept).
 *
 * `number` and `relativenumber` are two switches over one gutter: both on is
 * hybrid, one of them is absolute or relative, neither hides the numbers.
 */

import type { GutterMode } from "../editor/gutter";
import type { SetOption } from "../editor/vi/ex";

export interface ViOptions {
  wrap: boolean;
  lineNumbers: GutterMode;
  list: boolean;
}

function flip(current: boolean, value: boolean | "toggle"): boolean {
  return value === "toggle" ? !current : value;
}

/** `options` after `:set {option}` (`value` true), `:set no{option}` (false) or `:set {option}!`. */
export function applySet(options: ViOptions, option: SetOption, value: boolean | "toggle"): ViOptions {
  if (option === "wrap") return { ...options, wrap: flip(options.wrap, value) };
  if (option === "list") return { ...options, list: flip(options.list, value) };
  const mode = options.lineNumbers;
  let number = mode === "absolute" || mode === "hybrid";
  let relative = mode === "relative" || mode === "hybrid";
  if (option === "number") number = flip(number, value);
  else relative = flip(relative, value);
  const lineNumbers: GutterMode = number && relative ? "hybrid" : number ? "absolute" : relative ? "relative" : "off";
  return { ...options, lineNumbers };
}
