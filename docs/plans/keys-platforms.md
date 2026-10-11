# Plan: Keys for Windows and Linux (board card #71)

Status: **decisions settled 2026-10-11 (grilled with the owner); K1–K6 built the same day, not yet tried on a real Windows or Linux machine** (the Mac path is pinned by tests; the "Keyboard" setting runs the other path on the Mac). Follows the owner's stepwise workflow;
reviews run once before each commit (lean cadence, `CLAUDE.md`).

## Why

Every shortcut in the app was written for the Mac: `metaKey` is checked in about twenty places, the Shortcuts tab and the
tooltips show ⌘⌥⇧⌃ glyphs, and `core/globalKeys.ts` is the only file that knows another platform exists. The owner wants the
**key layer** ready so that a Windows or Linux build is not blocked by it. No port is planned (`axiomata-macos`, the `.app`
bundle and the login-shell `PATH` stay Mac-only); this card is the keys alone, and it is **testable on the Mac** through a
simulation setting.

## Decisions (owner, 2026-10-11)

| # | Question | Decision |
|---|---|---|
| K-1 | Scope | The key layer only. No Windows/Linux port, no native-menu work (the menu keeps its predefined Mac items). |
| K-2 | Primary key | One abstraction in `core/keys.ts`: the **primary key is ⌘ on the Mac and Ctrl elsewhere**. Code asks `chord(e, platform).primary`, never `e.metaKey`. Cmd-click (go to definition, open link) goes with the primary key. |
| K-3 | Terminal | In a terminal pane **every Ctrl combination belongs to the shell** (Ctrl+C is SIGINT, Ctrl+W deletes a word). App shortcuts that must work there need **Ctrl+Shift**. On the Mac nothing changes (⌘ is free there). |
| K-4 | Display | One central table of shortcuts (action → key) feeds matching *and* display: glyphs on the Mac (⌘⇧⌥⌃), English words elsewhere ("Ctrl+Shift+P"). The ~49 literal ⌘ in tooltips and labels go through one function. |
| K-5 | Rollout | Per area, each with tests that pin today's Mac behaviour: core → global → Studio → editor/terminal → diff/file panel → display last. The Mac path must be byte-for-byte unchanged at every step. |
| K-6 | Vi vs app keys | The Ctrl keys Vi's grammar knows (`VI_GRAMMAR`: Ctrl+f/b/d/u/r/v/o/i/…) belong to Vi while Vi is on. A test derives the collision list from the grammar; an app command that collides gets a Ctrl+Shift variant. |
| K-7 | Close window | **Ctrl+Shift+W** off the Mac (⌘W on the Mac). Plain Ctrl+W stays "delete word". |
| K-8 | Alt | ⌥ becomes Alt 1:1. OS-level collisions (Alt+Tab, Alt+F4, window-manager shortcuts on Linux) are written down; alternatives only after tests on a real machine. |
| K-9 | Simulation | A setting **Keyboard: Auto / Mac / Windows & Linux** (default Auto) under Keys. Auto reads `navigator.platform`; the other two force a platform so the whole layer runs on the Mac. Persisted in `dashboard.json` like the UI size. |
| K-10 | Editor semantics | The editor engine (`editor/keymap.ts`) keeps only the Mac vocabulary (D1: it imports nothing from the app). `EditorSurface` **translates** the other platform's keys into it before the call: Ctrl+←/→ → ⌥←/→ (word), Home/End → ⌘←/→ (line), Ctrl+Home/End → ⌘↑/↓ (document), Ctrl+Backspace → ⌥⌫ (word), Ctrl+S → ⌘S and so on. The table is a test (key in, effect out). Alt+↑/↓ (move line) stays. |
| K-11 | AltGr | **AltGr is always typing, never a chord.** On a German layout under Windows/Linux AltGr arrives as Ctrl+Alt; `chord` reads `getModifierState("AltGraph")` and, where that is missing (WebKitGTK is unchecked), treats Ctrl+Alt with a non-alphanumeric produced character as typing. Tested. No AltGr on the Mac. |
| K-12 | Terminal clipboard | Ctrl+Shift+C / Ctrl+Shift+V off the Mac (⌘C / ⌘V on the Mac). Ctrl+Insert / Shift+Insert are not added. |
| K-14 | Fold keys | Fold / unfold here is **Ctrl+Shift+[ / ]** off the Mac (read by the physical key), not Ctrl+Alt+[ / ]: that pair is what AltGr+8 / AltGr+9 send on a German layout and must stay typing. Fold all / unfold all are Ctrl+Alt+0 / Ctrl+Alt+J. |
| K-15 | Studio and diff letter keys | Off the Mac every letter command (quick open P, new note N, open O, sidebar B, close file W, project search F; diff reload R, layout D) is **Ctrl+Shift+letter**, always — not only where Vi collides — so a key never changes meaning with the focus. Digits (Ctrl+1–9) and `\` stay plain Ctrl. Discard hunk is Ctrl+Shift+Backspace. |
| K-13 | Delivery | Checkpoints K1–K6 below; two commits (K1–K3, K4–K6), one combined review before each; the plan is written before K1. |

## Known limits (accepted, 2026-10-11)

- **Vi on a Markdown/HTML file:** Ctrl+Shift+V toggles the preview (the file editor takes it before Vi), so there is no keyboard paste in Insert mode there; paste with `p`. Shift+Insert is not added.
- **Vi in the Studio:** Ctrl+F is Vi's page-forward and Ctrl+Shift+F is the project search, so "find in the file" has no key while Vi is on — use `/`. The list hides that entry then.
- **Terminal:** Ctrl+Shift+\ can only split to the right (the terminal's Ctrl+Shift stands for Ctrl, so the Shift cannot also mean "below").
- **Ctrl+Alt+symbol is never a shortcut** off the Mac (AltGr guard); only letters, digits and arrows take Ctrl+Alt.
- **OS collisions of Alt/Ctrl+Alt** (Alt+Tab, Ctrl+Alt+Arrow screen rotation on some drivers, Ctrl+Alt+L screen lock on some Linux desktops, Alt+click window drag on some window managers) are not worked around; alternatives only after a test on a real machine.
- ⌃⌘ (Control and Command together) on the Mac no longer counts as ⌘ in the places that read the primary key; it was never a binding.
- Nothing here was run on Windows or Linux: AltGr on WebKitGTK (`getModifierState("AltGraph")`) and `execCommand("copy")` for Ctrl+Shift+C are the first things to check.

## What was built

`core/keys.ts` (platform, `chord`, AltGr, setting), `core/keyHints.ts` (labels), `core/globalKeys.ts`, `ide/studioKeys.ts`,
`fileapp/editorKeys.ts` (the edge in front of the engine), `fileapp/diffKeys.ts`, `fileapp/panelZoomKeys.ts`,
`modules/terminalInput.ts` (`terminalKeyRoute`), `fileapp/shortcuts.ts` (the list per platform). Every table has a test that
presses the listed keys through the real matching code. `editor/` is unchanged (D1).

## Checkpoints

1. **K1 — core:** `core/keys.ts`: `Platform`, `detectPlatform`, `chord` (primary/alt/shift, AltGr guard), the `keyboard` setting
   store + persistence, and tests.
2. **K2 — global and Studio:** `globalKeys.ts` on `chord`; `IdeView`'s `onViewKeydown`; `FilePanel` zoom; Spotlight's ⌘Return.
3. **K3 — editor boundary, Vi collisions, terminal:** the translation table in front of `keyAction`; the Vi collision test;
   the terminal's Ctrl+Shift app keys and clipboard.
4. **K4 — diff, file panel, rest:** `DiffView`, `DiffPanes`, `FindBar`, `FileEditor`, `GitPanel`, `SessionMail`,
   `GitActionDialog`.
5. **K5 — display:** the shortcut table feeds `fileapp/shortcuts.ts`, tooltips and labels; the 49 literals go through it.
6. **K6 — docs:** this plan's status, `docs/architecture.md`, memory note.

## Not in scope

The native menu; `axiomata-macos`; a Windows/Linux build; remapping keys by the user; the LSP/Vi key tables themselves
(only their collisions with the app).
