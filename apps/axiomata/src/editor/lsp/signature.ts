/**
 * Signature help (`docs/plans/editor.md`, ED6.6): while a call's arguments
 * are typed, the called function's signature with the current parameter
 * marked — as blink.cmp shows it in the owner's Neovim (`signature =
 * { enabled = true }`). Part of the engine: no DOM, no app imports.
 */

/** What to show: the signature split around the active parameter, and what is said about it. */
export interface SignatureView {
  before: string;
  active: string;
  after: string;
  /** The active parameter's documentation, else the signature's, as Markdown. */
  documentation: string | null;
  /** "2/3" when the server offers several overloads. */
  overloads: string | null;
}

interface RawParameter {
  label?: unknown;
  documentation?: unknown;
}

interface RawSignature {
  label?: unknown;
  documentation?: unknown;
  parameters?: RawParameter[];
  activeParameter?: unknown;
}

function docText(raw: unknown): string | null {
  if (typeof raw === "string") return raw.trim() === "" ? null : raw;
  if (typeof raw === "object" && raw !== null) {
    const d = raw as { kind?: unknown; value?: unknown };
    if (typeof d.value !== "string" || d.value.trim() === "") return null;
    return d.kind === "plaintext" ? d.value.replace(/[\\`*_{}[\]()#+\-.!<>]/g, "\\$&") : d.value;
  }
  return null;
}

/** Reads a `SignatureHelp` answer; `null` when there is nothing to show. */
export function parseSignatureHelp(raw: unknown): SignatureView | null {
  const help = raw as { signatures?: unknown; activeSignature?: unknown; activeParameter?: unknown } | null;
  if (!help || !Array.isArray(help.signatures) || help.signatures.length === 0) return null;
  const signatures = help.signatures as RawSignature[];
  const index = typeof help.activeSignature === "number" ? help.activeSignature : 0;
  const which = index >= 0 && index < signatures.length ? index : 0;
  const sig = signatures[which];
  if (typeof sig?.label !== "string") return null;
  const label = sig.label;
  // The signature's own active parameter wins over the answer's (LSP 3.16).
  const activeRaw = typeof sig.activeParameter === "number" ? sig.activeParameter : help.activeParameter;
  const params = Array.isArray(sig.parameters) ? sig.parameters : [];
  const param = typeof activeRaw === "number" ? params[activeRaw] : undefined;
  let span: [number, number] | null = null;
  if (Array.isArray(param?.label) && param.label.length === 2) {
    const [from, to] = param.label as unknown[];
    if (typeof from === "number" && typeof to === "number" && from >= 0 && to <= label.length && from <= to) {
      span = [from, to];
    }
  } else if (typeof param?.label === "string" && param.label !== "") {
    // A string label is found in the signature, after its name (the `(`).
    const start = label.indexOf(param.label, Math.max(0, label.indexOf("(")));
    if (start >= 0) span = [start, start + param.label.length];
  }
  const [from, to] = span ?? [label.length, label.length];
  return {
    before: label.slice(0, from),
    active: label.slice(from, to),
    after: label.slice(to),
    documentation: docText(param?.documentation) ?? docText(sig.documentation),
    overloads: signatures.length > 1 ? `${which + 1}/${signatures.length}` : null,
  };
}
