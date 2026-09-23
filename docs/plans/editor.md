# Plan: Die Datei-App — ein eigener Editor als Single Point of Truth

Status: **gegrillt und bestätigt, Umsetzung noch nicht begonnen** (Stand 2026-09-23).
Owner-Wunsch: „nicht sofort umsetzen, aber schon einmal planen". Begonnen wird erst auf
ausdrückliches „los", mit ED0.

## 1. Idee

Eine App, die sich um **alles** kümmert, was mit Dateien zu tun hat: ansehen,
bearbeiten, neu anlegen, suchen, vergleichen. Heute sind das fünf verstreute Stellen mit
einem nackten `<textarea>` als einzigem Editor. Stattdessen:

- **Ein AAA-Editor**, der ein Hingucker ist: frei wählbare Schriften samt Schnitt
  (bis „Thin") und Größe, Vi-Modus und normaler Modus, Zeilennummern absolut und
  relativ, und was ein ernsthafter Code-Editor sonst braucht — bis hin zu LSP.
- **Single Point of Truth:** Jeder Ort in Axiomata, der eine Datei zeigt oder bearbeitet,
  benutzt dieselbe App (FileViewer, Neue Notiz, Second Brain, ToDo, die IDE, M7.3s Diffs).
- **Herauslösbar wie das Terminal und die IDE:** eigener Kern ohne Abhängigkeit von
  `axiomata-core`, später mit wenig Aufwand als eigenständige App nutzbar.
- **Selbst gebaut** (Owner-Grundsatz, wie beim Terminal statt `xterm.js`): eine eigene
  Editor-Engine, kein eingebettetes Monaco/CodeMirror. Bibliotheken für Teilprobleme
  sind wie beim Terminal (`vte`, `portable-pty`) in Ordnung; Muster aus Vim, Helix, Zed,
  VS Code und Neovide werden ausgewertet, nicht übernommen.

## 2. Was es heute gibt

| Ort | Kann | Anmerkung |
|---|---|---|
| FileViewer (`modules/md-file.svelte`, schwebendes Panel) | Markdown rendern, HTML per `srcdoc`, Bilder, Text/Code lesen; Bearbeiten als `<textarea>` | Der einzige „Editor" |
| „New note" (TopBar) | `md-file` im Compose-Modus → `create_note` (Agent wählt den Bereich) | |
| Second-Brain-Detailpanel | Vorschau, Löschen | |
| ToDo-Modul | liest/schreibt `ToDo.md` | eigene Oberfläche, bleibt |
| IDE (M7) | noch keine Datei-Ansicht | geplant: `FilePane`, CP9 „Datei aus dem Diff öffnen" |

Backend: `read/write/delete_workspace_file`, `read_workspace_image`, `search_workspace` —
**absichtlich auf den Second-Brain-Workspace eingesperrt** (`workspace::resolve`). Ein Editor
für die IDE muss auch Projektordner und Agenten-Worktrees öffnen.

## 3. Funktionskatalog (was in welchem Meilenstein kommt: §4 und §5)

**Kern**
- Puffer für große Dateien (Rope o. ä.), Undo/Redo (ggf. als Baum), Auswahl, **mehrere
  Cursor**, Suchen/Ersetzen mit Regex, Soft-Wrap, Einrückung, Zeilenenden/Encoding.
- Tastenbelegung **Normal** (Mac-üblich) und **Vi**: Normal/Insert/Visual/Visual-Line/
  Visual-Block, Operatoren × Bewegungen × Textobjekte, Zähler, Register, `.`-Wiederholung,
  Marken, `/`-Suche, `:`-Befehle; später Makros.
- Zeilennummern **absolut / relativ / hybrid**, Gutter mit Git-Markern und Diagnosen.
- Schriften: Familie (die elf gebündelten Mono-Schriften plus System), **Schnitt von Thin
  bis Bold**, Größe, Zeilenhöhe, Ligaturen an/aus.

**Hingucker**
- Weicher, animierter Cursor (Neovide-artige Spur, abschaltbar), sanftes Scrollen,
  Modus-Pille in der Farbe des Vi-Modus, Einrückungslinien, farbige Klammerpaare,
  hervorgehobene aktuelle Zeile, Minimap, „Sticky Scroll" (Kopfzeile des umgebenden
  Blocks bleibt stehen), dezentes Leuchten im Akzent-Orange, alles über `--ax-*`-Token
  und damit in allen fünf Themes.

**Sprache**
- Syntax-Hervorhebung (inkrementell), Faltung, Klammer-Sprung.
- **LSP-Client**: Diagnosen, Hover, Vervollständigung, Gehe-zu-Definition, Umbenennen,
  Formatieren — gegen die Server, die installiert sind (rust-analyzer, typescript-
  language-server, svelte-language-server …).

**Datei-App drumherum**
- Schnellöffnen (unscharfe Suche), Dateibaum, zuletzt geöffnet, Tabs/Splits (in der IDE
  über deren Dock), Markdown mit Live-Vorschau daneben, HTML-/Bild-Ansicht wie heute,
  **Diff-Ansicht** (teilt sich die Darstellung mit M7.3 CP8), Git-Gutter (M7.3-Engine),
  Erkennen externer Änderungen (ein Agent schreibt gerade in dieselbe Datei!),
  Autospeichern, Wiederherstellen ungespeicherter Stände.

## 4. Entscheidungen (gegrillt 2026-09-23, Q1–Q18, alle vom Owner bestätigt)

**Bauweise**

- **D1 — Engine in TypeScript** (Q1): Puffer, Cursor, Auswahl, Undo, Vi-Automat und
  Layout leben als eigenständiges Paket `apps/dashboard/src/editor/` — ohne Svelte, mit
  `vitest` getestet (wie `ide/layout.ts`). Grund: Der Text lebt, wo er gezeichnet wird;
  Eingabe mit Umlauten, toten Tasten und Mac-Eingabemethode passiert im Webview, und
  jeder Tastendruck ohne IPC-Umlauf hält den animierten Cursor flüssig. Anders als beim
  Terminal gibt es keinen Prozess in Rust, der das erzwänge.
- **D2 — Neue Rust-Crate `axiomata-files`** (Q1): Dateien lesen/schreiben, externe
  Änderungen beobachten, Suche, LSP-Server starten und ihr stdio durchreichen. Ohne
  Abhängigkeit von `axiomata-core`, wie `axiomata-terminal`.
- **D3 — Darstellung hybrid** (Q2): Text im DOM (virtualisierte Zeilen: scharf, echte
  Schnitte, Ligaturen, Kopieren), darüber eine Canvas-Ebene nur für Effekte.
- **D4 — Syntax über tree-sitter als WASM** (Q7): eine Parser-Bibliothek wie `vte` beim
  Terminal, kein fertiger Editor. Derselbe Baum speist Hervorhebung, Faltung,
  Vi-Textobjekte für Funktionen (`if`/`af`) und Sticky Scroll.
- **D5 — Dateizugriff** (Q3): registrierte Wurzeln (Second-Brain-Workspace,
  IDE-Projektordner, Agenten-Worktrees) plus, was der Nutzer selbst im **nativen
  Öffnen-Dialog** wählt. Ein Pfad, den nur der Webview behauptet, gibt nie etwas frei.

**Einstellungen und Aussehen**

- **D6 — Eigene Editor-Einstellungen** (Q5, Q8): `~/.axiomata/editor-settings.json`,
  erreichbar über ein Zahnrad im Editor, mit **Live-Vorschau** (Code-Beispiel mit Cursor,
  Zeilennummern, Hervorhebung). Inhalt: Modus normal/Vi (Vorgabe normal), Schrift,
  Schnitt, Größe, Zeilenhöhe, Ligaturen, Zeilennummern absolut/relativ/hybrid, Effekte
  an/aus, Zwischenablage, Autospeichern, LSP-Befehle je Sprache.
- **D7 — Schriften** (Q13): Der Schnitt-Regler bietet nur **echte** Schnitte, geladen
  erst bei Bedarf. v1: die elf mitgelieferten Mono-Schriften mit **allen** ihren
  Schnitten (Thin 100 bei JetBrains, IBM Plex, Roboto, Victor; ab 200 Source Code Pro
  und Inconsolata; ab 300 Fira Code; nur 400/700 bei Space, Ubuntu, Anonymous Pro —
  heute lädt das Terminal je Schrift nur drei). Installierte Mac-Schriften in ED5.
- **D8 — Hingucker** (Q17): in **ED2** weicher, animierter Cursor mit Spur (abschaltbar),
  sanftes Scrollen, hervorgehobene aktuelle Zeile, Einrückungslinien, farbige
  Klammerpaare, dezentes Akzent-Leuchten, schöne Zeilennummern; die Modus-Pille mit ED3.
  In **ED5** Minimap, Sticky Scroll, animierte Faltung. Alles über `--ax-*`-Token.

**Arbeiten mit Dateien**

- **D9 — Speichern** (Q9): ⌘S bzw. `:w`, Autospeichern als Einstellung; ungespeicherte
  Stände werden nebenbei unter `~/.axiomata/editor-recovery/` gesichert und beim
  nächsten Öffnen zur Wiederherstellung angeboten.
- **D10 — Externe Änderungen** (Q10): ohne eigene ungespeicherte Änderungen still neu
  laden (kurzer Hinweis); mit welchen ein Balken *Neu laden / Meine behalten /
  Unterschied ansehen*. Nie automatisch mischen.
- **D11 — Markdown** (Q11): Umschalter Quelltext/Vorschau und nebeneinander mit
  synchronem Scrollen in v1; Obsidian-artige Live-Vorschau im Text später.

**Orte und Umfang**

- **D12 — Wo der Editor erscheint** (Q12): im schwebenden Panel (fährt von unten hoch;
  für „aus einem Kontext heraus öffnen") und als **Vollbild-Ansicht** mit Dateibaum,
  Schnellöffnen und Tabs; in der IDE als Dock-Pane. **Keine** Kachel.
- **D13 — App-Ring** (Q18): **ein** Icon „Editor" (`edit_document`, Material Symbols),
  das die Vollbild-Ansicht öffnet. Der Ring lernt dafür neben „Kachel anlegen" den
  Eintragstyp **„Ansicht öffnen"**, und die **IDE bekommt denselben Weg** mit einem
  eigenen Ring-Icon (Owner, 2026-09-23). Das Panel hat kein Icon; es erscheint, wenn eine
  Datei aus Second Brain, Chat oder Diff geöffnet wird.
- **D14 — Was zur Datei-App gehört** (Q6): Text, Code, Markdown, HTML, Bilder, Neue
  Notiz, Second-Brain-Vorschau, IDE-Dateiansicht, Diffs. Eigenständig bleiben ToDo,
  Kanban und der Second-Brain-Graph. PDF später.
- **D15 — Reihenfolge** (Q4): ED0 → ED1 → ED2 → **M7.3 CP8/CP9 auf dem Editor**
  (eine Diff-Ansicht ist ein schreibgeschützter Editor mit Markierungen) → ED3 → ED4 →
  ED5 → ED6 → ED7.

**Bedienung**

- **D16 — Tasten** (Q14): Esc gehört immer dem Editor (Panel schließt mit ⌘W oder ×);
  im normalen Modus gehören ⌘S/⌘F/⌘Z dem Editor, im Vi-Modus die Vi-Tasten und ⌘-Tasten
  bleiben nutzbar; app-weite Tasten (⌘K, wenn es kommt) gehen vor. Dieselbe Regel wie
  in der IDE.
- **D17 — Vi in ED3** (Q15): Normal/Insert/Visual/-Line/-Block, die üblichen Bewegungen,
  Operatoren, Textobjekte (auch `if`/`af` über tree-sitter), Zähler, `.`, Undo/Redo,
  Register, Marken, Suche, `:w :q :s :{zeile}`, **Makros**. Später: `:g`, Ex-Bereiche,
  eigene Belegungen. Zwischenablage: Vorgabe **gemeinsam mit dem Mac**, per Einstellung
  getrennt.
- **D18 — LSP in ED6** (Q16): Server automatisch auf dem `PATH` erkannt, Befehl je
  Sprache überschreibbar; einer pro Projektwurzel und Sprache, gestartet mit der ersten
  Datei; fehlt einer, ein ruhiger Hinweis. Reihenfolge: Diagnosen → Hover → Definition →
  Vervollständigung → Formatieren/Umbenennen.
- **D19 — Ausgesprochene Annahmen:** „Neue Notiz" bleibt inhaltlich wie heute (Agent
  schlägt den Bereich vor), nur das Eingabefeld wird zum Editor. Undo ist in v1 linear.
  Mehrere Cursor kommen mit Suchen/Ersetzen in ED5. Sehr große Dateien öffnen
  schreibgeschützt, Binärdateien werden wie heute abgelehnt. Die eigenständige App (ED7)
  ist eine eigene Tauri-Hülle um D1 und D2, wie beim Terminal. KI-Funktionen im Editor
  sind nicht Teil dieses Plans.

## 5. Meilensteinkette

- **ED0 — Datei-Dienst:** `axiomata-files` mit erlaubten Wurzeln (D5) und
  Öffnen-Dialog, lesen/schreiben/beobachten; die heutigen Workspace-Befehle laufen
  darüber. Dazu der Ring-Eintragstyp „Ansicht öffnen" samt Icons für Editor und IDE (D13).
- **ED1 — Editor-Kern:** Puffer, Cursor, Auswahl, Undo, normale Tastenbelegung,
  virtualisierte DOM-Darstellung, Zeilennummern abs./rel./hybrid, Editor-Einstellungen
  mit Live-Vorschau und allen echten Schnitten (D6, D7), Speichern und Wiederherstellen
  (D9), externe Änderungen (D10).
- **ED2 — Aussehen:** tree-sitter (D4), Themes, die Hingucker aus D8, Markdown-Vorschau
  (D11).
- **(M7.3 CP8/CP9** auf dem Editor — D15.)
- **ED3 — Vi-Modus** (D17) samt Modus-Pille.
- **ED4 — Single Point of Truth:** FileViewer, Neue Notiz, Second-Brain-Vorschau und
  IDE-Pane benutzen den Editor, `md-file`s `<textarea>` verschwindet; Vollbild-Ansicht
  mit Dateibaum, Schnellöffnen und Tabs (D12).
- **ED5 — Werkzeuge:** Suchen/Ersetzen, mehrere Cursor, Faltung, Minimap, Sticky Scroll,
  installierte Mac-Schriften.
- **ED6 — LSP** (D18).
- **ED7 — Herauslösung** als eigenständige App.

Jeder Meilenstein wird vor seinem Start in Checkpoints zerlegt und gegrillt, wie bisher.

## 6. Verifikation (pro Meilenstein)

- Das TS-Paket ist von ED1 an ohne DOM testbar (`vitest`): Puffer, Undo, Cursor, später
  der komplette Vi-Automat als Tabelle „Tasten → erwarteter Text und Cursor".
- `axiomata-files` mit echten temporären Verzeichnissen getestet, besonders die
  Wurzel-Prüfung (Pfad-Ausbruch, Symlinks) — `security-auditor` ist ab ED0 Pflicht.
- Browser-Weg über `devmock.ts`, Live-Test am echten Mac für alles, was mit Gefühl zu
  tun hat (Tippen, Eingabemethode, Cursor-Animation, Scrollen) — ohne Oberflächen-
  Änderungen während des Tests.
