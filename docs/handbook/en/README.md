# The Axiomata-OS handbook

This folder is the **source** of the handbook: one page per area of the app, in English, with the buttons' real names.
The chat in the bar answers only from these pages and from the app's live data; what is not here, it does not know.

- **Edit here, nowhere else.** The app ships the handbook with every build and puts it into `~/.axiomata/handbook/`
  at start. If the copy there does not match the installed build, the app replaces it — so a change in
  `~/.axiomata/handbook/` is lost at the next start.
- **Your own notes about the app** (habits, ideas, open questions) belong in the vault, not here.
- **Whoever builds or changes a visible feature keeps its page up to date** in the same go; without that the round is
  not done.

Shape of every page: `# Title`, below it **one** sentence of summary (it becomes the chat's table of contents), then
“What it is”, “Where the buttons are”, “Common tasks”, “Traps” and, if needed, “See also”. About one screen long,
without screenshots, without keys or other secrets. The entry point is [index.md](index.md).
