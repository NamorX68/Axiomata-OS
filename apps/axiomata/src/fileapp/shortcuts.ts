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
 * separated by ` / `. Vi keys are written in Vim's own notation. A group's title and an entry's description are keys of
 * the editor catalog (`i18n/editor.ts`); `visibleShortcuts` hands them out in the current language.
 */

import { keyHints } from "../core/keyHints";
import type { Platform } from "../core/keys";
import type { KeyInput } from "../editor/keymap";
import { tEditor, type EditorText } from "../i18n/editor";
import { VI_CTRL_KEYS } from "./editorKeys";

export interface Shortcut {
  keys: string;
  what: EditorText;
  /** A move that ⇧ turns into extending the selection: the test checks both. */
  extends?: boolean;
  /**
   * The keys on Windows and Linux when they are not the glyphs turned into words (`core/keyHints.ts` does that):
   * `null` leaves the entry out there. `editorKeys.ts` is where the editor's keys are translated;
   * `shortcuts.test.ts` presses these through it.
   */
  other?: string | null;
  /** The keys there while Vi is on, where they differ from `other` (`null`: none, Vi has its own). */
  otherVi?: string | null;
  /** The description there, where it differs. */
  otherWhat?: EditorText;
  /** An entry that exists only on Windows and Linux; `keys` is then already in their words. */
  onlyOther?: boolean;
}

export interface ShortcutGroup {
  title: EditorText;
  items: Shortcut[];
  /** Run through the real key map by the test (see the module doc). */
  checked?: boolean;
  /** Shown only while Vi is on. */
  vi?: boolean;
}

export const SHORTCUTS: ShortcutGroup[] = [
  {
    title: "scgMoving",
    checked: true,
    items: [
      { keys: "← / →", what: "scOneCharacterLeftRight", extends: true },
      { keys: "⌥← / ⌥→", what: "scOneWordLeftRight", extends: true, other: "Ctrl+← / Ctrl+→" },
      { keys: "⌘← / ⌘→", what: "scStartEndOfTheLine", extends: true, other: null },
      { keys: "↑ / ↓", what: "scOneLineUpDown", extends: true },
      { keys: "⌘↑ / ⌘↓", what: "scStartEndOfTheFile", extends: true, other: "Ctrl+Home / Ctrl+End" },
      { keys: "Home / End", what: "scStartEndOfTheLine", extends: true },
      { keys: "⌘Home / ⌘End", what: "scStartEndOfTheFile", extends: true, other: null },
      { keys: "PgUp / PgDn", what: "scOnePageUpDown", extends: true },
      { keys: "⇧ + any move", what: "scExtendTheSelection" },
    ],
  },
  {
    title: "scgEditing",
    checked: true,
    items: [
      {
        keys: "⌫ / ⌥⌫ / ⌘⌫",
        what: "scDeleteACharacterWordTo",
        other: "Backspace / Ctrl+Backspace",
        otherWhat: "scDeleteACharacterWordBackwards",
      },
      { keys: "⌦ / ⌥⌦", what: "scDeleteACharacterWordForwards", other: "Delete / Ctrl+Delete" },
      { keys: "⏎", what: "scNewLine" },
      { keys: "⇥ / ⇧⇥", what: "scIndentOutdent" },
      { keys: "⌥↑ / ⌥↓", what: "scMoveTheLinesUpDown" },
      { keys: "⌥⇧↑ / ⌥⇧↓", what: "scDuplicateTheLinesUpDown" },
      { keys: "⌘/", what: "scCommentUncommentTheLines" },
      { keys: "⌘Z / ⇧⌘Z", what: "scUndoRedo" },
      { keys: "⌘X / ⌘C / ⌘V", what: "scCutCopyPaste" },
      { keys: "⌘A", what: "scSelectEverything" },
      { keys: "⌘L", what: "scSelectTheLineAgainThe" },
      { keys: "⌘S", what: "scSaveFormatsFirstWhereThat" },
      { keys: "⌘O", what: "scOpenAFile" },
      { keys: "⇧⌘V", what: "scSourcePreviewSideBySide" },
      { keys: "⌥Z", what: "scWrapLongLinesOnOff" },
      { keys: "Esc", what: "scBackToOneCursor" },
    ],
  },
  {
    title: "scgMultipleCursors",
    checked: true,
    items: [
      { keys: "⌥⌘↑ / ⌥⌘↓", what: "scAddACursorAboveBelow" },
      { keys: "⌘D", what: "scAddTheNextOccurrence" },
      { keys: "⌘U", what: "scRemoveTheLastCursor" },
      { keys: "⇧⌘L", what: "scSelectEveryOccurrence" },
    ],
  },
  {
    title: "scgFind",
    checked: true,
    items: [
      { keys: "⌘F", what: "scFindInTheFile", otherVi: null },
      { keys: "⌥⌘F", what: "scFindAndReplace" },
      { keys: "⌘G / ⇧⌘G", what: "scNextPreviousMatch" },
      { keys: "⌘E", what: "scSearchForTheSelection" },
    ],
  },
  {
    title: "scgFolding",
    checked: true,
    items: [
      { keys: "⌥⌘[ / ⌥⌘]", what: "scFoldUnfoldHere", other: "Ctrl+Shift+[ / Ctrl+Shift+]" },
      { keys: "⌥⌘0 / ⌥⌘J", what: "scFoldUnfoldEverything" },
    ],
  },
  {
    title: "scgLanguageServer",
    items: [
      { keys: "F12 / ⌘-click", what: "scGoToTheDefinition" },
      { keys: "⌘F12", what: "scImplementations" },
      { keys: "⇧F12", what: "scAllUses" },
      { keys: "Mouse rest", what: "scWhatTheServerSaysAbout" },
      { keys: "F8 / ⇧F8", what: "scNextPreviousProblem" },
      { keys: "⌘. / ⌘L", what: "scCodeActionsQuickFixesRefactorings" },
      { keys: "F2", what: "scRenameTheSymbol" },
      { keys: "⇧⌥F", what: "scFormatTheFile" },
      { keys: "⌃Space", what: "scComplete" },
      { keys: "⇧⌘Space", what: "scShowTheCallSSignature" },
    ],
  },
  {
    // `docs/plans/tags-links.md`, TL2: handled by `FileEditor.svelte` and `jumpHistory.ts`, not the key map.
    title: "scgNotesWikilinksAndTags",
    items: [
      { keys: "⌘-click", what: "scFollowAWikilinkIntoNothing" },
      { keys: "Click", what: "scInTheReadingViewFollow" },
      { keys: "⌃- / ⌃⇧-", what: "scBackForwardAlongTheJumps", other: "Ctrl+Alt+← / Ctrl+Alt+→" },
      { keys: "[[ / #", what: "scCompleteANoteNameA" },
    ],
  },
  {
    title: "scgCompletionAndCodeActionMenus",
    items: [
      { keys: "↓ / ↑ / ⌃N / ⌃P", what: "scChoose" },
      { keys: "⌃Y / ⏎", what: "scTakeAndInCompletionOnly" },
      { keys: "1–9", what: "scTakeThatCodeAction" },
      { keys: "Typing / ⌫", what: "scNarrowWidenTheCodeActions" },
      { keys: "⌃E / Esc", what: "scClose" },
    ],
  },
  {
    title: "scgEverywhereInTheApp",
    items: [
      { keys: "⌘K", what: "scOpenCloseTheSearchThe" },
      { keys: "⌘,", what: "scOpenTheSettings" },
      { keys: "⌘N", what: "scInTheChatInputA" },
      { keys: "⇧⇥", what: "scInTheChatInputSwitch" },
      { keys: "⌘W", what: "scCloseTheTopmostWindowThe" },
      { keys: "Esc", what: "scCloseTheTopmostWindowIn" },
      { keys: "F1 / F5, F2 / F10, F3, F4", what: "scWhileTheDebuggerIsStopped" },
      {
        keys: "Ctrl+Shift+…",
        what: "scInATerminalPaneEvery",
        onlyOther: true,
      },
      { keys: "Ctrl+Shift+C / Ctrl+Shift+V", what: "scCopyPasteInATerminal", onlyOther: true },
    ],
  },
  {
    title: "scgStudio",
    items: [
      { keys: "⌘P", what: "scOpenAFileByName" },
      { keys: "⌘O", what: "scOpenAFileWithThe" },
      { keys: "⇧⌘F", what: "scSearchTheProjectTheSidebar" },
      { keys: "⌘N", what: "scNewNote" },
      { keys: "⌘W", what: "scCloseTheFileTab" },
      { keys: "⌘1 – ⌘9", what: "scGoToTab19" },
      { keys: "⌃⇥ / ⌃⇧⇥", what: "scNextPreviousTabInThe" },
      { keys: "⌘\\ / ⇧⌘\\", what: "scMoveTheTabIntoA" },
      { keys: "Drag a tab", what: "scOntoAGroupJoinIt" },
      { keys: "Drag a divider", what: "scResizeTheGroupsBesideIt" },
      { keys: "⌘B", what: "scShowHideTheSidebarColumn" },
    ],
  },
  {
    title: "scgViMoving",
    vi: true,
    items: [
      { keys: "h j k l", what: "scLeftDownUpRight" },
      { keys: "<Left> <Right> <Up> <Down> <BS> <Space> <C-n> <C-p>", what: "scTheSameByOtherKeys" },
      { keys: "w W b B e E ge gE", what: "scWordsForwardBackToThe" },
      { keys: "0 ^ $ g_ |", what: "scLineStartFirstCharacterEnd" },
      { keys: "<Home> <End>", what: "scLineStartEnd" },
      { keys: "gg G", what: "scFirstLastLineWithA" },
      { keys: "+ - _ <CR>", what: "scFirstCharacterOfTheNext" },
      { keys: "{ } ( )", what: "scParagraphsAndSentences" },
      { keys: "%", what: "scTheMatchingBracket" },
      { keys: "H M L", what: "scTopMiddleBottomOfThe" },
      { keys: "gj gk", what: "scDownUpByScreenRows" },
      { keys: "f{c} F{c} t{c} T{c} ; ,", what: "scToBeforeACharacterAgain" },
      { keys: "/ ? n N * #", what: "scSearchForwardBackNextPrevious" },
      { keys: "'{a-z} `{a-z}", what: "scToAMarkItsLine" },
    ],
  },
  {
    title: "scgViChanging",
    vi: true,
    items: [
      { keys: "d c y > < = g~ gu gU gc", what: "scDeleteChangeCopyIndentOutdent" },
      { keys: "dd cc yy >> << == g~~ guu gUU gcc", what: "scTheSameForTheWhole" },
      { keys: "ys{motion}{c} yss{c} cs{a}{b} ds{c} S{c}", what: "scSurroundAddChangeDelete" },
      { keys: "i a I A o O gi gI", what: "scInsertBeforeAfterLineStart" },
      { keys: "x X s S D C Y", what: "scDeleteCharacterBeforeSubstituteLine" },
      { keys: "r{c} R", what: "scReplaceACharacterReplaceMode" },
      { keys: "p P gp gP", what: "scPasteAfterBeforeGCursor" },
      { keys: "J gJ", what: "scJoinLinesWithoutSpaces" },
      { keys: "~ U", what: "scToggleCaseVisualUUpper" },
      { keys: "<C-a> <C-x>", what: "scAddSubtractTheCount" },
      { keys: "u <C-r> .", what: "scUndoRedoRepeatTheLast" },
      { keys: "& @@ q{a-z} @{a-z} m{a-z}", what: "scRepeatSRepeatMacroRecord" },
      { keys: "gn gN", what: "scSelectTheNextPreviousMatch" },
    ],
  },
  {
    title: "scgViTextObjectsAfterI",
    vi: true,
    items: [
      { keys: "w W s p", what: "scWordWORDSentenceParagraph" },
      { keys: "\" ' `", what: "scQuotedString" },
      { keys: "( ) b [ ] { } B < >", what: "scBrackets" },
      { keys: "t", what: "scHTMLXMLTag" },
      { keys: "f c a", what: "scFunctionClassArgumentTreeSitter" },
    ],
  },
  {
    title: "scgViModesViewsAndThe",
    vi: true,
    items: [
      { keys: "v V <C-v> gv o O", what: "scVisualByCharacterLineBlock" },
      { keys: "<Esc> <C-[> <C-c>", what: "scBackToNormal" },
      { keys: "zt zz zb z<CR> z. z-", what: "scScrollTheCursorLineTo" },
      { keys: "<C-e> <C-y> <C-d> <C-u> <C-f> <C-b> <PageDown> <PageUp>", what: "scScrollByLinesHalfPages" },
      { keys: "<C-o> <C-i> <Tab>", what: "scBackForwardInTheJump" },
      { keys: "zc zo za zR zM", what: "scCloseOpenToggleAFold" },
      { keys: "]c [c", what: "scNextPreviousChangeInA" },
      { keys: "]d [d", what: "scNextPreviousProblem" },
      { keys: "gd K gf", what: "scDefinitionOnANoteS" },
      { keys: "gri grt grr", what: "scImplementationsTypeDefinitionUses" },
      { keys: "grn gra", what: "scRenameCodeActionsVisualFor" },
      { keys: "ZZ ZQ", what: "scSaveAndCloseCloseWithout" },
      { keys: ":", what: "scCommandLineVisualForThe" },
    ],
  },
  {
    title: "scgViCommands",
    vi: true,
    items: [
      { keys: ":w[rite] :wq :x[it] :q[uit] :q!", what: "scSaveSaveAndCloseClose" },
      { keys: ":e[dit]!", what: "scBackToTheFileOn" },
      { keys: ":s[ubstitute]/a/b/gc :&", what: "scReplaceCAskEachTime" },
      { keys: ":g/a/cmd :v/a/cmd", what: "scRunACommandOnThe" },
      { keys: ":d[elete] :norm[al] {keys}", what: "scDeleteLinesTypeKeysOn" },
      { keys: ":se[t] wrap nu rnu list", what: "scWrappingNumbersRelativeNumbersWhitespace" },
      { keys: ":noh[lsearch]", what: "scHideTheSearchHighlight" },
      { keys: ":for[mat] :ren[ame] {name} :act[ion]", what: "scFormatRenameCodeActions" },
      { keys: ":{n}", what: "scGoToLineN" },
    ],
  },
];

/** A shortcut as the panel shows it: keys for the platform, the description in the current language. */
export interface VisibleShortcut {
  keys: string;
  what: string;
}

/** A group as the panel shows it. */
export interface VisibleGroup {
  title: string;
  items: VisibleShortcut[];
  checked?: boolean;
  vi?: boolean;
}

/** What `item` shows on `on`, or `null` when it is not there (a Mac-only key, or one Vi takes over). */
function shown(item: Shortcut, on: Platform, vi: boolean): VisibleShortcut | null {
  if (on === "mac") return item.onlyOther ? null : { keys: item.keys, what: tEditor(item.what) };
  const keys = vi && item.otherVi !== undefined ? item.otherVi : item.other;
  if (keys === null) return null;
  const text = keys ?? (item.onlyOther ? item.keys : keyHints(item.keys, on, { vi, viLetters: VI_CTRL_KEYS }));
  return { keys: text, what: tEditor(item.otherWhat ?? item.what) };
}

/**
 * The groups to show on `on`: the Vi ones only with Vi on — and then first, since they are the keys in use; narrowed by
 * `query` (keys or description, as shown — in the current language).
 */
export function visibleShortcuts(vi: boolean, query: string, on: Platform = "mac"): VisibleGroup[] {
  const q = query.trim().toLowerCase();
  const out: VisibleGroup[] = [];
  for (const group of SHORTCUTS) {
    if (group.vi && !vi) continue;
    const title = tEditor(group.title);
    const present = group.items.flatMap((item) => shown(item, on, vi) ?? []);
    const items = q
      ? present.filter(
          (s) =>
            s.keys.toLowerCase().includes(q) || s.what.toLowerCase().includes(q) || title.toLowerCase().includes(q),
        )
      : present;
    if (items.length > 0) out.push({ title, items, checked: group.checked, vi: group.vi });
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
