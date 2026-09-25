/**
 * The Vi state every editor surface shares (`docs/plans/editor.md`, ED3, V3,
 * V4): one set of registers, the last `f`/`t`, the file marks and the last
 * macro — `yy` in the file app and `p` in an IDE pane meet in the same `"`.
 *
 * The unnamed register is the Mac clipboard while the setting says "shared"
 * (the default, D17); reading and writing it go through Rust (`pbpaste`/
 * `pbcopy`), so a read answers later and the machine waits for it.
 */

import { invokeBackend } from "../core/backend";
import type { ClipboardPort } from "../editor/vi/registers";
import { ViShared } from "../editor/vi/machine";
import { editorSettings } from "./editorSettings";

const macClipboard: ClipboardPort = {
  read: () => invokeBackend<string>("clipboard_read").catch(() => ""),
  write: (text) => {
    void invokeBackend<void>("clipboard_write", { text }).catch(() => undefined);
  },
};

let shared: ViShared | null = null;

/** The one `ViShared` of the app, created on first use and kept in step with the clipboard setting. */
export function viShared(): ViShared {
  if (shared) return shared;
  const created = new ViShared(macClipboard);
  editorSettings.subscribe((s) => {
    created.registers.shared = s.viClipboard === "shared";
  });
  shared = created;
  return created;
}
