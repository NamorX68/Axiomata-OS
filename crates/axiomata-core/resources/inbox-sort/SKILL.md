---
name: inbox-sort
description: Sorts the notes in the workspace's Inbox/ into the area they belong to, using the areas' AGENTS.md routers; leaves anything unclear in the Inbox and asks about it.
backend: opencode
timeout_secs: 900
---

# Inbox sort

You sort the owner's vault inbox. Your working directory is the vault.

1. Read `AGENTS.md` at the root: it lists the areas (top-level folders) and links each area's own
   `AGENTS.md`, which lists what is already in it. Read the area routers you need.
2. List the files in `Inbox/` (ignore `Inbox/AGENTS.md` and `Inbox/CLAUDE.md` — they are generated).
3. For each file, read it and decide which area it belongs to — and, if the area has subfolders that
   fit, which subfolder. Decide only when it is **clear**: the topic plainly matches one area. A file that
   could go to two areas, fits none, or whose content you cannot read, stays where it is.
4. Move each clear file with `mv "Inbox/<file>" "<Area>/<optional subfolder>/<file>"`. Never overwrite:
   if a file of that name exists there, leave the file in the Inbox and ask about it. Never rename, edit
   or delete a file, and never create a new area.
5. Do not touch the `AGENTS.md` routers; the app regenerates them.

Finish with a short report in German, exactly these two parts:

**Einsortiert** — one line per moved file: `Inbox/<file> → <target>` and a few words why.

**Fragen** — one line per file left in the Inbox: the file, the areas it could belong to, and one
question the owner can answer with a word (e.g. "Arbeit oder Learning?"). Write "Keine" if there are none.
