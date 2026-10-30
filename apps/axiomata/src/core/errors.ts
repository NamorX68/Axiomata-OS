import { knownCode, tErrors } from "../i18n/errors";

/**
 * A backend error as a sentence. Tauri commands reject with a string, the file
 * service with `{ kind, message }`, a failed call with an `Error` — the views
 * show all of them the same way.
 */
export function messageOf(err: unknown): string {
  if (typeof err === "string") return fromCore(err);
  if (err instanceof Error) return fromCore(err.message);
  const message = (err as { message?: string } | null)?.message;
  return message === undefined ? String(err) : fromCore(message);
}

/** Opens and separates the parts of a coded refusal (`axiomata_core::error::UI_CODE_MARK`, the unit separator). */
const MARK = "\u001f";

/**
 * A core error's sentence: a coded refusal (`␟code␟{"name":"value"}␟English`) in the owner's language when the code
 * is known (`i18n/errors.ts`), else its English sentence; anything else as it came.
 */
function fromCore(text: string): string {
  if (!text.startsWith(MARK)) return text;
  const [, code = "", json = "{}", ...rest] = text.split(MARK);
  const english = rest.join(MARK);
  if (!knownCode(code)) return english;
  let params: Record<string, string> = {};
  try {
    const parsed: unknown = JSON.parse(json);
    if (parsed && typeof parsed === "object") params = parsed as Record<string, string>;
  } catch {
    return english;
  }
  return tErrors(code, params);
}

/**
 * A backend error as the English sentence Rust wrote, the code taken off — for text a model reads (the chat's answer
 * to a module action): what a model reads stays English (`docs/plans/i18n.md`, Q1).
 */
export function englishOf(err: unknown): string {
  const text = typeof err === "string" ? err : err instanceof Error ? err.message : messageOf(err);
  if (!text.startsWith(MARK)) return text;
  return text.split(MARK).slice(3).join(MARK);
}
