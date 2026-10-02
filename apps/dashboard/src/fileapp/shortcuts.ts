/**
 * Every key the file app and its editor answer to, for the inspector's
 * Shortcuts tab (`docs/plans/editor-look.md`, LK1, K2).
 *
 * **It cannot fall behind the keys themselves**: `shortcuts.test.ts` turns each
 * entry of a `checked` group back into key presses and runs them through the
 * real key map (`editor/keymap.ts`) — every entry must do something, and every
 * distinct thing the key map does must be listed. The Vi groups are checked
 * against the machine's own grammar (`VI_GRAMMAR`, `EX_COMMAND_NAMES`). Keys
 * handled elsewhere (the file app, the language server, the completion menu)
 * are listed by hand; their groups are not `checked`.
 *
 * Keys are written as on a Mac: `⌃⌥⇧⌘` and then the key; alternatives are
 * separated by ` / `. Vi keys are written in Vim's own notation.
 */

import type { KeyInput } from "../editor/keymap";

export interface Shortcut {
  keys: string;
  what: string;
  /** A move that ⇧ turns into extending the selection: the test checks both. */
  extends?: boolean;
}

export interface ShortcutGroup {
  title: string;
  items: Shortcut[];
  /** Run through the real key map by the test (see the module doc). */
  checked?: boolean;
  /** Shown only while Vi is on. */
  vi?: boolean;
}

export const SHORTCUTS: ShortcutGroup[] = [
  {
    title: "Moving",
    checked: true,
    items: [
      { keys: "← / →", what: "One character left / right", extends: true },
      { keys: "⌥← / ⌥→", what: "One word left / right", extends: true },
      { keys: "⌘← / ⌘→", what: "Start / end of the line", extends: true },
      { keys: "↑ / ↓", what: "One line up / down", extends: true },
      { keys: "⌘↑ / ⌘↓", what: "Start / end of the file", extends: true },
      { keys: "Home / End", what: "Start / end of the line", extends: true },
      { keys: "⌘Home / ⌘End", what: "Start / end of the file", extends: true },
      { keys: "PgUp / PgDn", what: "One page up / down", extends: true },
      { keys: "⇧ + any move", what: "Extend the selection" },
    ],
  },
  {
    title: "Editing",
    checked: true,
    items: [
      { keys: "⌫ / ⌥⌫ / ⌘⌫", what: "Delete a character / word / to the line start backwards" },
      { keys: "⌦ / ⌥⌦", what: "Delete a character / word forwards" },
      { keys: "⏎", what: "New line" },
      { keys: "⇥ / ⇧⇥", what: "Indent / outdent" },
      { keys: "⌥↑ / ⌥↓", what: "Move the lines up / down" },
      { keys: "⌥⇧↑ / ⌥⇧↓", what: "Duplicate the lines up / down" },
      { keys: "⌘/", what: "Comment / uncomment the lines" },
      { keys: "⌘Z / ⇧⌘Z", what: "Undo / redo" },
      { keys: "⌘X / ⌘C / ⌘V", what: "Cut / copy / paste" },
      { keys: "⌘A", what: "Select everything" },
      { keys: "⌘L", what: "Select the line (again: the next one too) — code actions where a language server runs" },
      { keys: "⌘S", what: "Save (formats first where that is on)" },
      { keys: "⌘O", what: "Open a file" },
      { keys: "⇧⌘V", what: "Source / preview / side by side (Markdown, HTML, SVG)" },
      { keys: "⌥Z", what: "Wrap long lines on / off" },
      { keys: "Esc", what: "Back to one cursor" },
    ],
  },
  {
    title: "Multiple cursors",
    checked: true,
    items: [
      { keys: "⌥⌘↑ / ⌥⌘↓", what: "Add a cursor above / below" },
      { keys: "⌘D", what: "Add the next occurrence" },
      { keys: "⌘U", what: "Remove the last cursor" },
      { keys: "⇧⌘L", what: "Select every occurrence" },
    ],
  },
  {
    title: "Find",
    checked: true,
    items: [
      { keys: "⌘F", what: "Find in the file" },
      { keys: "⌥⌘F", what: "Find and replace" },
      { keys: "⌘G / ⇧⌘G", what: "Next / previous match" },
      { keys: "⌘E", what: "Search for the selection" },
    ],
  },
  {
    title: "Folding",
    checked: true,
    items: [
      { keys: "⌥⌘[ / ⌥⌘]", what: "Fold / unfold here" },
      { keys: "⌥⌘0 / ⌥⌘J", what: "Fold / unfold everything" },
    ],
  },
  {
    title: "Language server",
    items: [
      { keys: "F12 / ⌘-click", what: "Go to the definition" },
      { keys: "⌘F12", what: "Implementations" },
      { keys: "⇧F12", what: "All uses" },
      { keys: "Mouse rest", what: "What the server says about the symbol" },
      { keys: "F8 / ⇧F8", what: "Next / previous problem" },
      { keys: "⌘. / ⌘L", what: "Code actions (quick fixes, refactorings)" },
      { keys: "F2", what: "Rename the symbol" },
      { keys: "⇧⌥F", what: "Format the file" },
      { keys: "⌃Space", what: "Complete" },
      { keys: "⇧⌘Space", what: "Show the call's signature" },
    ],
  },
  {
    title: "Completion and code-action menus",
    items: [
      { keys: "↓ / ↑ / ⌃N / ⌃P", what: "Choose" },
      { keys: "⌃Y / ⏎", what: "Take (⏎ and ⇥ in completion only without Vi)" },
      { keys: "1–9", what: "Take that code action" },
      { keys: "Typing / ⌫", what: "Narrow / widen the code actions" },
      { keys: "⌃E / Esc", what: "Close" },
    ],
  },
  {
    title: "Studio",
    items: [
      { keys: "⌘P", what: "Open a file by name (name:12 goes to line 12)" },
      { keys: "⌘O", what: "Open a file with the file dialog" },
      { keys: "⇧⌘F", what: "Search the project (the sidebar's Search view)" },
      { keys: "⌘N", what: "New note" },
      { keys: "⌘W", what: "Close the file tab" },
      { keys: "⌘1 – ⌘9", what: "Go to tab 1–9" },
      { keys: "⌃⇥ / ⌃⇧⇥", what: "Next / previous tab (in the focused group)" },
      { keys: "⌘\\ / ⇧⌘\\", what: "Move the tab into a new group to the right / below" },
      { keys: "Drag a tab", what: "Onto a group: join it — onto an edge: split" },
      { keys: "Drag a divider", what: "Resize the groups beside it" },
      { keys: "⌘B", what: "Show / hide the sidebar column" },
    ],
  },
  {
    title: "Vi — moving",
    vi: true,
    items: [
      { keys: "h j k l", what: "Left, down, up, right" },
      { keys: "<Left> <Right> <Up> <Down> <BS> <Space> <C-n> <C-p>", what: "The same, by other keys" },
      { keys: "w W b B e E ge gE", what: "Words forward, back, to the end (W: separated by spaces only)" },
      { keys: "0 ^ $ g_ |", what: "Line start, first character, end, last character, column" },
      { keys: "<Home> <End>", what: "Line start / end" },
      { keys: "gg G", what: "First / last line (with a count: that line)" },
      { keys: "+ - _ <CR>", what: "First character of the next / previous / this line" },
      { keys: "{ } ( )", what: "Paragraphs and sentences" },
      { keys: "%", what: "The matching bracket" },
      { keys: "H M L", what: "Top, middle, bottom of the screen" },
      { keys: "gj gk", what: "Down / up by screen rows" },
      { keys: "f{c} F{c} t{c} T{c} ; ,", what: "To / before a character, again, back" },
      { keys: "/ ? n N * #", what: "Search forward / back, next, previous, the word under the cursor" },
      { keys: "'{a-z} `{a-z}", what: "To a mark (its line / its exact place)" },
    ],
  },
  {
    title: "Vi — changing",
    vi: true,
    items: [
      { keys: "d c y > < = g~ gu gU gc", what: "Delete, change, copy, indent, outdent, reindent, case, comment — then a motion or object" },
      { keys: "dd cc yy >> << == g~~ guu gUU gcc", what: "The same for the whole line" },
      { keys: "ys{motion}{c} yss{c} cs{a}{b} ds{c} S{c}", what: "Surround: add, change, delete" },
      { keys: "i a I A o O gi gI", what: "Insert before, after, line start, line end, new line below, above, where last" },
      { keys: "x X s S D C Y", what: "Delete character, before, substitute, line, to the end, change to the end, copy line" },
      { keys: "r{c} R", what: "Replace a character / Replace mode" },
      { keys: "p P gp gP", what: "Paste after / before (g: cursor after the text)" },
      { keys: "J gJ", what: "Join lines (without spaces)" },
      { keys: "~ U", what: "Toggle case (Visual U: upper case)" },
      { keys: "<C-a> <C-x>", what: "Add / subtract the count" },
      { keys: "u <C-r> .", what: "Undo, redo, repeat the last change" },
      { keys: "& @@ q{a-z} @{a-z} m{a-z}", what: "Repeat :s, repeat macro, record / play a macro, set a mark" },
      { keys: "gn gN", what: "Select the next / previous match (cgn: change it)" },
    ],
  },
  {
    title: "Vi — text objects (after i / a)",
    vi: true,
    items: [
      { keys: "w W s p", what: "Word, WORD, sentence, paragraph" },
      { keys: "\" ' `", what: "Quoted string" },
      { keys: "( ) b [ ] { } B < >", what: "Brackets" },
      { keys: "t", what: "HTML/XML tag" },
      { keys: "f c a", what: "Function, class, argument (tree-sitter)" },
    ],
  },
  {
    title: "Vi — modes, views and the rest",
    vi: true,
    items: [
      { keys: "v V <C-v> gv o O", what: "Visual by character, line, block; again; other end" },
      { keys: "<Esc> <C-[> <C-c>", what: "Back to Normal" },
      { keys: "zt zz zb z<CR> z. z-", what: "Scroll the cursor line to the top, middle, bottom" },
      { keys: "<C-e> <C-y> <C-d> <C-u> <C-f> <C-b> <PageDown> <PageUp>", what: "Scroll by lines, half pages, pages" },
      { keys: "<C-o> <C-i> <Tab>", what: "Back / forward in the jump list" },
      { keys: "zc zo za zR zM", what: "Close, open, toggle a fold; open / close all" },
      { keys: "]c [c", what: "Next / previous change in a diff" },
      { keys: "]d [d", what: "Next / previous problem" },
      { keys: "gd K gf", what: "Definition, hover, open the path under the cursor" },
      { keys: "gri grt grr", what: "Implementations, type definition, uses" },
      { keys: "grn gra", what: "Rename, code actions (Visual: for the selection)" },
      { keys: "ZZ ZQ", what: "Save and close / close without saving" },
      { keys: ":", what: "Command line (Visual: for the selected lines)" },
    ],
  },
  {
    title: "Vi — commands",
    vi: true,
    items: [
      { keys: ":w[rite] :wq :x[it] :q[uit] :q!", what: "Save, save and close, close" },
      { keys: ":e[dit]!", what: "Back to the file on disk" },
      { keys: ":s[ubstitute]/a/b/gc :&", what: "Replace (c: ask each time); repeat" },
      { keys: ":g/a/cmd :v/a/cmd", what: "Run a command on the lines that match / do not match" },
      { keys: ":d[elete] :norm[al] {keys}", what: "Delete lines; type keys on every line" },
      { keys: ":se[t] wrap nu rnu list", what: "Wrapping, numbers, relative numbers, whitespace" },
      { keys: ":noh[lsearch]", what: "Hide the search highlight" },
      { keys: ":for[mat] :ren[ame] {name} :act[ion]", what: "Format, rename, code actions" },
      { keys: ":{n}", what: "Go to line n" },
    ],
  },
];

/** The groups to show: the Vi ones only with Vi on — and then first, since they are the keys in use; narrowed by `query` (keys or description). */
export function visibleShortcuts(vi: boolean, query: string): ShortcutGroup[] {
  const q = query.trim().toLowerCase();
  const out: ShortcutGroup[] = [];
  for (const group of SHORTCUTS) {
    if (group.vi && !vi) continue;
    const items = q
      ? group.items.filter(
          (s) => s.keys.toLowerCase().includes(q) || s.what.toLowerCase().includes(q) || group.title.toLowerCase().includes(q),
        )
      : group.items;
    if (items.length > 0) out.push({ ...group, items });
  }
  // Stable: the Vi groups move up, everything keeps its order within its kind.
  return vi ? [...out.filter((g) => g.vi), ...out.filter((g) => !g.vi)] : out;
}

const MODIFIERS: Record<string, keyof Pick<KeyInput, "ctrl" | "alt" | "shift" | "meta">> = {
  "⌃": "ctrl",
  "⌥": "alt",
  "⇧": "shift",
  "⌘": "meta",
};

const KEY_NAMES: Record<string, string> = {
  "←": "ArrowLeft",
  "→": "ArrowRight",
  "↑": "ArrowUp",
  "↓": "ArrowDown",
  Home: "Home",
  End: "End",
  PgUp: "PageUp",
  PgDn: "PageDown",
  "⌫": "Backspace",
  "⌦": "Delete",
  "⏎": "Enter",
  "⇥": "Tab",
  Esc: "Escape",
};

/**
 * The key presses one written shortcut stands for (`⌥⇧↑` → ⌥⇧ArrowUp), one
 * per alternative; `null` for a form that is not a single key (`⇧ + any move`).
 */
export function keyPresses(keys: string): KeyInput[] | null {
  const out: KeyInput[] = [];
  for (const alternative of keys.split(" / ")) {
    const input: KeyInput = { key: "", meta: false, alt: false, shift: false, ctrl: false };
    let rest = alternative.trim();
    while (rest.length > 1 && rest[0] in MODIFIERS) {
      input[MODIFIERS[rest[0]]] = true;
      rest = rest.slice(1);
    }
    const key = KEY_NAMES[rest] ?? (rest.length === 1 ? rest.toLowerCase() : null);
    if (key === null) return null;
    input.key = key;
    out.push(input);
  }
  return out;
}
