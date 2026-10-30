# Tags and wikilinks

How to connect notes with `#tags` and `[[Wikilinks]]`, how the tag list in TAGS.md is kept and how the one-time vault run proposes tags and links for every note.

## What it is

Axiomata writes tags and links **like Obsidian**, so that the vault stays readable without the app:

- **Tags:** `#tag` in the text or `tags:` in the frontmatter, nested as `#rust/async`. A heading, code, a link/URL or
  a bare number (`#1`) is never a tag. Upper/lower case does not count.
- **Wikilinks:** `[[Note]]`, `[[Note|Display text]]`, `[[Note#Heading]]`.
- **The tag list** is in `TAGS.md` at the root of the vault, one line `- #tag — Description` per tag. The app writes
  it; agents choose tags **only from it** and propose a new tag when none fits. Goal: about 30–60 tags in the vault,
  1–4 per note.
- **Agent links** are in a section `## Related` at the end of a note, at most three, only to notes that exist; the
  rest of the text stays unchanged.

## Where the buttons are

- **Settings → Workspace → “Tags”:** “Proposed by agents” with “Accept” / “Turn down”; “On the list (…)” with a
  description line per tag, the pencil (“Rename …”: first “Count uses”, then “Rename”) and the bin (“Take … off the
  list”); below it a line for adding (“Add”). A hexagon with a number on the “Workspace” tab and in the dock shows
  waiting proposals.
- **Below that, “Tag the whole vault”:** “1 · Propose a tag list” (“Propose”) and “2 · Plan tags and links for every
  note” (“Plan”, later “Plan the rest”), then “Preview (… notes)” with “Write into … notes” or “Discard”.
- **In the editor:** after `[[` notes complete, after `#` tags. ⌘-click (or Vi `gd`, or a click in the reading view)
  follows the link; ⌃- jumps back.
- **In the reading view** the tags from the frontmatter are chips at the top; a click shows the notes with this tag
  in the Second Brain. The tag chips in the Second Brain's detail panel, the tag group in ⌘K and the search field
  “Find notes or #tag” do the same.

## Common tasks

1. **Accept a proposal:** Settings → Workspace → “Proposed by agents” → “Accept”. The tag goes onto the list and onto
   the notes that wanted it.
2. **Rename or merge a tag:** the pencil at the tag → new name → “Count uses” → “Rename”. A name that is already on the
   list merges the two. All notes are rewritten along with it.
3. **Tag the whole vault:** “Tag the whole vault” → step 1 “Propose”, accept the proposals → step 2 “Plan” → check
   “Preview” → “Write into … notes”.

## Traps

- The vault run costs money (each step says beforehand what it reads and costs) and writes **nothing** before you
  click “Write into … notes”.
- “Take … off the list” only takes the tag off the list; the notes keep it.
- A link to a note that does not exist is shown dimmed; ⌘-click **creates it** (next to the current note). If a name
  matches several notes, name the folder too.
- If you rename a note in the app, it asks “Update … links in … notes?” — with “Update” the `[[Links]]` follow.
- Do not edit `TAGS.md` by hand; that is what the settings are for. In the terminal the owner has
  `axiomata-cli notes tags rename <old> <new>`, `notes tags describe <tag> "<Text>"` and `notes tags lift-inline`
  (moves inline tags into the frontmatter); `rename` and `lift-inline` first show with `--dry-run` what would
  change.

## See also

[Second Brain](second-brain.md) · [Editor](editor.md) · [Notes](notizen.md) · [Settings](einstellungen.md)
