# Creating and filing notes, and the Inbox

How to write a new note, how the agent files it in the right area of the vault when you save it, and what happens with the Inbox.

## What it is

The **vault** is the folder with your Markdown notes (path under Settings → Workspace → “Vault”). Every top-level
folder is an **area**; in it and in the vault itself lies an `AGENTS.md` with a generated map of the files (the
“router”), so that agents find their way. The **“Memory”** tile shows whether these maps are current (“fresh” /
“stale”), and “Sync” writes them anew.

You write a **new note** without choosing a folder: on the first save an agent turn looks for the right area, a file
name, tags from the tag list and related notes. If no existing area fits, it may propose a new one; only if nothing at
all fits does the note land in `Inbox/`. The **Inbox** is the place for what is not sorted yet; the skill `inbox-sort`
tidies it up.

## Where the buttons are

- **New note:** dock “Note”, ⌘K “New note” or ⌘N in the Studio. The floating file window “New note” opens for
  writing; at the bottom it says “New note — ⌘S files it”.
- **File it:** ⌘S. While the agent works: “Filing the note…”; then the note opens for reading at its new place.
- **Close without filing:** the question “This note is not filed yet.” with “File it” / “Discard” / “Cancel”.
- **Sort the Inbox:** in the Skills Deck `inbox-sort` ▶ (or as a routine).
- **Empty file at a fixed place:** `/newfile <path>` in the chat.

## Common tasks

1. **Jot something down quickly:** dock “Note” → write → ⌘S. The agent files it.
2. **Empty the Inbox:** start `inbox-sort`; it moves only clear cases, gives them tags and up to three related links
   and lists under “Fragen” what it could not place.
3. **Refresh the router:** in the “Memory” tile click “Sync” when it shows “stale”.

## Traps

- A new note is **never** saved automatically; only ⌘S files it. Filing is an agent turn on the chat's harness and
  provider and counts against the daily limit.
- The agent never overwrites an existing file. If the vault path is wrong, nothing lands where you look.
- `inbox-sort` creates **no new areas** and moves only into existing folders; whatever is unclear stays in the Inbox.
- Your own notes do not belong in the `Kanban/` folder (the board's mirror) — the app overwrites it.
- The router between the `AXIOMATA-ROUTER` markers in `AGENTS.md` is generated; your own text belongs outside them.

## See also

[Editor](editor.md) · [Tags and links](tags-links.md) · [Second Brain](second-brain.md) · [Skills](skills.md)
