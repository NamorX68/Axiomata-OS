/**
 * The Vi state every editor surface shares (`docs/plans/editor.md`, ED3, V3,
 * V4): one set of registers, the last `f`/`t`, the file marks and the last
 * macro — `yy` in the file app and `p` in an IDE pane meet in the same `"`.
 *
 * The unnamed register is the Mac clipboard while the setting says "shared"
 * (the default, D17); reading and writing it go through Rust (`pbpaste`/
 * `pbcopy`), so a read answers later and the machine waits for it.
 *
 * What lasts across restarts (named registers, file marks, histories, V4) is
 * read from `~/.axiomata/editor-vi.json` when the state is first made, and
 * written back a moment after the keys settle (`viStateChanged`).
 */

import { invokeBackend } from "../core/backend";
import { toast } from "../core/toast";
import type { ClipboardPort } from "../editor/vi/registers";
import { ViShared } from "../editor/vi/machine";
import { editorSettings } from "./editorSettings";
import { restoreVi, snapshotVi } from "./viPersist";

/** How long after the last key the remembered state is written. */
const SAVE_DELAY_MS = 1000;

const macClipboard: ClipboardPort = {
  read: () => invokeBackend<string>("clipboard_read").catch(() => ""),
  write: (text) => {
    void invokeBackend<void>("clipboard_write", { text }).catch(() => undefined);
  },
};

let shared: ViShared | null = null;
/** Nothing is written before the file was read — an early save would wipe what it held. */
let restored = false;
let saveTimer: ReturnType<typeof setTimeout> | null = null;

/** The one `ViShared` of the app, created on first use and kept in step with the clipboard setting. */
export function viShared(): ViShared {
  if (shared) return shared;
  const created = new ViShared(macClipboard);
  editorSettings.subscribe((s) => {
    created.registers.shared = s.viClipboard === "shared";
  });
  shared = created;
  void invokeBackend<{ json: string }>("get_editor_vi_state")
    .then(({ json }) => restoreVi(created, JSON.parse(json)))
    .catch(() => undefined)
    .finally(() => {
      restored = true;
    });
  return created;
}

/** Something Vi remembers may have changed: written to `editor-vi.json` once the keys settle. */
export function viStateChanged(): void {
  if (!shared) return;
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(saveNow, SAVE_DELAY_MS);
}

function saveNow(): void {
  saveTimer = null;
  if (!shared) return;
  // The file is still being read: try again later rather than drop the change or overwrite what it holds.
  if (!restored) {
    saveTimer = setTimeout(saveNow, SAVE_DELAY_MS);
    return;
  }
  const json = JSON.stringify(snapshotVi(shared));
  void invokeBackend<void>("save_editor_vi_state", { json }).catch((err) =>
    toast(`Could not save editor-vi.json: ${String(err)}`, "danger"),
  );
}
