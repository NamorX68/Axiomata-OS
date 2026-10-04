/**
 * A backend error as a sentence. Tauri commands reject with a string, the file
 * service with `{ kind, message }`, a failed call with an `Error` — the views
 * show all of them the same way.
 */
export function messageOf(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return (err as { message?: string } | null)?.message ?? String(err);
}
