# Plan: Die Datei-App — ein eigener Editor als Single Point of Truth

Status: **ED0, ED1 und ED2 fertig** (2026-09-23/24; ED2 wartet auf die Farbabnahme und den
Live-Test); **M7.3 CP8/CP9 auf dem Editor fertig** (2026-09-25, `git-layer.md`); **ED3 (Vi)
(V1–V12) fertig** (2026-09-25: Automat, Anbindung, Suche/Ex, tree-sitter-Textobjekte und
`editor-vi.json`); wartet auf den Live-Test. **ED4 (Single Point of Truth) gegrillt (W1–W17), in
Arbeit.**

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
  an/aus, Zwischenablage, Autospeichern. (Die LSP-Befehle je Sprache stehen seit ED6 nicht hier, sondern
  in `~/.axiomata/lsp.json`, die nur Rust liest — L2.)
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

### ED0 im Detail (gegrillt 2026-09-23, Q1–Q17, bestätigt; Umsetzung begonnen)

**Entscheidungen**

- **E1 — Datei-Identität** (Q1): Über die IPC kommt nie ein absoluter Pfad, der etwas
  freigibt, sondern `{ root, rel }`. `root` ist `workspace`, `project:<id>`,
  `worktree:<agent-id>` oder `grant:<id>`; absolute Pfade gibt Rust nur zur Anzeige zurück.
- **E2 — Link-Regeln je Wurzel** (Q2): Der Workspace bleibt streng (kein Symlink, kein
  Hardlink). Projekt-, Worktree- und Grant-Wurzeln erlauben Symlinks, deren Ziel in
  derselben Wurzel liegt, und Hardlinks (pnpm). Wer nach außen zeigt, wird abgelehnt.
- **E3 — Größen** (Q3): Lesen bis 16 MiB, ab 2 MiB mit `large: true` (Editor öffnet
  schreibgeschützt), Schreiben bis 2 MiB. Die alten Workspace-Befehle behalten 1 MiB.
- **E4 — Version** (Q4): Jedes Lesen liefert eine Version (Länge + FNV-1a-Hash des
  Inhalts); Schreiben nimmt sie optional mit und scheitert mit `Conflict`, wenn die
  Datei sich inzwischen geändert hat. Grundlage für D10.
- **E5 — Beobachten** (Q5, Q15): `notify` (FSEvents), in ED0 nur abonnierte Einzeldateien;
  entprellt (~150 ms) zu `files:changed { root, rel, kind: modified|deleted|created,
  version }`, ein Umbenennen-über (atomares Schreiben) zählt als `modified`. Bäume in ED4.
- **E6 — Öffnen-Dialog** (Q6): `tauri-plugin-dialog`, aber nur aus Rust aufgerufen
  (`files_pick`); die JS-API des Plugins wird nicht freigeschaltet.
- **E7 — Grants** (Q11, Q12): dauerhaft in `~/.axiomata/file-grants.json`, widerrufbar
  (Liste in den Einstellungen ab ED1, vorher CLI). Eine gewählte Datei gibt nur sich frei,
  ein gewählter Ordner seinen Inhalt; derselbe Pfad nutzt den vorhandenen Grant.
- **E8 — Wurzeln frisch auflösen** (Q13): bei jedem Aufruf über einen `RootResolver`-Trait,
  den die Tauri-Schicht aus Config, DB und Grant-Datei umsetzt; `axiomata-files` kennt core
  nicht. Ein verschwundenes Worktree ist „Wurzel unbekannt".
- **E9 — Fehler** (Q14): die neuen `file_*`-Befehle liefern `{ kind, message }`
  (`Conflict`, `TooLarge`, `NotUtf8`, `NotFound`, `UnknownRoot`, `Refused`, `Io`); die alten
  bleiben beim String.
- **E10 — Bestand** (Q7): `*_workspace_file`/`read_workspace_image` behalten Namen und
  Verhalten, laufen aber über `axiomata-files`; die Suche bleibt in core bis ED4.
- **E11 — CLI** (Q16): `files roots`, `files read <root> <rel>`,
  `files write <root> <rel> [--expect <version>]` (Inhalt von stdin),
  `files grants list|revoke <id>`.
- **E12 — Ring** (Q8–Q10): Eintragstyp „Ansicht öffnen" mit Kennung `view:<name>`,
  ausblendbar und gruppierbar wie interne Module. ED0 bringt `view:ide` (`code_blocks`);
  `view:editor` (`edit_document`) kommt mit ED1s minimaler Vollbild-Ansicht. Der
  IDE-Knopf in der IconBar bleibt.

**Checkpoints**

- **ED0.1** — Crate `axiomata-files`: Wurzel-Prüfung (E2), lesen/schreiben mit Version
  (E3, E4), löschen, Bild; `core::workspace` delegiert daran (E10). Danach
  `security-auditor`. **Erledigt 2026-09-23.** Der Audit fand ein TOCTOU-Fenster
  (Ordner zwischen Prüfung und Zugriff gegen einen Symlink getauscht — gab es schon im
  alten `workspace::resolve`); geschlossen, indem jede Aktion den Elternordner per
  `openat(O_NOFOLLOW)` vom Wurzel-Deskriptor aus festhält und relativ dazu liest,
  umbenennt oder löscht (`pinned.rs`, über `rustix`, kein eigenes `unsafe`).
- **ED0.2** — `RootResolver` (E8), Grants (E7), `files_pick` (E6), `file_*`-Befehle (E9),
  `devmock`, CLI (E11). Danach `rust-dependency-auditor`, `security-auditor`.
  **Erledigt 2026-09-23.** Abhängigkeiten sauber (keine Advisories, `rustix` nicht
  doppelt). Der Audit fand einen mittleren Befund: Ein gewählter Ordner bekam immer einen
  neuen `Contained`-Grant, auch wenn er schon im strengen Workspace lag. Jetzt läuft auch
  ein Ordner über `locate` und kommt unter der vorhandenen Wurzel zurück. Zusätzlich zu
  E11: `files grants add <path>` zum Testen ohne Dialog.
- **ED0.3** — Beobachten (E5). **Erledigt 2026-09-23** (`watch.rs`, `notify` 8,
  `file_watch`/`file_unwatch`, Ereignis `files:changed`, Abos fallen beim Neuladen der
  Seite weg). Die Performance-Durchsicht fand zwei HIGH-Befunde, beide behoben: Lesen und
  Hashen liefen unter der Zustands-Sperre, und `file_watch` blockierte die async-Laufzeit.
  Offen, weil für ein paar offene Dateien unnötig: ein schnellerer Hash als FNV-1a für
  große Dateien und eine `stat`-Vorprüfung, damit ein Ordner-Ereignis nicht alle
  Nachbar-Abos neu hasht. Nachziehen, falls ED1 bei großen Dateien stockt.
- **ED0.4** — Ring-Eintragstyp und IDE-Icon (E12). **Erledigt 2026-09-23** (`RING_VIEWS` in
  `core/apps.ts`, Glyph `code-blocks` = `code_blocks` U+F84D; `edit_document` ist U+F88C für ED1).
  `architecture-reviewer`: nichts Kritisches oder Hohes. Mitgenommen für ED4:
  `Roots::locate` löst jede Wurzel doppelt auf (`list()` und dann `root()`). Beim
  Dateibaum, der oft nachschlägt, soll `list()` die `Root`s gleich mitliefern. Abschluss: `architecture-reviewer`,
  `docs/architecture.md`, Commit.

### ED1 im Detail (gegrillt 2026-09-24, Q1–Q18, bestätigt; Umsetzung begonnen)

**Entscheidungen**

- **F1 — Puffer** (Q1, Q15): jetzt eine Zeilenliste hinter einer schmalen Schnittstelle
  (`lineCount`, `line`, `replace`, `slice`). **Ein Rope wird Pflicht** und kommt als
  erster Checkpoint von ED5; die ED1-Tests laufen dann unverändert gegen ihn.
- **F2 — Eingabe** (Q2): ein verstecktes `<textarea>` folgt dem Cursor und nimmt Tippen,
  tote Tasten, IME und Einfügen an; die Zeilen sind reine Anzeige, das Modell ist die
  einzige Wahrheit.
- **F3 — Positionen** (Q3): `{ line, col }` in UTF-16 gespeichert; Bewegen, Löschen und
  Wortwahl über Grapheme (`Intl.Segmenter`); die Statuszeile zählt Grapheme mit Tabs.
- **F4 — Undo** (Q4): linear, unbegrenzt pro geöffneter Datei. Tippen bleibt ein Schritt
  bis 1 s Pause, Cursor-Sprung oder Wechsel Tippen/Löschen; Einfügen, Ausschneiden,
  Einrücken, Zeile verschieben je ein Schritt; Undo stellt die Auswahl wieder her.
- **F5 — Tasten** (Q5): Pfeile (+⌥ Wort, +⌘ Zeile/Datei), Pos1/Ende, Bild↑/↓, alles mit ⇧
  als Auswahl, ⌘A; ⌫/⌦ (+⌥ Wort, +⌘ bis Zeilenanfang); ⌘Z/⇧⌘Z; ⌘X/C/V (ohne Auswahl die
  ganze Zeile); ⇥/⇧⇥; ↩ mit Einrückung; ⌥↑/↓ verschieben, ⇧⌥↑/↓ duplizieren; ⌘/ Kommentar;
  ⌘L Zeile wählen; ⌘S, ⌘O. Nicht in ED1: ⌘F, mehrere Cursor (ED5), Klammer-Paare (ED2).
- **F6 — Umbruch** (Q6, Q16): schon in ED1, an für `.md`/`.txt`, aus für Code, ⌥Z schaltet.
  Nummer nur an der ersten sichtbaren Zeile, relative Nummern zählen logische Zeilen,
  ↑/↓ wandern durch sichtbare Zeilen, ⌘←/→ erst sichtbare, dann logische Zeile; Umbruch an
  Wortgrenzen, Fortsetzungen behalten die Einrückung.
- **F7 — Ansicht** (Q7): eine Datei zur Zeit; Kopfzeile mit Wurzel/Pfad, Ungespeichert-Punkt,
  Öffnen… (⌘O), Zuletzt-Liste, Zahnrad, Zurück zum OS; Statuszeile (Zeile:Spalte,
  Zeilenende, Einrückung, schreibgeschützt). Versteckt statt abgebaut. Ring-Icon
  `view:editor` (`edit_document`, U+F88C).
- **F8 — Wiederherstellung** (Q8): 2 s nach der letzten Änderung nach
  `~/.axiomata/editor-recovery/<hash>.json` (Wurzel, Pfad, Basis-Version, Inhalt, Zeit,
  0600); gelöscht beim Speichern oder Verwerfen; Balken *Wiederherstellen/Verwerfen* beim
  Öffnen, mit Hinweis, wenn die Datei sich inzwischen geändert hat; älter als 30 Tage wird
  beim Start aufgeräumt.
- **F9 — Autospeichern** (Q9): aus (Vorgabe) / nach 1 s Pause / beim Verlassen; immer mit
  erwarteter Version, bei Konflikt nie überschreiben, sondern der Balken aus F10.
- **F10 — Externe Änderungen** (Q10): sauber → still neu laden mit Hinweis; mit eigenen
  Änderungen ein Balken *Neu laden* (Rückfrage) / *Meine behalten* (nächstes ⌘S fragt) /
  *Unterschied ansehen* (bis CP8: die Platten-Fassung schreibgeschützt daneben). Gelöscht:
  *Schließen* / *Neu anlegen*.
- **F11 — Einrückung, Zeilenenden** (Q11): aus der Datei erkannt (erste 200 Zeilen), sonst
  Einstellung (4 Leerzeichen, Tab-Breite 4); LF/CRLF beibehalten, gemischt → LF mit Hinweis;
  fehlender Schluss-Umbruch bleibt fehlend.
- **F12 — Einstellungen** (Q12, Q18): `editor-settings.json` über `core::json_state`: Modus
  (Vi ausgegraut bis ED3), Schrift, Schnitt, Größe, Zeilenhöhe, Ligaturen, Zeilennummern,
  Umbruch-Vorgaben, Einrückung/Tab-Breite, Autospeichern. Live-Vorschau ist ein echter
  schreibgeschützter Editor. Vorgaben: JetBrains Mono Regular 14 px, Zeilenhöhe 1,5,
  Ligaturen an, Zeilennummern hybrid, Umbruch wie F6, 4 Leerzeichen, Autospeichern aus.
- **F13 — Schnitte** (Q13, Q17): alle Schnitte jedes Pakets, erst bei Bedarf geladen
  (umgesetzt als ein wörtliches `import()` je Schnitt statt `import.meta.glob`: typgeprüft,
  und Vite baut trotzdem jeden Schnitt als eigenes Stück); der Regler zeigt nur echte Schnitte mit Namen. Hat eine Schrift den
  gewählten Schnitt nicht, bleibt er gespeichert und der nächstliegende echte wird gezeigt
  (mit Hinweis) — nie ein künstlicher. Das Terminal behält seine drei festen Schnitte.

**Checkpoints** (Q14)

- **ED1.1** — Modell: Puffer, Positionen, Auswahl, Befehle, Tastenbelegung, Undo,
  Einrückung/Zeilenenden erkennen. Reines TS in `src/editor/`, alles mit `vitest`.
  **Erledigt 2026-09-24** (73 Tests). Zwei Erkenntnisse: Wortgrenzen laufen über eigene
  Zeichenklassen, weil `Intl.Segmenter` nach Prosa-Regeln `bar.baz` als ein Wort sieht.
  Und der Puffer hält den Schluss-Umbruch nie, `joinForSave` hängt ihn immer an. Beides
  ist in den Kopfkommentaren begründet. ↑/↓ und ⌘←/→ fragen schon ein `RowLayout`, sodass
  ED1.2 den Umbruch liefert, ohne die Befehle anzufassen.
- **ED1.2** — Darstellung: virtualisierte Zeilen mit Umbruch, textarea-Eingabe, Cursor und
  Auswahl, Gutter, Maus (Klick, Ziehen, Doppel-/Dreifachklick). Browser über `devmock`.
  **Erledigt 2026-09-24.** Die reine Rechnung liegt in `src/editor/` und ist getestet:
  `wrap.ts` (Umbruch), `visual.ts` (sichtbare Zeilen mit einem Cache nach Zeilentext),
  `geometry.ts` (Cursor, Auswahl-Streifen, Klick zu Position) und `gutter.ts`. Das
  Svelte-Bauteil ist `src/fileapp/EditorSurface.svelte`. Die Geometrie ist reine Rechnung
  mit einer einmal gemessenen Zeichenbreite, alle Schriften sind Monospace; CJK und Emoji
  zählen zwei Zellen. Die Zwischenablage läuft über die nativen `copy`/`cut`/`paste`-Ereignisse
  und braucht deshalb keine Berechtigung. Im Browser geprüft über `__ax.editorDemo()` (nur
  DEV, verschwindet mit ED1.3): Rendern, Tippen, Auswahl, Ziehen, Doppelklick, ⌥Z, Scrollen
  (54 Zeilen im DOM statt 212). Eingabemethode, tote Tasten und die echte Zwischenablage
  prüft erst der Live-Test.
- **ED1.3** — Vollbild-Ansicht und Dateien: `view:editor`, Öffnen, Speichern, Autospeichern,
  externe Änderungen, Wiederherstellung (Rust-Befehle dazu), große Dateien schreibgeschützt.
  **Erledigt 2026-09-24**, bis auf das Autospeichern (F9). Das kommt mit seiner Einstellung
  in ED1.4.
  - `src/fileapp/`: `session.ts` enthält alle Datei-Abläufe aus F8–F10 und ist mit einem
    Fake-Backend getestet (14 Tests). `FileAppView.svelte` ist die Vollbild-Ansicht, versteckt
    statt abgebaut wie die IDE. Die Zuletzt-Liste liegt in `recent.ts`, gespeichert unter
    `settings.editor.recent` in `dashboard.json`.
  - Rust: `core::editor_recovery` mit den drei `editor_recovery_*`-Befehlen; das Aufräumen
    läuft beim Start im Hintergrund.
  - `listenBackend` ist neu in `core/backend.ts`, der erste Tauri-Ereignis-Abonnent, mit
    Mock-Ereignissen für den Browser (`__ax.mockExternalWrite`).
  - Ring: `view:editor` (Symbol `edit_document`). Das DEV-Vorführfenster aus ED1.2 ist entfernt.
  - Oberflächentexte auf Englisch wie im Rest der App (IDE: „Back to the OS“), also „Reload /
    Keep mine / Compare“ statt der deutschen Arbeitsnamen aus F10.
  - Im Browser geprüft: Öffnen, Tippen, externe Änderung mit Balken, Vergleich, Keep mine
    mit Rückfrage und Überschreiben, stilles Neuladen mit Hinweis, Löschen. Dabei gefunden
    und behoben: Die Oberfläche merkte ein Neuladen von außen nicht (neue Eigenschaft
    `revision`), und ⌘S hätte doppelt gespeichert.
  - `security-auditor`: ein mittlerer Befund, behoben. Die Zahl der Recovery-Einträge war
    unbegrenzt; jetzt sind es höchstens 256, und ein neuer Eintrag verdrängt den ältesten.
    Das Aufräumen erfasst jetzt auch liegengebliebene `*.axiomata-tmp`-Dateien.
- **ED1.4** — Einstellungen: `editor-settings.json`, Zahnrad, Live-Vorschau, alle Schnitte.
  **Erledigt 2026-09-24.**
  - Rust: `core::editor_settings` mit `get/save_editor_settings`, dasselbe
    `json_state`-Muster wie beim Terminal.
  - Frontend: `fileapp/editorSettings.ts` bereinigt Feld für Feld und fällt auf die Vorgaben
    aus F12 zurück. `fonts.ts` listet alle echten Schnitte; die Lader sind wörtliche
    `import()`-Aufrufe, Vite baut also jeden Schnitt als eigenes Stück.
  - `EditorSettingsPanel.svelte` mit echter, schreibgeschützter Vorschau.
  - Das Autospeichern (F9) ist hier mit eingebaut: nach 1 s Pause oder beim Verlassen
    (Ansicht zu, Fenster verliert den Fokus, Wechsel zu einer anderen Datei).
  - Die Ansicht lädt einen Schnitt, bevor die Oberfläche ihn bekommt, sonst würde sie an
    der Ersatzschrift messen.
  - Im Browser geprüft: Vorschau mit Ligaturen, der Hinweis bei fehlendem Schnitt (Space Mono
    Thin → Regular 400), ExtraLight 200 nachgeladen, Autospeichern nach 1 s.
- **Abschluss** — Live-Test mit dem Owner am Mac (Tippen, Umlaute, IME, Scrollen; keine
  UI-Änderungen währenddessen), Prüfer, Commit.
  Die Prüfer sind durch (2026-09-24). `architecture-reviewer`: nichts Kritisches oder Hohes.
  Ein mittlerer Punkt ist behoben: `onMount` der Oberfläche setzte die Auswahl, ohne neu zu
  zeichnen, und hätte eine vorgewählte Auswahl (CP8) zerstört; die Zeile ist entfernt.
  Kleinigkeiten ebenfalls behoben: neutraler Typ `LoadedJsonState` statt des Terminal-Typs,
  `sessionTick` statt `tick` in der Ansicht, Zeilen über 120 Zeichen. `refactoring-specialist`:
  drei kleine Doppelungen herausgezogen. Für ED7 notiert: `src/editor/` ist ohne Änderung
  herauslösbar; `src/fileapp/` hängt an drei App-Stellen (`invokeBackend`, `getSetting` aus
  `dashboard.json`, Toasts), die eine eigenständige App ersetzen muss. Der Commit folgt
  vor dem Live-Test; Befunde aus dem Test kommen als eigener Commit.
  **Live-Test 2026-09-24** (Owner, ZSA Voyager mit Modifikatoren als Halte-Tasten): das
  meiste funktioniert. Befund: ⌥Z ist auf dem Owner-Layout eine tote Taste (`¨`), meldet also
  `key = "Dead"`, und das hängengebliebene Kompositionsfeld zeigte ein nicht löschbares `¨` am
  Cursor. Behoben: ⌥-Kürzel werden zusätzlich über `keyCode` erkannt (WebKit meldet die
  Grundtaste des Layouts), eine Komposition endet auch ohne `compositionend` bzw. beim
  Fokusverlust, und die Vorschau in den Einstellungen ist höher.

### ED2 im Detail (gegrillt 2026-09-24, Q1–Q15, bestätigt; Umsetzung begonnen)

**Entscheidungen**

- **G1 — Grammatiken selbst gebaut** (Q1, Q14): `scripts/build-grammars.sh` holt jede
  Grammatik auf einem festen Git-Tag und baut sie mit `tree-sitter-cli` 0.27 zu WASM
  (die CLI lädt WASI-SDK und `wasm-opt` selbst nach, kein emscripten, kein Docker).
  Quellen: `tree-sitter` (Rust, TypeScript/TSX, JavaScript, JSON, CSS, HTML, Python, Bash),
  `tree-sitter-grammars` (Markdown, TOML, YAML, Lua, Svelte), `alex-pinkus` (Swift),
  `DerekStride` (SQL). Lizenzhinweise liegen daneben.
- **G2 — Sprachen** (Q2): die 15 oben; alles andere ist Klartext.
- **G3 — CSP** (Q3): `'wasm-unsafe-eval'` in `script-src`, nur WebAssembly, kein JS-`eval`.
- **G4 — Parsen im Haupt-Thread, inkrementell** (Q4); das erste Parsen einer großen Datei
  zeigt bis dahin ungefärbten Text; über 2 MiB (schreibgeschützt) keine Hervorhebung. Ein
  Worker folgt nur, wenn der Live-Test ruckelt.
- **G5 — 17 Syntax-Token** (Q5): `--ax-syntax-{keyword,string,number,comment,function,type,
  variable,constant,property,operator,punctuation,tag,attribute,heading,link,emphasis,code}`,
  Fangnamen fallen auf ihren Oberbegriff zurück (`function.method` → `function`).
- **G6 — Farben** (Q6): von Claude aus jeder Theme-Palette abgeleitet (Orange bleibt
  Akzent) und dem Owner per Screenshot aller fünf Themes zur Abnahme gezeigt.
- **G7 — Hingucker** (Q7, Q8): gleitender Cursor mit Spur (Canvas-Ebene nach D3; aus /
  gleiten / gleiten mit Spur, Vorgabe mit Spur), sanftes Scrollen bei Sprüngen, aktuelle
  Zeile, Einrückungslinien, farbige Klammerpaare, dezentes Akzent-Leuchten, schönere
  Zeilennummern; jeder mit Schalter im Zahnrad; „Bewegung reduzieren" von macOS schaltet
  Animation und sanftes Scrollen ab.
- **G8 — Markdown-Vorschau** (Q9): ⌘⇧V schaltet Quelltext / Vorschau / nebeneinander;
  Renderer ist `core/markdown.ts`; relative Bilder über den Dateidienst mit der Wurzel der
  Datei; synchroner Bildlauf über `data-line`-Marken je Block.
- **G9 — Ablage** (Q11): gebaute Grammatiken und Abfragen eingecheckt unter
  `apps/dashboard/public/grammars/`, geladen per `fetch` erst beim ersten Bedarf.
- **G10 — Eingebettete Sprachen** (Q12): `injections.scm` in ED2 (Markdown-Codeblöcke,
  `<script>`/`<style>` in Svelte und HTML).
- **G11 — Spracherkennung** (Q13): Endung, einige Dateinamen, sonst Shebang; ein manueller
  Umschalter kommt mit ED4.
- **G12 — Abfragen** (Q15): die `highlights.scm` der Grammatiken selbst plus je Sprache eine
  kleine eigene Ergänzung, wo etwas fehlt; keine nvim-treesitter-Abfragen (Lua-Prädikate).

**Checkpoints** (Q10)

- **ED2.1** — Grammatik-Skript, gebaute Grammatiken, CSP, tree-sitter laden, inkrementelles
  Parsen (Test: Baum und Text stimmen nach jeder Änderung überein). **Erledigt 2026-09-24.**
  17 Grammatiken (die 15 Sprachen plus TSX und Markdown-Inline), zusammen 13 MB statt der
  geschätzten 5–10; die größten sind Swift, SQL und TypeScript. `web-tree-sitter` 0.27 zählt
  Spalten in UTF-16 wie der Editor, es wird also nichts umgerechnet. Gefunden: `node.text`
  liest über den Parse-Rückruf mit veralteter Position. Deshalb schneidet der Highlighter
  Texte aus dem Puffer, und der Test vergleicht den nachgeführten Baum mit einem frisch
  geparsten. `EditorDocument.onTextChange` meldet jede Änderung; `TextStore.offsetAt` ist neu.
- **ED2.2** — Hervorhebung und die Farben aller fünf Themes (Screenshots zur Abnahme).
  **Erledigt 2026-09-24**, die Abnahme durch den Owner steht noch aus.
  - TypeScript nutzt die JavaScript-Abfragen plus die eigenen, Svelte die HTML-Abfragen plus
    die eigenen (deren `; inherits:` ist eine nvim-Eigenheit).
  - Eingebettete Sprachen werden getrennt geparst und nach Text zwischengespeichert.
  - Die Token-Tabelle ist eine `Map`, weil `"constructor" in {}` über den Prototyp wahr ist.
  - Nur Farbe, kein Fett oder Kursiv, damit der Browser keinen Schnitt vortäuscht (F13).
  - Hervorhebung hat pro Theme einen eigenen Ton.
  - `theme/validator.ts` kennt die neuen Token.
- **ED2.3** — Hingucker samt Schaltern. **Erledigt 2026-09-24.**
  - Das Gleiten des Cursors zeichnet eine Canvas-Ebene nur während der Bewegung, im
    Ruhezustand blinkt der DOM-Cursor. Gerechnet wird in Dokument-Koordinaten, damit beim
    Scrollen nichts nachgleitet.
  - Klammerfarben ergeben sich aus der Tiefe im Syntaxbaum, Klammern in Strings stören also
    nicht.
  - Leere Zeilen übernehmen die Einrückungslinien ihrer Nachbarn.
  - Sanftes Scrollen erst ab drei Zeilen Sprung.
  - „Bewegung reduzieren“ schaltet Gleiten und sanftes Scrollen ab.
  - Die aktuelle Zeilennummer ist in Akzentfarbe, mit etwas mehr Abstand zum Text.
- **ED2.4** — Markdown-Vorschau. **Erledigt 2026-09-24.**
  - `renderMarkdownBlocks` markiert jeden Block mit `data-line`. Dafür gibt es eine zweite
    DOMPurify-Instanz, die nur `data-line` durchlässt.
  - Die Stile liegen gemeinsam mit dem FileViewer in `core/markdown-prose.css`.
  - Bilder werden über `file_read_image` mit der Wurzel der Notiz gelesen.
  - ⌘⇧V schaltet Quelltext / Vorschau / nebeneinander; der synchrone Bildlauf hat eine kurze
    Echo-Sperre.
- **Abschluss** — Live-Test, Prüfer, Commit.
  Die Prüfer sind durch (2026-09-24).
  - `architecture-reviewer`: nichts Kritisches oder Hohes; die D1-Grenze hält. Der mittlere
    Punkt ist umgesetzt: Das Gleiten des Cursors ist aus der Oberfläche nach
    `fileapp/cursorGlide.ts` gezogen, bevor ED3 dort weiter anbaut.
  - `security-auditor`: Die CSP-Freigabe ist eng, und der Sanitizer hält alles, was er vorher
    hielt (vom Prüfer empirisch nachgeprüft). Mittlerer Befund, behoben: `data-line` war auf
    jedem Element erlaubt, eine Notiz konnte also eine Zeilenmarke fälschen. Jetzt wird jeder
    Block normal bereinigt und erst danach vom Code eingerahmt; die zweite DOMPurify-Instanz
    ist weg. Kleiner Befund, behoben: Tags lassen sich verschieben, deshalb prüft
    `build-grammars.sh` den Commit jeder Grammatik und bricht bei Abweichung ab.
  - ~~Offene Kleinigkeit für CP8: zwei verschiedene Echo-Sperren beim synchronen Bildlauf~~ —
    mit M7.3 CP8 erledigt: eine gemeinsame `fileapp/scrollLink.ts` für Markdown-Vorschau und
    die Diff-Ansicht nebeneinander (git-layer.md, H12).

### ED3 im Detail (gegrillt 2026-09-25, Q1–Q12, bestätigt; Umsetzung begonnen)

Die Entscheidungen heißen **V** (D bis G und H sind vergeben).

- **V1 — Vi überall, wo der Editor ist** (Q1): Datei-App, Datei-Pane und die schreibgeschützten
  Flächen (Diff, Compare) — dort nur Bewegen, Visual, Yank, Suche, `]c`/`[c` (nächster/voriger
  Hunk, H9) und `gf`/⏎ (öffnen).
- **V2 — Visual-Block als eigene Auswahlart** (Q2) `{anchor, head, block}`: als Rechteck
  gezeichnet, `d c y r I A > <` zeilenweise in einem Undo-Schritt; `I`/`A` tippen in die erste
  Zeile und verteilen den Text beim Esc, wie Vim. Echte Mehrfach-Cursor bleiben ED5.
- **V3 — Mac-Zwischenablage über `pbcopy`/`pbpaste`** (Q3) als eigene Rust-Befehle in
  `axiomata-macos`, lesen und schreiben — ohne neue Abhängigkeit, ohne Paste-Popup.
- **V4 — Register und Gedächtnis** (Q4): `"` (= Mac-Zwischenablage, solange geteilt), `"0`–`"9`,
  `"-`, `"a`–`"z` (Großbuchstabe hängt an), `"_`, `"+`/`"*` (immer Mac), lesbar `". "% ": "/`.
  Über einen Neustart bleiben benannte Register (damit Makros), Datei-Marken `A`–`Z` und die
  Such-/Befehlshistorie, in `~/.axiomata/editor-vi.json`.
- **V5 — Tasten** (Q5): ⌘-Kombinationen behalten ihre Mac-Bedeutung (⌘S ⌘O ⌘C/⌘X/⌘V auf der
  Visual-Auswahl, ⌘Z/⌘⇧Z = `u`/Ctrl-r, ⌘/, ⌘⇧V); Ctrl gehört Vi (r v d u f b e y o i a x [).
  Insert-Modus = die ganze Mac-Belegung aus ED1 plus Ctrl-w/u, Ctrl-r {Register},
  Ctrl-o {Befehl}, Ctrl-t/d. Ein Insert-Durchgang ist ein Undo-Schritt.
- **V6 — Ex** (Q6): `:w :q` (Pane schließen / Vollbild verlassen) `:wq :x :q! :e! :{n}`,
  `:s`/`:%s` mit Bereichen `'<,'>` und `n,m` und Flags `g i I`, `:noh`, `:set wrap nu rnu list`;
  Historie und Tab-Ergänzung. Nicht: `:g`, `:normal`, Flag `c`.
- **V7 — Suche** (Q7): JavaScript-RegExp plus übersetzte Vim-Atome (`\<` `\>` `\c` `\C`,
  smartcase), incsearch und hlsearch. Dieselbe Suche trägt später ED5.
- **V8 — Modus-Pille und Cursor** (Q8): Block in Normal/Visual, Balken in Insert, Unterstrich in
  Replace; Pille in der Statuszeile mit Aufnahme („● REC q"), angefangener Eingabe und der
  `:`-Zeile; Token `--ax-vi-normal/insert/visual/replace`, Normal im Akzent-Orange.
- **V9 — Textobjekte** (Q9): klassisch `w W s p " ' \` ( [ { < t`, dazu über tree-sitter
  `if/af` (Funktion), `ic/ac` (Klasse/Struct/Impl/Interface), `ia/aa` (Argument/Parameter) —
  eigene kleine Abfragen pro Sprache, wo es passt.
- **V10 — Checkpoints** (Q10): **ED3.1** Automat im Kern (`editor/vi/`, Tabellen-Tests) ·
  **ED3.2** Anbindung (Tasten, Cursor-Formen, Block-Rechteck, Pille, Einstellung frei, Diff/
  Compare, Zwischenablage) · **ED3.3** Suche und Ex · **ED3.4** tree-sitter-Textobjekte,
  `editor-vi.json`, Token, Feinschliff. Ein Commit pro Checkpoint.
- **V11 — `gc` und Surround** (Q11): `gcc`/`gc{Bewegung}`/Visual-`gc` über das vorhandene
  Kommentieren; `ys{Objekt}{Zeichen}`, `cs`, `ds`, Visual-`S`.
- **V12 — Tasten als Zeichen** (Q12): Im Normal-Modus liest der Editor Vi-Tasten als Zeichen,
  nicht als Tastencodes — jedes Layout geht; eine tote Taste (`^` `` ` `` `~` im deutschen
  Layout) wirkt sofort (Komposition abbrechen, ihr Zeichen nehmen). Insert bleibt wie heute.

**ED3.1 — gebaut (2026-09-25):** `editor/vi/` — `keys` (Tasten als Zeichen, Sondertasten in
Vim-Notation, getippter Text und aufgelöste Mac-Befehle als eigene Arten), `parse` (Grammatik
`["x][count]` + Operator/Bewegung/Textobjekt/Befehl, liefert die Tasten ohne Zähler für `.`),
`motions`, `textobjects`, `ops` (Löschen, Einfügen, Verschieben, Groß/Klein, `=`-Heuristik,
Verbinden, Kommentieren, Surround, Ctrl-a/x), `registers` (Mac-Zwischenablage als `"`, die ihre
Textart für eigenes Geschriebenes behält), `marks` (Marken, Sprungliste, folgen dem Text),
`regions`, `machine` (Modi, Punkt-Wiederholung, Makros, Visual inkl. Block mit `I`/`A`/`c`/`$`,
Insert/Replace). Eine späte Zwischenablage hält den Befehl an und führt ihn aus, sobald der Text
da ist; währenddessen getippte Tasten warten in der Schlange. Dazu `EditorDocument` mit
Undo-Gruppen (ein Insert-Durchgang = ein Schritt). Getestet als Tabelle „Tasten → Text und
Cursor“ (über 180 Fälle).

Die Prüfungen von ED3.1: Die Maschine war zu groß (Architektur, HIGH) — Insert/Replace liegen
jetzt in `vi/insert.ts`, die Befehls-Schalter sind nach Themen geteilt. Die Undo-Grenzen stimmten
nicht (Architektur HIGH, Tests): eine Pfeiltaste im Insert teilte den Schritt nie, und Ctrl-o
fasste seinen einen Befehl mit dem Getippten davor zusammen — jetzt ist, was vor einer Pfeiltaste
oder vor Ctrl-o getippt wurde, ein Schritt, der eine Befehl ein eigener. Ein Schreiben in ein
Nur-lese-Register (`".yiw`) landete still in `"0` — jetzt verweigert. `gj`/`gk` stellen die
Zielspalte des Dokuments wieder her. Die Testlücken-Prüfung brachte gut 200 weitere Fälle.
**Vorgemerkt:** `ViKey` bekommt eine Art-Kennung (`{ kind: … }`), sobald eine weitere Tastenart
dazukommt — heute unterscheiden `typeof`/`"text" in` die drei Arten.

**ED3.2 — gebaut (2026-09-25):** Die Maschine hängt an jeder Editor-Fläche. `fileapp/viKeys`
entscheidet pro Taste (V5, V12): Zeichen kommen durchs Textfeld, Sondertasten und Ctrl werden
Vi-Tasten, ⌘ bleibt Mac (⌘Z → `u`, ⌘C → `"+y`/`"+yy`, ⌘V → `"+P`, ⌘A → `ggVG`, ⌘/ → `gc`), im
Insert gilt die ganze Mac-Belegung. `fileapp/viSurface` übersetzt zwischen Fläche und Maschine
(getippter Text, tote Tasten über `compositionupdate`, Cursor-Form, Visual-Bereiche — ein Block
als ein Streifen pro Zeile), `fileapp/viScroll` rechnet die sichtbaren Zeilen und `zt zz zb`,
`fileapp/viShared` ist die eine geteilte Register-/Markenbank, die der Einstellung
„Zwischenablage: geteilt/getrennt“ folgt. Die Mac-Zwischenablage läuft über `clipboard_read`/
`clipboard_write` (`axiomata-macos::clipboard`, `pbpaste`/`pbcopy`, UTF-8, höchstens 16 MiB).
Block-, Balken- und Unterstrich-Cursor, die Modus-Pille in der Statuszeile (Token
`--ax-vi-*` in allen Themes), die Glocke als kurzer Rahmen. `EditorSurface` behält Bell und
Scrollen, alles andere reicht sie an den Besitzer: `FileEditor` speichert (`:w`-Effekte,
`ZZ`), schließt (Datei-App verlassen / Pane schließen, nicht bei ungespeicherten Änderungen) und
öffnet Datei-Marken; das Diff springt mit `]c`/`[c` zum Hunk und öffnet mit `gf`/⏎ die Datei.
Die Einstellung „Modus: Vi“ ist frei.

Die Prüfungen von ED3.2: Die Maschine wurde bei jeder Einstellungsänderung (Schriftgröße, ⌥Z)
neu gebaut und verlor Modus, Visual und Aufnahme (Architektur, CRITICAL) — jetzt hängt sie nur
am An/Aus von Vi, und `dispose()` schließt eine offene Undo-Gruppe. ⌘C/⌘X taten im Insert
nichts (HIGH) — jetzt wie am Mac. Scroll-Geometrie aus der Komponente in `viScroll` gezogen.
Sicherheit (MEDIUM): `pbpaste`/`pbcopy` konnten einen Thread für immer halten und das Schreiben
war unbegrenzt — jetzt läuft das Pipe-I/O auf einem Hilfsthread, das Werkzeug wird nach 5 s
beendet und immer abgeräumt, geschrieben werden höchstens 16 MiB; beide werden über ihren
absoluten Pfad gestartet (LOW). Dass das Textfeld in Vi auch auf schreibgeschützten Flächen nicht
`readonly` ist, bleibt so: Die Vi-Tasten kommen dort über das Textfeld (V12), die Maschine sperrt
jede Änderung selbst (dreifach geprüft). **Vorgemerkt (App-weit, nicht ED3):** Die eigenen
Tauri-Befehle haben keine ACL — ohne App-Manifest in `build.rs` prüft Tauri sie für lokale
Aufrufe gar nicht gegen `capabilities/`. Heute erreicht kein Fremdinhalt (srcdoc-iframe,
DOMPurify-Markdown) die IPC; eine eigene Allow-List wäre Tiefenverteidigung für alle Befehle.

**ED3.3 — gebaut (2026-09-25):** Die Befehlszeile lebt **im Automaten**, nicht in der Ansicht:
Ihre Tasten laufen durch dieselbe Schlange wie alle anderen, also spielen Makros `:s/a/b/⏎` und
`.` ein `d/foo⏎` genau nach. `vi/cmdline` ist die eine Zeile (Bearbeiten, ⌥⌫/⌘←, Historie mit
Präfix-Filter, Tab-Ergänzung, Ctrl-r {Register}); `vi/cmdmode` arbeitet mit ihr hinter einem
kleinen Host wie `insert.ts`: incsearch (der Treffer, zu dem die Suche ginge, wird gezeigt und
angesteuert, der Cursor bleibt), hlsearch bis `:noh`, `n N * #` als Bewegungen (auch für
Operatoren: `dn`, `d/x⏎`, `c?y⏎`), `@:`, `&`. `/` und `?` sind jetzt Bewegungen der Grammatik; der
wartende Befehl läuft nach ⏎ mit „letzte Suche“ als Ziel weiter. `vi/search`: JavaScript-RegExp
mit `u`-Flag, `\<` `\>` `\c` `\C`, smartcase, Treffer zeilenweise (ein Muster überspannt keinen
Zeilenumbruch), wrapscan mit Meldung; ein Escape, das der Unicode-Modus ablehnt (`\=`, `\<` in
einer Klasse), meint das Zeichen selbst. `vi/ex`: `:w :q :q! :wq :x :e! :{n} :s :& :&& :noh :set`
mit Bereichen `% . $ n 'm` samt `+n/-n`, Fehlertexte wie Vim (`E486`, `E492` …).
`vi/substitute`: beliebiger Trenner, Flags `g i I &`, Ersetzung mit `& \0–\9 ~ \r \n \t \u \l \U
\L \E`, ein Undo-Schritt, Cursor auf die zuletzt entstandene Zeile. Meldungen stehen in der Pille
bis zur nächsten Taste, Fehler in `--ax-danger` mit Glocke.

Anbindung: `ViStatusLine.svelte` (Pille, Aufnahme, Befehlszeile mit Cursor, Meldung) im Footer
der Datei-App und als schwebende Leiste über Diffs. `:set wrap nu rnu list` gilt nur für diesen
Editor und wird nie gespeichert (`fileapp/viOptions`, wie in Vim; die Einstellungen bleiben beim
Zahnrad); `nonu nornu` blendet die Nummern aus (`GutterMode "off"`), `list` zeigt `→` für Tabs und
`·` für Leerzeichen am Zeilenende. `:e!` verwirft ungespeicherte Änderungen ohne Rückfrage (das `!`
ist sie). Suchtreffer über den `.mark`-Mechanismus, Token `--ax-search-match`,
`--ax-search-current`, `--ax-editor-whitespace` in allen Themes. Während Vi Text tippt (Suche in
einem Diff), nimmt `interceptKey` dem Editor ⏎ nicht mehr weg; `EditorSurface` ist ein eigener
Stapelkontext (`isolation`), damit ihre Ebenen nicht über Overlays des Besitzers liegen.
**Bekannt:** Ein Muster mit katastrophalem Backtracking (`(a+)+$` auf langer Zeile) kann die
Ansicht beim Tippen einfrieren — JavaScript-RegExp kennt keine Zeitgrenze (Vim hat `redrawtime`).

Die Prüfungen von ED3.3: `"%` hing als eine Funktion am geteilten Registerspeicher, und der zuletzt
gebaute Automat gewann für alle — zwei Panes nebeneinander setzten den falschen Dateinamen ein
(Architektur, HIGH); jetzt trägt jeder Automat seinen eigenen ein, bevor er Tasten abarbeitet.
Schreibgeschützte Flächen schluckten `:q`, `ZZ`/`ZQ`, `:e!`, `:set` und fremde Datei-Marken still
(MEDIUM) — jetzt „Not available in a read-only view“ mit Glocke. `:set nu`/`list` blieben beim
Dateiwechsel stehen, `wrap` nicht (LOW) — alle drei gelten jetzt für die Datei, in der sie getippt
wurden. `;` in Bereichen zählt die zweite Adresse von der ersten aus, wie in Vim (die
Testlücken-Prüfung fand, dass es wie `,` wirkte). Refactoring: ein gemeinsames `isWordChar`,
`substituteInLine`, `applySubstitution`, `parseSubstituteCommand`, ein Helfer für Markierungsläufe
in `EditorSurface`. Gut 50 weitere Testfälle.

**ED3.4 — gebaut (2026-09-25):** `editor/syntax/objects.ts` liefert `if/af` (Funktion), `ic/ac`
(Klasse, Struct, Impl, Trait, Interface, Enum, Modul) und `ia/aa` (Argument, Parameter,
Typparameter) aus dem tree-sitter-Baum des Highlighters. Statt einer Abfragedatei je Sprache ist
es **eine Tabelle von Knotentypen** über alle gebündelten Grammatiken — sie teilen sich die
meisten Namen (`function_declaration`, `class_declaration`, `arguments` …); eine Sprache, deren
Typen fehlen, hat das Objekt schlicht nicht. `af` ist der ganze Knoten (ganze Zeilen, wenn er sie
allein belegt), `if` der Rumpf zwischen den Klammern (ganze Zeilen, wenn die Klammern allein
stehen) oder der Block/Ausdruck selbst (Python, `x => x + 1`), `aa` nimmt ein trennendes Komma
mit. Die Oberfläche fragt den Baum erst im Moment der Taste ab (der Highlighter kommt nach ihr).
Diffs haben keinen Baum und damit keine dieser Objekte.

**`~/.axiomata/editor-vi.json`** (V4): benannte Register `a`–`z` (also Makros), Datei-Marken
`A`–`Z`, Befehls- und Suchhistorie (je 100) und die letzte Suche — nach dem Muster von
`editor-settings.json`: Rust (`core::editor_vi`, `get_/save_editor_vi_state`) prüft nur „Objekt mit
Version“ und schreibt atomar mit 0600, das Frontend (`fileapp/viPersist`) prüft Feld für Feld und
verwirft nur, was kaputt ist. Ein Register über 256 KiB wird nicht geschrieben. Gelesen wird beim
ersten Gebrauch von `viShared`, geschrieben eine Sekunde, nachdem die Tasten ruhen — nie bevor
gelesen wurde. Die letzte Suche kommt ohne Hervorhebung zurück (wie nach `:noh`), `n` findet sie.

Nebenbei gefunden: Visual `vip` (und jetzt `vif`) ließ die letzte Zeile eines zeilenweisen
Objekts weg — seit ED3.1; behoben.

Die Prüfungen von ED3.4: Swift hängt Parameter ohne Listenknoten direkt an die Funktion, `ia`/`aa`
griff dort nie (Testlücken-Prüfung) — jetzt zählt ein bloßer `parameter` einer Funktion mit; eine
Swift-Closure hat kein `body`-Feld, ihr Inneres sind die `statements` (Architektur, MEDIUM). Kam
ein Speichern vor dem ersten Lesen der Datei, ging die Änderung verloren (MEDIUM) — jetzt wird es
neu eingeplant, ein gescheitertes Speichern meldet sich als Toast. Sicherheit (LOW, beide
übernommen): alle Register zusammen höchstens 2 MiB und Historien-Einträge höchstens 4 KiB, damit
die Datei nie über die 4 MiB wächst, die `json_state` beim Lesen annimmt (sonst würde sie beim
nächsten Start als kaputt beiseitegelegt); dieselben Grenzen beim Lesen. Eine manipulierte
Datei-Marke öffnet nur, was der Datei-Dienst ohnehin erlaubt (`axiomata-files`). Die Knotentypen
sind jetzt für alle Sprachen mit Funktionen getestet (Rust, JS, TS, Python, Lua, Bash, Swift).

**ED3 ist damit fertig.** Offen für den Owner: Live-Test von Vi in der echten App (tote Tasten, die
Mac-Zwischenablage, `:w`/`:q` in Datei-App und IDE-Pane, Suche in einem Diff).

### ED4 im Detail (gegrillt 2026-09-25, Q1–Q17, bestätigt; Umsetzung begonnen)

Die Entscheidungen heißen **W**.

- **W1 — Womit eine Datei öffnet** (Q1): nach Herkunft. Second Brain, Chat und Agent zeigen
  Markdown (und HTML) in der **Vorschau**, `/newfile` und „Bearbeiten“ im Editor; Code immer im
  Editor. ⌘⇧V schaltet überall um.
- **W2 — Die Kachel „Document“ entfällt** (Q2): ein neuer, schlanker Panel-Typ um `FileEditor`;
  `md-file.svelte` wird gelöscht, die Agenten-Aktion `open` öffnet das Panel.
- **W3 — HTML, SVG, Bilder** (Q3): HTML-Vorschau ist das sandboxed `srcdoc`-iframe von heute (mit
  der Link-Navigation zwischen Lektionen), bearbeitet wird im Editor; SVG öffnet als Quelltext mit
  Vorschau per ⌘⇧V (als `<img>` aus einer `data:`-URL, kein Skript); Bilder bekommen eine reine
  Bildansicht (Einpassen/100 %, Abmessungen, Größe).
- **W4 — Neue Notiz** (Q4): ein unbenannter Puffer; ⌘S/`:w` ruft `create_note` (der Agent wählt
  den Bereich, D19), danach zeigt die Ansicht auf die geschriebene Datei. Bis dahin sichert die
  Wiederherstellung den Entwurf; Schließen mit Inhalt fragt „Verwerfen?“. Auch in der
  Vollbild-Ansicht über ⌘N.
- **W5 — Second-Brain-Vorschau** (Q5): gerendertes Markdown (`MarkdownPreview`), Code als
  schreibgeschützte Editorfläche mit Farben; „Open“ öffnet das Panel.
- **W6 — Dateibaum** (Q6): Wurzeln Workspace, IDE-Projekte, Dialog-Freigaben (keine Worktrees);
  neuer Befehl `file_list(root, rel)` für genau einen Ordner, durch den Schutz des Datei-Dienstes,
  nachgeladen beim Aufklappen; `.git`, `node_modules`, `target` und Punkt-Dateien ausgeblendet
  (Schalter „Versteckte zeigen“), `.gitignore` in Projekten ausgegraut. Aktionen: Öffnen, Neue
  Datei, Neuer Ordner, Umbenennen, Löschen mit Rückfrage.
- **W7 — Tabs** (Q7): je Tab ein versteckt weiterlebender `FileEditor`; ein Vorschau-Tab wie in
  VS Code (Einfachklick ersetzt ihn, Bearbeiten oder Doppelklick macht ihn fest); die Tabs
  überstehen einen Neustart; kein Teilen (dafür das IDE-Dock).
- **W8 — Schnellöffnen ⌘P jetzt** (Q8): unscharf über die Dateinamen aller Wurzeln aus W6; ein
  Rust-Index (mit `.gitignore`, Obergrenze um 50 000 Dateien), den ⌘K später mitbenutzt.
- **W9 — IDE-Pane „Files“** (Q9): derselbe Baum, auf die Projektwurzel beschränkt; ein Klick
  öffnet im Datei-Pane (in der Datei-Gruppe), ⌘P im Projekt.
- **W10 — Alte Workspace-Befehle** (Q10): `/open`, `/newfile`, `open-file` und die
  Agenten-Aktion geben weiter workspace-relative Pfade (→ `{root: "workspace", rel}`); die
  `*_workspace_file`-Befehle bleiben für Suche, Graph und Skills.
- **W11 — Panels** (Q11): je Datei ein Panel wie heute (eine offene Datei kommt nach vorn), jedes
  ein eigener `FileEditor`; Knopf „In der Datei-App öffnen“ übergibt die Sitzung samt
  ungespeicherter Arbeit als Tab und schließt das Panel. ⌘W/× schließt, mit Rückfrage bei
  Ungespeichertem.
- **W12 — Vollbild-Aufbau und Tasten** (Q12): Baum links (Breite ziehbar, ⌘B), Tab-Leiste oben,
  Kopfzeile wie heute; ⌘P, ⌘N, ⌘W, ⌃Tab/⌃⇧Tab, ⌘1–9. Tabs und Baum-Einstellungen unter
  `settings.editor` in `dashboard.json`.
- **W13 — Baum-Aktionen** (Q13): neue Befehle `file_rename`, `file_mkdir`; ein offener Tab folgt
  dem neuen Namen, eine gelöschte offene Datei behält ihren Tab mit dem bekannten Balken; Ordner
  werden rekursiv mit Rückfrage („N Dateien löschen?“) gelöscht; Verschieben per Ziehen in ED5.
- **W14 — Schnellöffnen im Detail** (Q14): Treffer im Dateinamen zählen mehr, zuletzt Geöffnetes
  vorn, die Wurzel als Etikett; Index je Wurzel beim ersten ⌘P, neu gebaut wenn älter als 30 s
  (die alten Treffer sofort); ⏎ fester Tab, ⌥⏎ Vorschau-Tab, `:12` springt zur Zeile.
- **W15 — Second-Brain-Vorschau für Nicht-Markdown** (Q15): Bilder als Bild, HTML als Quelltext
  mit Farben, nur der Anfang langer Dateien (etwa 200 Zeilen).
- **W16 — „Files“ im IDE-Layout** (Q16): im Standard-Layout neuer Projekte (links, schmal) und im
  „+“-Menü des Docks; bestehende Layouts bleiben.
- **W17 — Checkpoints** (Q17): **ED4.1** Datei-Panel (W1–W4, W10, `md-file` weg) · **ED4.2**
  Second-Brain-Vorschau · **ED4.3** Tabs in der Vollbild-Ansicht samt Übergabe aus dem Panel ·
  **ED4.4** Dateibaum mit `file_list`/`file_mkdir`/`file_rename` · **ED4.5** Schnellöffnen ·
  **ED4.6** IDE-Pane „Files“. Ein Commit je Checkpoint.

**ED4.1 — gebaut (2026-09-25):** `fileapp/FilePanel.svelte` ist ein reines Panel-Modul `file`
(neues Flag `stageOnly`: nicht in der Modulauswahl, nicht im Ring, `createInstance` lehnt ab);
`md-file.svelte` samt Kachel ist gelöscht. `FileEditor` zeigt jetzt jede Dateiart:
`fileKinds.ts` entscheidet (Markdown/HTML/SVG mit gerenderter Ansicht, Rasterbilder), beim
Öffnen „zum Lesen“ beginnen Markdown und HTML gerendert (W1). `HtmlPreview` (sandboxed `srcdoc`,
Links zwischen Lektionen öffnen im selben Editor), `SvgPreview` (`<img>` aus `data:`),
`ImageView` (`file_read_image`, Einpassen/100 %, Pixel- und Dateigröße). „New note“ ist eine
`FileSession` ohne Datei (`FileSession.untitled`, Entwurf unter eigenem Wiederherstellungs-
Schlüssel, nie geschrieben oder beobachtet); ⌘S/`:w` ruft `create_note`, danach zeigt das
Panel die abgelegte Notiz gerendert. Alle Öffner gehen über `openFilePanel`/`openNewNote`
(`core/staging.ts`); ein Panel folgt dem Editor zu einer neuen Datei (`panelSync.ts`). Schließen
per ×, ⌘W (und Esc außerhalb des Editors) fragt über einen Schließ-Wächter bei Ungespeichertem
bzw. einer nicht abgelegten Notiz nach (Speichern/Ablegen, Verwerfen, Abbrechen); Esc im Editor
gehört dem Editor (D16).

**Nebenbefund zu W2:** Eine Agenten-Aktion zum Öffnen gab es bisher nur an einer Dokument-
*Kachel* (und `open-file` hatte keinen Sender) — der Agent konnte dir praktisch nie eine Datei
öffnen. Jetzt gibt es Aktionen der Shell selbst (`registerShellAction`, Instanz `shell` im
Manifest), die erste ist `openFile`: eine workspace-relative Datei im Panel, gerendert.

Die Prüfungen von ED4.1: Ein Link `..%2f..%2f` überstand die Auflösung in `htmllink.ts` als
`../` — erst der Datei-Dienst hätte ihn abgewiesen (Sicherheit, MEDIUM); jetzt liefert
`resolveRelativeLink` für so einen Pfad `null`, und nichts wird geöffnet. `closeAllStaged`
umging die Schließ-Wächter (Architektur, MEDIUM) — ungenutzt, gelöscht. Der Abgleich „Config
folgt dem Editor“ ist eine reine, getestete Funktion (`panelSync.ts`). Nebenbei: Aufgaben-
Kästchen in gerendertem Markdown waren Textfelder (DOMPurify ließ `type` fallen) — jetzt echte,
schreibgeschützte Kästchen, auch im Chat. Und `/help` nannte unter `/add` nie ein Modul — der
Text wurde beim Laden gebaut, bevor die Module registriert waren (Testlücken-Prüfung); jetzt wird
er beim Aufruf gebaut. **Vorgemerkt für den Anfang von ED4.3:** `FileEditor`
(~720 Zeilen) verschlanken, bevor Tabs viele davon halten — ein Zustand „Sitzung oder Bild“ statt
zweier Variablen, die Vorschau-Steuerung als eigenes kleines Modul.

**ED4.2 — gebaut (2026-09-25):** `fileapp/FilePeek.svelte` ist der Blick in eine Datei im
Detailpanel des Second Brain (W5, W15): Markdown gerendert wie in der Vorschau des Editors,
Code und HTML als schreibgeschützte Editorfläche mit Farben (Umbruch an, absolute Zeilennummern,
ohne Vi und ohne Effekte), Bilder als Bild — gelesen über den Datei-Dienst, nur die ersten 200
Zeilen (`headOf` in `fileKinds.ts`), mit einem Hinweis, wenn mehr da ist. Die eigene
Auszugslogik des Second Brain (`excerpt`/`excerptHtml` in `core/markdown.ts`, ein Cache) ist
weg; „Open“ öffnet wie bisher das Panel.

Die Prüfungen von ED4.2: Wie ein Highlighter angelegt und bei einem Wechsel verworfen wird, stand
viermal von Hand da (Architektur, HIGH) — jetzt einmal, `fileapp/highlighting.ts` (`highlightFor`),
für Editor, Diff, Einstellungs-Vorschau und Peek. Bewusst **ohne Cache** (MEDIUM, abgelehnt): das
Lesen ist ein lokaler Aufruf, und ein Cache zeigte eine Datei veraltet, die ein Agent gerade
geändert hat. Dass `shell/` den Peek direkt einbindet, ist gewollt — der Editor ist über das
Datei-Panel ohnehin im Start-Bundle. `headOf` mit 0 Zeilen schnitt ein Zeichen ab — behoben.

**ED4.3 — gebaut (2026-09-25):** Zuerst wie vorgemerkt `FileEditor` verschlankt: ein Zustand
`opened` (Sitzung, Bild oder nichts) statt zweier Variablen, die Vorschau-Modi als kleine reine
Funktionen in `fileKinds.ts`. Dann die Tabs: `fileapp/tabs.ts` ist das reine Modell (eine Datei,
ein Tab; der eine wiederverwendbare Vorschau-Tab; Schließen geht zum rechten, sonst linken
Nachbarn; Speichern unter `settings.editor.tabs`), `FileTab.svelte` ein `FileEditor` je Tab —
die hinteren bleiben gemountet und gestapelt (unsichtbar und `inert`, nicht `display: none`, das
die Scroll-Position verlöre). Tab-Leiste mit Punkt für Ungespeichertes und ×, Vorschau-Tab
kursiv (Doppelklick oder Bearbeiten macht ihn fest). Tasten in der Capture-Phase: ⌘O, ⌘N (neue
Notiz als Tab), ⌘W, ⌃Tab/⌃⇧Tab, ⌘1–⌘9; `:q` schließt den Tab. Schließen fragt über
`UnsavedQuestion.svelte` nach (dieselbe Frage wie im Panel). „Open in the file app“ im Panel
übergibt die lebende Sitzung (`FileEditor.detach` → `adoptSession`, `handoff.ts`): der Tab hat
den ungespeicherten Text und die Undo-Geschichte.

**⌘W und das macOS-Menü:** Tauri gibt einer macOS-App ein Standardmenü, dessen File- und
Window-Menü „Close Window“ auf ⌘W legen — das hätte die eine App-Fenster geschlossen statt eines
Tabs oder Panels. `src-tauri/src/menu.rs` baut jetzt dasselbe Menü ohne diesen Eintrag (das
Edit-Menü bleibt, es trägt ⌘C/⌘V in Textfelder; ⌘Q beendet weiter). **Nur in der echten App zu
prüfen** (Live-Test): ⌘W schließt Tab bzw. Panel und nie das Fenster.

Die Prüfungen von ED4.3: Ein zweites × während einer offenen Schließ-Frage überschrieb die erste,
deren Frage nie mehr beantwortet wurde (Architektur, MEDIUM) — jetzt wird es ignoriert, solange
eine Frage steht. Das Speichern der Tabs hing an der Reihenfolge der Effekte (MEDIUM) — jetzt
wartet es auf einen beobachteten Zustand. Ein Tab ohne gültigen aktiven Nachbarn kam vorn statt
hinten dazu (Testlücken). Kleinkram: die Liste der Tab-Ansichten räumt auf, eine übergebene
Sitzung, die kein Tab mehr übernahm, wird geschlossen, ein leeres Help-Menü hält die
Menü-Suche von macOS. **Vorgemerkt:** ein offener Tab soll einem Umbenennen folgen — der
Datei-Beobachter kennt kein „umbenannt“, also braucht ED4.4 einen ausdrücklichen Weg vom Baum
in die Sitzung (`retarget`); und ob Tabs im Hintergrund ihren Highlighter abgeben, entscheidet
sich vor ED4.5, wenn ⌘P viele Tabs leicht macht.

**ED4.4 — gebaut (2026-09-26):** Der Datei-Dienst kann Ordner (`axiomata-files::dir`): `list_dir`
(Ordner zuerst, höchstens 5 000 Einträge, `.gitignore`-Treffer markiert — auch ein verschachteltes
`.gitignore` mit `!` zählt, und was in einem ignorierten Ordner liegt, gilt als ignoriert),
`make_dir`, `rename_entry` (nie überschreibend, `renameat` mit `NOREPLACE`), `count_tree` und
`delete_tree` (rekursiv, nie die Wurzel, ein Symlink wird entfernt, nie verfolgt). Alles geht wie
bisher Komponente für Komponente mit `openat(O_NOFOLLOW)` vom Wurzelverzeichnis aus; ein
verlinkter Ordner erscheint als Link und lässt sich nicht als Ordner öffnen. Befehle
`file_list`/`file_mkdir`/`file_rename`/`file_count`/`file_delete_tree`. Im Frontend
`FileTree.svelte`: die Wurzeln ohne Worktrees (W6), Ordner werden beim Aufklappen gelesen, Punkt-
Dateien, `.git`, `node_modules` und `target` ausgeblendet („hidden“ zeigt sie), Ignoriertes
ausgegraut; Klick öffnet im Vorschau-Tab, Doppelklick fest; Rechtsklick oder ⋯: Neue Datei, Neuer
Ordner, Umbenennen (in der Zeile), Löschen (mit „… und N Einträge darin?“). Offene Tabs folgen
einem Umbenennen — auch eines Ordners, in dem sie liegen — über `FileSession.moved` (beobachtet
den neuen Pfad, nimmt den beiseitegelegten Text mit, lädt nicht neu). Die Regeln stehen rein in
`treeModel.ts`; Breite (gezogen), Sichtbarkeit (⌘B), „hidden“ und offene Ordner unter
`settings.editor.tree`. Die Tab-Leiste steht jetzt in der Spalte rechts vom Baum.

Die Prüfungen von ED4.4: Umbenennen erreichte nur die Tabs der Datei-App — ein Panel mit derselben
Datei bekam vom Beobachter ein falsches „gelöscht“, und die Kette im Frontend lief nicht im Takt
mit dem Beobachter (Architektur, CRITICAL/HIGH). Jetzt meldet der Rust-Befehl selbst
`files:renamed` (und `files:removed` nach dem Löschen), sobald er fertig ist, und **jeder**
`FileEditor` folgt — Tab, Panel, später das IDE-Pane; der Tab folgt seinem Editor, die Datei-App
pflegt nur noch „Recent“. `.git`, `.hg` und `.jj` ändert der Baum nie (Sicherheit, HIGH: die
Git-Ebene der Agenten steht darauf). „Neue Datei“ war Lesen-dann-Schreiben und hätte eine eben
entstandene Datei überschreiben können (HIGH/MEDIUM) — jetzt `file_create` mit `O_CREAT|O_EXCL`.
`pin` lehnt `..` selbst ab, statt sich auf den Aufrufer zu verlassen (MEDIUM). Das Zählen hört auf
allen Ebenen an der Grenze auf, ein Ordner, in den beim Löschen geschrieben wird, sagt das, der ⋯
ist ein eigener Knopf (kein Knopf im Knopf), und das Umschreiben der offenen Ordner ist rein und
getestet (`expandedAfterRename`/`expandedAfterDelete`). Gut 20 weitere Rust-Tests.

Entschieden (aus ED4.3 vorgemerkt): **Tabs im Hintergrund geben nichts ab.** Ein verborgener Tab
zeichnet nicht neu und rechnet nichts; er hält nur Text und Syntaxbaum. Eine Obergrenze oder ein
Schlafmodus wäre Aufwand ohne spürbaren Gewinn — erst wenn es sich im Gebrauch anders zeigt.

**ED4.5 — gebaut (2026-09-26):** Schnellöffnen mit ⌘P (W8, W14). Rust `axiomata-files::index`
(`file_index`): ein Durchgang über eine Wurzel mit dem `ignore`-Walker — `.gitignore` gilt (auch
ohne Git-Repo), Punkt-Dateien und -Ordner, `node_modules` und `target` bleiben draußen, Links
werden nicht verfolgt, höchstens 50 000 Dateien. Nur Namen, kein Inhalt; geöffnet wird wie immer
über den Datei-Dienst. `fileapp/quickOpen.ts` rechnet rein: Buchstaben in Reihenfolge,
Treffer im Dateinamen deutlich vorn, Wortanfänge (auch camelCase) und zusammenhängende Treffer
zählen mehr, kürzere Pfade gewinnen Gleichstände, zuletzt Geöffnetes kommt nach vorn; `name:12`
springt zu Zeile 12. `QuickOpen.svelte`: Treffer markiert, die Wurzel als Etikett, ↑/↓, ⏎ fester
Tab, ⌥⏎ Vorschau-Tab, Esc. Der Index je Wurzel lebt für die Sitzung und wird nach 30 s im
Hintergrund neu gelesen — die alten Treffer sind sofort da (W14). Der Index ist so gebaut, dass
⌘K ihn später mitbenutzen kann (W8).
Reviews: Sicherheit LOW — Links werden nie gelistet (auch nicht auf Ziele in der Wurzel), und jedes
Öffnen prüft der Wächter ohnehin neu. Nachgezogen: der Durchgang bricht zusätzlich nach 500 000
angesehenen Einträgen ab (`MAX_VISITED`, ein Worktree voller ignoriertem Ballast hält ihn nicht
auf), und das Modul-Doc sagt, dass `.gitignore`-Dateien *über* der Wurzel nicht gelesen werden
(wie ein Editor, der einen Ordner durchsucht — bewusst, ein Test hält es fest). Der
Modul-Cache in `QuickOpen.svelte` ist keine Svelte-Reaktivität; ein Kommentar warnt, dass jede
`$derived` den `arrived`-Zähler mitlesen muss. Tests ergänzt: verschachtelte und negierte
`.gitignore`, Zeilenangaben, camelCase, dieselbe Datei aus zwei Wurzeln.

**ED4.6 — gebaut (2026-09-26):** das IDE-Pane „Files“ (W9, W16). `ide/panes/FilesPane.svelte` ist
der `FileTree` der Datei-App mit genau einer Wurzel, `project:<id>` des offenen Projekts; offene
Ordner und der Schalter „hidden“ liegen im Tab (`FilesPaneConfig`) und damit im gespeicherten
Layout des Projekts. Ein Klick öffnet die Datei über `openOrFocus` in der Datei-Gruppe; gibt es
noch keine, wird sie ein neuer Reiter in der Gruppe neben dem Baum — seit dem ersten IDE-Test des
Owners (2026-09-27) kein Split mehr, auch nicht beim Öffnen aus einem Terminal- oder Agenten-Pane:
dort wird die Datei Reiter derselben Gruppe. Wer sie als eigenes Pane will, zieht sie heraus; weitere
Dateien sammeln sich dann dort. Der Baum markiert die Datei im vordersten Tab der Datei-Gruppe
(`frontFile`). Neue Projekte beginnen mit Files links (16 % der Breite) und einem Terminal
(`withFilesPane`); bestehende Layouts bleiben, wie sie sind. Das „+“ der Tab-Leiste ist jetzt ein
kleines Menü „Terminal / Files“. ⌘P in der IDE öffnet das Schnellöffnen nur über das Projekt, die im
Dock offenen Dateien zuerst; ⌘P ist die einzige eigene Taste der IDE-Ansicht, alles andere bleibt
bei den Panes. Architektur-Review ohne CRITICAL/HIGH; nachgezogen: ein offenes „+“-Menü schließt, wenn
das einer anderen Gruppe aufgeht, die Nachbar-Regel sagt, dass sie auf der flachen Reihe beruht, die
`layout.ts` beim Normalisieren herstellt, und ein Files-Pane ohne offenes Projekt zeigt einen Hinweis
statt nichts. Mehrere Files-Panes sind erlaubt wie mehrere Terminals. Damit ist **ED4 komplett**.

### ED5 im Detail (gegrillt 2026-09-26, Q1–Q19, bestätigt; umgesetzt 2026-09-26/27 — ED5 komplett)

**Entscheidungen**

- **T1 — Rope** (Q1): ein **unveränderliches Rope aus Zeilenblöcken** hinter `TextStore` — ein
  balancierter Baum, die Blätter halten Folgen von Zeilen, jeder Knoten kennt Zeilenzahl und
  UTF-16-Länge; `line(i)`, `offsetAt` und der Weg zurück in O(log n). Knoten werden geteilt, jede
  Fassung ist ein billiger Schnappschuss (für die Suche im Worker, später ein Undo-Baum).
- **T2 — Große Dateien** (Q2): bis 16 MB bearbeitbar (Schreibgrenze in `axiomata-files` mit);
  über 2 MB ein **leichter Modus** — kein tree-sitter, keine Minimap, kein Sticky Scroll, ein
  Hinweis in der Statuszeile.
- **T3 — Undo** (Q3): bleibt linear (D19); der Baum kommt später auf T1 auf.
- **T4 — Suchen/Ersetzen, Umfang** (Q4): eine Such-Leiste in der Datei (⌘F, ⌥⌘F mit Ersetzen,
  Regex/Groß-klein/ganzes Wort, „3/17“, ⏎/⇧⏎, ⌘G/⇧⌘G, ⌘E, „Alle ersetzen“ als ein Undo-Schritt)
  **und** eine Projektsuche (⇧⌘F) in Rust; dateiübergreifendes Ersetzen später, eigener Punkt.
- **T5 — Such-Maschine** (Q5): ein **Web Worker** auf einem Rope-Schnappschuss mit Zeitgrenze
  (um 1 s, dann beendet: „Muster zu teuer“), für den normalen Modus und Vi gemeinsam; Treffer
  dürfen über Zeilen gehen, sobald das Muster `\n` enthält. Die Rust-Projektsuche nutzt das
  `regex`-Crate (ohne Backtracking) und braucht keine Zeitgrenze.
- **T6 — Mehrere Cursor** (Q6): nur im normalen Modus. ⌥-Klick, ⌥⌘↑/↓, ⌘D, ⌘U (letzten
  zurücknehmen — ⌘K ist app-weit reserviert), ⇧⌘L, ⌥-Ziehen als Spaltenauswahl, Esc zurück zu
  einem. Jede Aktion an jedem Cursor, als ein Undo-Schritt; Einfügen verteilt Zeilen, wenn die
  Zahl passt. Vi behält den Visual-Block; Mehrfach-Cursor in Vi später.
- **T7 — Faltung** (Q7): Bereiche aus tree-sitter (dieselbe Knotentabelle wie die Textobjekte),
  Markdown-Überschriften, sonst Einrückung. Pfeile im Gutter, ⌥⌘[ / ⌥⌘], ⌥⌘0 / ⌥⌘J, Vi
  `zc zo za zR zM`, „⋯ N Zeilen“, ein Treffer darin klappt auf, Animation abschaltbar. Gemerkt je
  Datei für offene Tabs/Panes (`dashboard.json`), mit den Tabs aufgeräumt.
- **T8 — Minimap** (Q8): winzige Zeichen in Token-Farben auf einem Canvas, ziehbares Sichtfenster,
  Marken für Treffer, Diff/Git und später Diagnosen. An in Vollbild und IDE, aus im Panel.
- **T9 — Sticky Scroll** (Q9): bis 5 Kopfzeilen aus der Knotentabelle, Klick springt, weicher
  Schatten; an außer im Panel, abschaltbar.
- **T10 — Installierte Mac-Schriften** (Q10): CoreText in `axiomata-macos`, alle Familien mit
  ihren echten Schnitten, Filter „nur Monospace“ als Vorgabe; eine fehlende Schrift fällt still auf
  die Vorgabe zurück, mit Hinweis in den Einstellungen.
- **T11 — Verschieben im Baum** (Q11): Ziehen auf einen Ordner derselben Wurzel (`file_rename`
  ohne Ersetzen, Tabs folgen), zugeklappte Ordner öffnen sich nach einer halben Sekunde; über
  Wurzeln hinweg abgelehnt.
- **T12 — Vi-Zugaben** (Q12): `:g`/`:v`, `:s///c` (y/n/a/q), `gn`/`cgn` — mit der Such-Maschine.
- **T13 — Wo die Projektsuche lebt** (Q13): in der Datei-App Reiter **Files | Search** in der
  linken Spalte (⇧⌘F; Wurzel-Filter über die Baum-Wurzeln); in der IDE ein Dock-Pane „Search“
  (im „+“-Menü, ⇧⌘F), aufs Projekt beschränkt; ein Klick öffnet an der Zeile. Eine Komponente.
- **T14 — Projektsuche im Detail** (Q14): Regex/Groß-klein/ganzes Wort, Globs „einschließen“ und
  „ausschließen“; `.gitignore`/Index-Regeln, Binärdateien und Dateien über 16 MB bleiben draußen;
  Treffer kommen fortlaufend per Event, neue Eingabe bricht ab; höchstens 10 000 Treffer mit
  Hinweis; gruppiert nach Datei, zuklappbar, Ausschnitt markiert; offene ungespeicherte Tabs
  zeigen den Stand der Platte mit ●.
- **T15 — Such-Leiste im Detail** (Q15): „nur in der Auswahl“, `$1`/`$&`, Groß/klein beim Ersetzen
  erhalten. In jeder Editor-Fläche; in Diffs und schreibgeschützten Dateien nur Suchen. In Vi
  öffnet ⌘F dieselbe Leiste, `/` bleibt die Vi-Zeile; beide teilen den letzten Suchbegriff.
- **T16 — Schriften fürs Terminal** (Q16): dieselbe Liste, fest „nur Monospace“.
- **T17 — Diffs** (Q17): Faltung, Minimap (Hunks farbig) und Sticky Scroll auch dort, dazu der
  Schalter „unveränderte Bereiche falten“.
- **T18 — Zusammenspiel mit Vi** (Q18): Wechsel nach Vi oder Esc macht aus mehreren Cursorn den
  zuletzt gesetzten; `j`/`k` überspringen eine Faltung, `dd` löscht sie ganz; Suchen und `gn`
  klappen auf; Sticky-Scroll-Zeilen verdecken nie den Cursor (`zt`, Scrollen rechnen mit ihnen).
- **T19 — Checkpoints** (Q19), ein Commit je Checkpoint: **ED5.1** Rope (ED1-Tests unverändert,
  Zufallstest gegen `LineStore`, 16 MB, leichter Modus) · **ED5.2** Such-Maschine (Worker, Vi
  zieht um, T12) · **ED5.3** Mehrere Cursor · **ED5.4** Such-Leiste (⌥⏎ macht alle Treffer zu
  Cursorn) · **ED5.5** Faltung · **ED5.6** Sticky Scroll und Minimap · **ED5.7** Projektsuche ·
  **ED5.8** Installierte Schriften · **ED5.9** Verschieben im Baum. Sicherheitsprüfung bei ED5.1,
  ED5.7, ED5.9; Abhängigkeitsprüfung bei neuen Crates (ED5.7/5.8).

**ED5.1 — gebaut (2026-09-26):** das Rope (T1, T2). `editor/rope.ts`: `Rope` ist ein unveränderlicher
B-Baum aus Zeilenblöcken (Blätter bis 64 Zeilen, Äste bis 16 Kinder, alle Blätter gleich tief; ein
unterfülltes Geschwister wird auf dem Rückweg eingemischt), `RopeStore` der veränderliche `TextStore`
darum, `snapshot()` gibt die aktuelle Fassung fest heraus (für den Such-Worker in ED5.2). `text()` wird
je Fassung einmal gebaut. `EditorDocument` benutzt `RopeStore`; `LineStore` bleibt als einfacher
Vergleich für Tests. Die ED1-Suite läuft unverändert gegen beide, dazu ein Zufallstest mit 4500
Änderungen gegen `LineStore` und Invarianten-Prüfung nach jeder. Gemessen an 16 MB (242 000 Zeilen):
1000 Einfügungen 4,6 ms, 1000 `offsetAt` 0,6 ms (Zeilenliste: 69 ms), Öffnen 17 ms. Große Dateien:
`MAX_WRITE_BYTES` = `MAX_READ_BYTES` (16 MiB), über 2 MiB `FileSession.light` statt schreibgeschützt —
kein tree-sitter, ein Hinweis „Large file — light mode“ in der Statuszeile; die Wiederherstellung
folgt der Schreibgrenze.
Reviews: Sicherheit fand einen HIGH-Punkt — mit 16 MiB je Eintrag hätten 256 Wiederherstellungs-Einträge
bis 16 GiB belegen können. Jetzt begrenzt `MAX_TOTAL_BYTES` (256 MiB) den ganzen Ordner, das Speichern
schiebt die ältesten Einträge hinaus, bis der neue passt, und ein Eintrag, den `load_in` nicht mehr
zurücklesen würde (JSON über 64 MiB), wird gar nicht erst geschrieben. Veraltete „read-only“-Texte in
`file_read`/`file_write` und der CLI korrigiert. Offen fürs Live-Testen: Speicherbedarf mit mehreren
16-MiB-Tabs (IPC geht als JSON). Tests ergänzt (sehr lange Zeile, Anhängen am Ende eines tiefen Baums,
Löschen über ganze Teilbäume, `text()`-Cache je Fassung, die 16-MiB-Kante in Rust); der nie erreichte
Anhänge-Zweig in `splice` ist entfernt.

**ED5.2 — gebaut (2026-09-26):** die Such-Maschine (T5, T12). Umsetzung von T5 im Detail: der Worker ist
ein **Prüfer**. `editor/search/guard.ts` (`SearchGuard`) lässt jedes Muster zuerst im Worker
(`search/worker.ts`) auf einer festen Rope-Fassung laufen, mit 1 s Zeitgrenze — danach wird der Worker
beendet und neu gestartet, das Urteil heißt „Pattern too expensive“. Urteile gelten je Fassung und Muster
(`WeakMap<Rope, …>`) und tragen alle Treffer (bis 100 000) als Offsets mit; `n`/`N`, hlsearch und `gn`
antworten daraus per Binärsuche (`search/jump.ts`), ohne das Muster im Haupt-Thread noch einmal
auszuführen. Wo das Muster doch im Haupt-Thread läuft (`:s`, `:g`), dann erst nach dem Urteil —
es kostet dort nicht mehr als im Worker. Solange das Urteil unterwegs ist, **pausiert Vis Tasten-
schlange** wie beim Warten auf die Zwischenablage (`SearchPending`) und spielt die Tasten danach ab —
Makros und `.` bleiben damit genau; ein `<CR>` der Befehlszeile wird geprüft, *bevor* sie schließt.
Vorschau und hlsearch zeichnen bis dahin nichts und kommen mit dem Urteil (`onVerdict`, `settled`).
Ohne Worker (Tests, jsdom) läuft alles wie vorher sofort. Muster mit `\n` treffen über Zeilen
(`search/matches.ts` `spansLines`), auch in `:s` (`substituteSpanning`). Dazu T12: `:g`/`:g!`/`:v`
mit `:d`, `:s`, `:normal` als Befehl (`vi/global.ts`: Zeilen erst markieren, dann besuchen,
`LineAnchors` folgen den Änderungen; ein Undo-Schritt), `:d [x]`, `:norm[al]`, `:s///c` mit
y/n/a/q/l (ein Undo-Schritt), `gn`/`gN`/`cgn`/`dgn` (mit `.` wiederholbar). Browser-Test mit echtem
Worker: `/fn s` markiert und springt; `/(x+x+)+y` auf einer langen x-Zeile — die Seite antwortet
währenddessen in 6 ms, nach 1 s die Meldung, `j` geht sofort weiter.
Reviews (Architektur CRITICAL, Tests drei Fehler) — behoben: (1) `:g` mit `:s` hielt mit echtem Worker nach
der ersten Zeile still an und `2@:` mit einem `:g` darin lief endlos, weil jede eigene Änderung eine neue,
ungeprüfte Fassung erzeugt; jetzt gilt: eine **Schleife** (`:g`, ein `:normal` über Zeilen, gezähltes `@:`)
lässt alle ihre Muster *vorab* prüfen und wartet innerhalb nicht mehr — das einzige Mal, dass ein Muster
auf Text läuft, den der Worker nicht gesehen hat, und der unterscheidet sich nur um die Änderungen der
Schleife selbst. (2) Der Worker durchläuft jetzt immer den *ganzen* Text und meldet nur die ersten 100 000
Treffer — ein Urteil beweist so die ganze Datei; Markierungen eines zeilenübergreifenden Musters mit mehr
Treffern suchen nur im sichtbaren Ausschnitt. (3) `gn` auf einem leeren Treffer am Zeilenende setzte den
Visual-Anker hinter den Cursor — jetzt wie der Cursor begrenzt. Außerdem ist die Rückfrage von `:s///c` in
ein eigenes Modul gezogen (`vi/confirm.ts`), `cmdmode.ts` bleibt beim Verteilen der Befehle. Für ED5.4
vorgemerkt: „Alle ersetzen“ der Such-Leiste prüft einmal und ersetzt dann aus den Offsets des Urteils.

**ED5.3 — gebaut (2026-09-27):** mehrere Cursor (T6). `EditorDocument` hält neben `selection` (dem Haupt-
Cursor, zuletzt gesetzt) `extra` in der Reihenfolge des Hinzufügens; Undo-Schritte merken sich alle Cursor
davor und danach. `editor/multicursor.ts` führt einen Befehl an jedem Cursor aus — von hinten nach vorn, mit
dem unveränderten Ein-Cursor-Befehl, alle anderen Cursor folgen jeder Textänderung (`onTextChange`) —, als
*ein* Undo-Schritt; Tippen mit mehreren Cursorn verschmilzt wie mit einem, bis zur Pause oder einem Sprung.
Sich berührende Cursor werden eins; zeilenweise Befehle (⇧⇥, ⌥↑/↓, ⌘/) nehmen eine Zeile nur einmal, ⇥ an
nackten Cursorn setzt Einrückung an jedem. Dazu ⌥-Klick (setzen/wegnehmen), ⌥-Ziehen (Spalte), ⌥⌘↑/↓ (in der
Spalte des Ausgangs-Cursors), ⌘D (Wort, dann nächstes Vorkommen, rundherum; das neue ist der Haupt-Cursor),
⌘U, ⇧⌘L, Esc. Kopieren nimmt jede Auswahl, Einfügen verteilt die Stücke, wenn die Zahl passt, sonst den
ganzen Text an jeden Cursor. Vi bleibt bei einem Cursor (T18: seine erste Bewegung nimmt die anderen weg).
Browser-Test: dreimal ⌘D auf `fn`, „func“ getippt — alle drei ersetzt, ⌘Z nimmt alle in einem Schritt zurück.
Reviews (Tests: ein Fehler, Architektur: zwei CRITICAL) — behoben: ⌥↑/↓ mit Cursorn auf benachbarten Zeilen
verschluckte eine Bewegung und ließ Cursor springen; Zeilen verschieben geht jetzt **je Block** berührender
Zeilen (`eachLineBlock`), die Cursor behalten ihre Lage im Block, ⇧⌥↓ dupliziert weiter jede Zeile für sich.
Die Zielspalte für ↑/↓ hat jetzt jeder Cursor selbst (`extraGoals`), vorher verrutschten die Spalten schon
beim ersten ↓. ⌥-Ziehen fügt seine Spalte zu schon vorhandenen Cursorn hinzu, statt sie zu verwerfen.
Für ED5.4: ⌥⏎ der Such-Leiste setzt die Cursor direkt aus den Treffern des Urteils (`setSelections`), nicht
über `occurrences` (das sucht nur wörtlich).

**ED5.4 — gebaut (2026-09-27):** die Such-Leiste (T4, T15). `fileapp/FindBar.svelte` schwebt oben rechts über
jeder Editor-Fläche; die Logik ist `editor/search/findModel.ts` (ohne DOM), die Sprache `editor/search/find.ts`:
dieselbe Muster-Sprache wie Vis `/` (JS-Regex plus `\<` `\>` `\c` `\C`), wörtliche Eingaben sind einfach
maskiert, „ganzes Wort“ ist `\<(?:…)\>`. ⌘F (eine Auswahl in einer Zeile wird der Begriff, eine über mehrere
Zeilen der Bereich), ⌥⌘F mit Ersetzen, ⌘G/⇧⌘G, ⌘E, in Vi genauso — die Fläche nimmt die Tasten vor dem Automaten.
Im Feld: ⏎/⇧⏎, ⌥⏎ (alle Treffer zu Cursorn, höchstens 10 000, nicht in Vi), ⌥⌘C/W/R/L/P, im Ersetzen-Feld ⏎
und ⌥⌘⏎. Treffer kommen nur aus dem Urteil des Prüfers; eine Aktion, die auf ein Urteil wartet, läuft danach,
eine neuere ersetzt sie. „Alle ersetzen“ ist *eine* Änderung über den Bereich vom ersten bis zum letzten Treffer
(ein Undo-Schritt, schnell auch bei 100 000 Treffern); bei abgeschnittenem Urteil läuft das Muster einmal ganz im
Haupt-Thread (der Prüfer hat den ganzen Text abgenommen). Gruppen holt ein haftender `exec` genau an der
Trefferstelle — auf der Zeile, wenn das Muster zeilenweise sucht, damit `^` und Lookbehind dasselbe sehen.
`$&`/`$0`, `$1`–`$99`, `$<name>`, `$$`, `\n` `\t`; „Groß/klein erhalten“ für GROSS, klein und Wortanfang.
Der Bereich „nur in der Auswahl“ folgt jeder Änderung (`onTextChange`), neues Token `--ax-search-scope`.
In Vi ist ein Treffer „der aktuelle“, wenn der Cursor auf seinem Anfang steht. Leiste und Vi teilen den letzten
Suchbegriff (`fileapp/findShared.ts`): die Leiste schreibt ihn Vi mit ausgeschriebener Groß/klein-Regel
(`\C`/`\c`) in `SearchMemory` und die Historie, ein neueres `/` übernimmt die Leiste als Regex.
In Diffs und schreibgeschützten Dateien nur Suchen. Browser-Test mit echtem Worker: Zähler, Ersetzen mit
foo/Foo/FOO → qux/Qux/QUX, Alle ersetzen, ⌥⏎ → drei Cursor.
Reviews (Architektur: ein CRITICAL, ein HIGH, ein MEDIUM; Tests: keine Fehler, viele Fälle ergänzt) — behoben:
(1) schon das Tippen eines Begriffs warf Mehrfach-Cursor weg — jetzt hebt die Suche beim Tippen nur hervor,
solange mehrere Cursor stehen; erst ein Sprung oder Ersetzen geht zu einem Treffer. (2) Ein zweites ⌘F bei
offener Leiste überschrieb einen noch nicht benutzten Begriff mit dem geteilten. (3) In Vi schloss Esc aus dem
Text die Leiste nie — jetzt, sobald Esc nichts mehr abzubrechen hat. Vorgemerkt (LOW): der Prüfer verwirft beim
Tippen nur wartende Aufträge desselben Musters; bei sehr teuren Mustern könnten sich Zwischenstände stauen.

**ED5.5 — gebaut (2026-09-27):** die Faltung (T7, T17, T18). `editor/fold/ranges.ts` bestimmt, *wo* der Text falten
kann, `editor/fold/state.ts` (`FoldState`), *was* gefaltet ist — geschlossene Faltungen sind reine Zeilenbereiche,
die jeder Änderung folgen: Zeilen darüber verschieben sie, Änderungen unter der Kopfzeile dehnen/stauchen sie,
Tippen auf der Kopfzeile lässt sie stehen, eine Änderung über Kopf- oder Endzeile hinweg öffnet sie. Verschachtelte
Faltungen behalten ihren Zustand. Umsetzung von T7 im Detail: **Einrückung gilt immer** (auch im leichten Modus und im
Diff), tree-sitter gewinnt auf den Zeilen, wo es selbst einen Bereich hat — die Knotentabelle der Textobjekte
(`FUNCTIONS`/`CLASSES`, jetzt exportiert), dazu jeder Klammerknoten (`{ … }`, `[ … ]`, `( … )`) und mehrzeilige
Kommentare (Erweiterung gegenüber T7: nur Funktionen/Klassen hätten `if`-Blöcke und Objekte nicht gefaltet). Eine
schließende Zeile (`}`, `)`, `]`, `end`) bleibt sichtbar, damit eine Faltung nie das `} else {` des nächsten Blocks
verschluckt. Markdown faltet nach Überschriften (Code-Zäune ausgenommen). Gefaltete Zeilen haben in `VisualLayout`
keine Zeilen; ↑/↓, ←/→ an der Zeilengrenze und ⌘↓ gehen über eine Faltung hinweg, Vis `j`/`k` zählen sie als eine
Zeile, zeilenweise Operatoren (`dd`, `yj`, `>>`, `cc`) nehmen sie ganz (T18); relative Zeilennummern zählen gefaltete
Zeilen nicht mit. Nach jedem Befehl öffnet die Fläche, was einen Cursor verdeckt (Suche, `n`, `gn`, Undo, Sprung) —
außer bei ⌘A. Chevron im Gutter (beim Überfahren, bei gefalteten Zeilen immer), „⋯ N lines“ hinter der Kopfzeile
(Klick öffnet), ⌥⌘[ ⌥⌘] ⌥⌘0 ⌥⌘J (auch als physische Taste für andere Layouts), Vi `zc zo za zM zR`. Die Zeilen
darunter gleiten an ihren Platz, neu gezeigte blenden ein — abschaltbar („Animated folding“, aus bei „Bewegung
reduzieren“). Jede `FileSession` besitzt ihre Faltungen (sie gehen mit vom Panel in den Tab);
`fileapp/foldMemory.ts` merkt sie je Datei in `settings.editor.folds` (`dashboard.json`), vergessen beim Schließen des
Tabs oder IDE-Panes, höchstens 100 Dateien. T17: im Diff faltet die Einrückung, die beiden Seiten der geteilten Ansicht
teilen *einen* `FoldState` (ihre Zeilen sind gepaart, sie bleiben im Gleichschritt); der Schalter „unveränderte
Bereiche falten“ ist der schon vorhandene „Whole file“-Schalter (H11). Gemessen: Bereiche für 10 500 Zeilen
TypeScript 11 ms (Baum) + 8 ms (Einrückung), nur 150 ms nach der letzten Änderung neu; ab 50 000 Zeilen fragt der
Gutter Zeile für Zeile (`near`, höchstens 5000 Zeilen nach unten), weil der ganze Durchgang bei 242 000 Zeilen 263 ms
kostet. Browser-Test: Chevron, Pill, ⌥⌘0/⌥⌘J, ⌘F in eine Faltung, Vi `zM`/`j`/`zo`/`zc`/`dd`; dabei gefunden:
ein Zeilenkommentar nimmt seinen Zeilenumbruch mit (endet in Spalte 0 der nächsten Zeile) und faltete so die `fn`-Zeile
darunter weg — ein Knoten, der am Zeilenanfang endet, endet jetzt auf der Zeile davor.
Reviews (Architektur: ein HIGH, drei MEDIUM; Tests: ~50 Fälle ergänzt, kein Fehler, drei Vim-Abweichungen) — behoben:
(1) Vis `H`/`M`/`L`, ein bloßes `G` und `Ctrl-D/U/F/B/E/Y` landeten in Faltungen und öffneten sie damit; jetzt landen
sie auf der Kopfzeile (`motions.ts` `shown`), `H`/`L` zählen eine Faltung als eine Zeile; `12G` nennt eine Zeile und
öffnet weiterhin. (2) Schloss sich eine Faltung über einem Extra-Cursor, fielen *alle* Extra-Cursor weg — jetzt rückt
nur der verdeckte auf die Kopfzeile. (3) Die Gleitdauer liest `--ax-dur-med` aus dem Theme statt einer Zahl daneben.
(4) `o` und `p` auf einer gefalteten Kopfzeile setzen unter die ganze Faltung, die geschlossen bleibt. Bekannt und
gelassen: `J` auf einer gefalteten Kopfzeile verbindet mit der ersten verborgenen Zeile (Vim: mit der Zeile danach);
eine Datei, die in Tab und IDE-Pane zugleich offen ist, verliert beim Schließen des einen nur die Erinnerung; im
Split-Diff kann eine Faltung an einer Hunk-Grenze drüben Zeilen verdecken, die dort kein Block sind.

**ED5.6 — gebaut (2026-09-27):** Sticky Scroll und Minimap (T8, T9, T17, T18). **Sticky Scroll**
(`editor/sticky.ts`): bis zu fünf Kopfzeilen der Blöcke um die oberste Zeile bleiben über dem Text stehen. Umsetzung
von T9 im Detail: die Blöcke sind die **Faltbereiche** (`fold/ranges.ts`) — mit Baum Funktionen, Klassen und
Klammerblöcke, sonst Einrückung —, *eine* Vorstellung von „Block“ für Faltung und Sticky Scroll (Erweiterung gegenüber
„aus der Knotentabelle“, wie bei ED5.5). Slot für Slot nennt die Zeile unter dem Slot den Block ihrer Tiefe, gepinnt
wird ein Kopf erst, wenn seine eigene Zeile hinausgescrollt ist; der innerste wird von der Zeile nach seinem Block
hinausgeschoben. Die Köpfe decken den Text darunter ab (T18): der Cursor bleibt darunter (`clearOfSticky` in
`revealCursor` und `goToLine`), `zt` setzt die Zeile unter sie, Vis `H`/Viewport beginnt unter ihnen. Klick springt
zum Kopf (mit seinen äußeren Köpfen darüber), Mausrad darüber scrollt den Text; Zeilennummern absolut, Farben wie im
Text, weicher Schatten (`--ax-sticky-shadow`). Die Bereiche sind die des Gutters (150 ms nach der letzten Änderung
frisch). **Minimap** (`editor/minimap.ts` Geometrie, `fileapp/Minimap.svelte` Canvas, 110 px rechts vom Scroller):
jede sichtbare Zeile als 3-px-Streifen, Zeichen als 1-px-Blöcke in Token-Farben (T8: „winzige Zeichen“ — in dieser
Größe ist ein Glyph ein Block), über die Farben der Theme-Tokens (über ein Probe-Element aufgelöst, neu bei jedem
Theme-Wechsel). Ein längerer Text scrollt die Minimap im Verhältnis mit, der Schieber (`--ax-minimap-slider`) läuft
über ihre ganze Höhe; Ziehen scrollt, ein Klick daneben zentriert dort. Marken: Treffer der Such-Leiste oder Vis,
im Diff hinzugefügte/entfernte Zeilen mit Randbalken (T17), die Cursorzeile; Diagnosen später. Gezeichnet wird nur
der sichtbare Teil, einmal je Animations-Frame. Die Such-Leiste rückt links neben die Minimap. Einstellungen
„Minimap“ und „Sticky scroll“ (an); aus im Panel (`FileEditor` `compact`), in `FilePeek`, in der Vorschau der
Einstellungen und im leichten Modus (T2). Browser-Test: vier gepinnte Köpfe in verschachteltem Rust, ↑ unter die
Köpfe scrollt mit, `zt` setzt unter sie, Minimap-Klick und -Ziehen.
Reviews (Architektur: ein HIGH, zwei MEDIUM; Tests: 27 Fälle ergänzt, kein Fehler) — behoben: (1) ein umbrochener
Kopf (bei Markdown mit Umbruch der Normalfall) wurde gepinnt, solange seine letzte Zeile noch zu sehen war, und stand
dann doppelt da — gepinnt wird jetzt erst, wenn alle seine Zeilen hinaus sind. (2) Die Minimap bekam die Klammerfarben
nie. (3) `clearOfSticky` sucht nur noch aufwärts und hält damit auch eine Pixelposition mitten in einer Zeile frei.
Dazu: Mausrad über Köpfen/Minimap rechnet Zeilen- und Seiten-Modus um, ein Ziehen, das außerhalb endet, lässt den
Schieber nicht hell hängen. Bekannt und gelassen: im Split-Diff pinnt jede Seite ihre eigenen Köpfe (nahe einem Hunk
evtl. verschieden viele); eine Datei ab 50 000 Zeilen unter 2 MB hat keine Sticky-Köpfe (der Gutter fragt dort
zeilenweise), ohne Hinweis.

**ED5.7 — gebaut (2026-09-27):** die Projektsuche (T4, T5, T13, T14). Rust: `axiomata-files::search` — dieselben
Dateien wie ⌘P (`index::walk_files`, jetzt geteilt: keine Dotfiles, `node_modules`/`target`, `.gitignore`), eingeengt
durch „include“/„exclude“-Globs (`ignore`-Overrides); gelesen wird jede Datei über `read_text`, also durch den Wächter
mit gepinntem Elternordner, binäre Dateien und solche über 16 MiB fallen heraus. Das Muster ist das des `regex`-Crates
(ohne Backtracking, Größengrenze 16 MiB; wörtliche Eingabe maskiert, „ganzes Wort“ `\b…\b`, schon über Tauri im
Lockfile, kein neues Crate). Treffer über Zeilen hinweg werden an ihrer ersten Zeile gemeldet; Spalten in UTF-16,
eine lange Zeile als 240-Einheiten-Fenster um den Treffer. Höchstens 10 000 Treffer (`truncated`), neue Fehlerart
`BadPattern`. Tauri `file_search` streamt über einen `ipc::Channel` in Bündeln (32 Dateien oder 50 ms), `Searches`
merkt je Suchfeld (`owner`) die laufende Suche, eine neue bricht die alte ab; `file_search_cancel`. CLI `files search`.
Frontend: `fileapp/projectSearch.ts` (Modell mit Generationen, spätere Bündel einer alten Suche werden verworfen),
`fileapp/ProjectSearch.svelte` (eine Komponente für beide Orte: Suchfeld mit Aa/ab/.*, einklappbare Globs, Wurzelwahl,
Ergebnisse nach Datei gruppiert und einklappbar, Treffer markiert; sucht beim Tippen nach 250 ms, ⏎ sofort; ein Klick
öffnet an der Zeile). Datei-App: linke Spalte mit Reitern **Files | Search**, ⇧⌘F; Wurzelwahl über die Baum-Wurzeln
(eine Wurzel je Suche — Umsetzung von „Wurzel-Filter“). IDE: Pane-Art `search` im „+“-Menü, ⇧⌘F holt ihn nach vorn
oder öffnet ihn in der Gruppe des Files-Panes, aufs Projekt beschränkt; ein Treffer öffnet einen Datei-Pane daneben.
„●“ für ungespeicherte Dateien kommt aus `fileapp/dirtyFiles.ts`, das jeder `FileEditor` pflegt (Tab, Panel,
IDE-Pane). Browser-Test (Mock): Datei-App und IDE, ⇧⌘F, Treffer öffnet an der Zeile; CLI auf dem echten Workspace.
Reviews (erstmals schlank: volle Sicherheitsprüfung + ein kombiniertes Sonnet-Review) — behoben: (1) **HIGH, Sicherheit**:
die Globs liefen als `ignore`-Overrides in den Durchlauf, und ein Override *erlaubt*, was er trifft, an Dotfile- und
`.gitignore`-Regeln vorbei — `include: [".env"]` hätte Geheimnisse gelesen. Jetzt filtern die Globs (`globset`, schon
über `ignore` im Lockfile) nur, was der Durchlauf liefert, und können ihn nur einengen; `walk_files` nimmt gar keine
Overrides mehr an; Regressionstest. Ein Glob ohne `/` trifft den Namen in jedem Ordner. (2) Höchstens drei Suchen lesen
gleichzeitig (Semaphore in `Searches`), damit eine Flut aus dem Webview die Blocking-Threads der anderen Datei-Befehle
nicht belegt. (3) Ein aus dem Layout wiederhergestellter IDE-Search-Pane holte sich beim Start den Fokus. (4) Genau
10 000 Treffer hießen „abgeschnitten“ — jetzt erst, wenn ein weiterer Treffer es beweist. (5) Ein kaputter Glob wird als
Glob benannt, nicht als Muster; Glob-Fehler tragen keinen Pfad.

**ED5.8 — gebaut (2026-09-27):** installierte Mac-Schriften (T10, T16). `axiomata-macos::fonts` fragt CoreText nach
allen Schnitten (`CTFontCollectionCreateFromAvailableFonts`), direkt über `unsafe extern "C"` gegen die System-Frameworks
— kein neues Crate, wie der Rest von `axiomata-macos`; jedes Create/Copy-Objekt wird genau einmal freigegeben
(`Owned`). Je Familie die CSS-Gewichte ihrer aufrechten Schnitte (CoreTexts -1…1 auf den nächsten von Apples benannten
Schnitten, wie WebKit) und ob sie monospaced ist; Kursive, private Systemschriften (`.`-Namen) und Namen, die aus
einem CSS-String ausbrechen könnten, fallen heraus (`usable_name`, im Frontend dieselbe Regel `usableFamilyName`).
Tauri `installed_fonts` rechnet einmal je App-Lauf (CoreText braucht im Debug-Build ~2,6 s; neu installierte
Schriften nach Neustart). Frontend: `core/installedFonts.ts` (Store, einmal gefragt), `fileapp/fonts.ts` kennt die
Gewichte installierter Familien, `drawnFamily` zeichnet eine nicht (mehr) installierte gewählte Schrift still mit
der Vorgabe; die Einstellungen nehmen jede sichere Familie an. Einstellungen: Gruppen „Bundled“ / „Installed on this
Mac“, „Only monospaced fonts“ (an), Hinweis bei fehlender Schrift. Terminal (T16): dieselbe Liste, nur monospaced, als
weitere Gruppe im Schnellwahl-Menü über dem freien Feld. Browser-Test (Mock): Gruppen, Auswahl, Rückfall auf die
Vorgabe; der Hinweistext selbst nur per Unit-Test (`drawnFamily`).
Review (kombiniert, Sonnet): FFI ohne Befund (jedes Create/Copy genau einmal freigegeben, Typprüfung vor jedem Lesen,
Zahlentypen passend). Behoben: die TS-Regel ließ C1-Steuerzeichen (U+0080–U+009F) durch, die Rusts `is_control`
abweist, und zählte die Länge in UTF-16 statt in UTF-8-Bytes — jetzt dieselbe Regel auf beiden Seiten, mit Test.

**ED5.9 — gebaut (2026-09-27):** Verschieben im Baum (T11). `FileTree.svelte` (damit auch der IDE-Files-Pane): eine
Datei oder ein Ordner, gezogen und auf einen Ordner derselben Wurzel losgelassen, zieht dorthin um — über das schon
vorhandene `file_rename` (gepinnte Elternordner, `RENAME_NOREPLACE`, `.git` nie), offene Kopien folgen über
`files:renamed`. Ziehen mit Pointer-Events statt HTML-Drag-and-drop, das Tauris Fenster fürs Ablegen aus dem Finder
abfangen kann (dieselbe Wahl wie bei den IDE-Tabs); ab 5 px Bewegung ist es ein Ziehen, der Klick danach wird
geschluckt, Esc bricht ab. Ziel ist die Zeile unter dem Zeiger (`data-drop-root`/`data-drop-dir`): ein Ordner, der
Ordner einer Datei oder eine Wurzel; ein zugeklappter Ordner öffnet sich nach 500 ms. `treeModel.moveTarget` entscheidet
(Ziel, schon dort / auf sich selbst → nichts, andere Wurzel oder in sich selbst → abgelehnt mit Meldung); der Geist am
Zeiger ist durchgestrichen, wo es nicht geht. Rust: `rename_entry` lehnt einen Ordner in sich selbst ausdrücklich ab
(statt `EINVAL`). Browser-Test (Mock): `README.md` → `src/`, Ablehnung über Wurzeln, Aufklappen beim Verweilen.
Sicherheitsprüfung (voll): nur LOW. Über Wurzeln hinweg ist ein Verschieben schon im Befehl unmöglich (ein `root` für
beide Pfade), „in sich selbst“ verhindert ohnehin der Kernel; die Meldung dafür vergleicht jetzt Pfad-Komponenten statt
roher Zeichenketten (`./src/x`, `src//x`). Kein zusätzliches Sonnet-Review: der Diff ist klein, die Sicherheitsprüfung
deckt ihn ab.

### ED6 im Detail (gegrillt 2026-09-28, Q0–Q11, bestätigt; Umsetzung begonnen)

Vorlauf und zwölf Entscheidungen (L0–L11). D18 bleibt der Rahmen (Server auf dem `PATH` erkannt, einer
pro Wurzel und Sprache, Reihenfolge Diagnosen → Hover → Definition → Vervollständigung →
Formatieren/Umbenennen); D6 wird in einem Punkt geändert (L2).

- **L0 — Vorlauf `$HOME`** (Q0): `normalize_root` lehnt als IDE-Projektwurzel `$HOME` selbst und alles
  darüber (`/Users`, `/`) ab, mit klarer Meldung. Alles *innerhalb* des Home-Ordners bleibt erlaubt
  (`~/Documents`, …). Grund: ein Projekt „Home“ gäbe dem Webview das ganze Home, und ein Sprachserver
  würde es indizieren. Erledigt damit die Owner-Notiz zur `$HOME`-Wurzel. **Umgesetzt** (`store::too_wide`).
- **L1 — Aufgeteilt** (Q1): Rust (`axiomata-files`) startet/beendet die Server, macht das
  `Content-Length`-Framing und reicht ganze JSON-Nachrichten über einen Tauri-Kanal durch; das Protokoll
  (Initialisierung, Dokument-Synchronisation mit Rope-Versionen, UTF-16-Positionen, Antworten) lebt in
  TypeScript unter `src/editor/lsp/` — ohne DOM/Svelte, extrahierbar wie der Rest der Engine (D1).
- **L2 — Welche Befehle laufen, entscheidet Rust** (Q2): eine eingebaute Tabelle bekannter Server, auf dem
  `PATH` erkannt; Überschreiben nur in `~/.axiomata/lsp.json`, die **kein** Tauri-Befehl schreibt (von
  Hand, oder im Editor über den Datei-Dienst mit Dialog-Freigabe). **Ändert D6:** keine LSP-Befehle in
  `editor-settings.json` — die schreibt der Webview, und ein kompromittierter Webview dürfte sonst
  Programme starten lassen.
- **L3 — Wo** (Q3): Datei-App im Vollbild und IDE-Pane. Nicht im schwebenden Panel und nicht in der
  Second-Brain-Vorschau (kurze Blicke; wie Minimap/Sticky Scroll dort aus).
- **L4 — Wurzeln und Lebensdauer** (Q4): ein Server pro (Wurzel, Sprache), gestartet mit der ersten Datei
  seiner Sprache in dieser Wurzel, beendet nach 10 Minuten ohne offene Datei der Wurzel. Workspace,
  Projekt und Worktree gleich behandelt (jeder Worktree hat seinen Stand); `grant:`-Einzeldateien
  bekommen keinen.
- **L5 — Checkpoints** (Q5): **ED6.1** Server-Verwaltung + Protokoll-Basis + **Diagnosen**; **ED6.2**
  **Hover** + **Definition** (auch in andere Dateien, als Reiter); **ED6.3** **Vervollständigung**;
  **ED6.4** **Formatieren** + **Umbenennen** (Änderungen über mehrere Dateien; nicht offene Dateien über
  den geschützten Datei-Dienst).
- **L6 — Diagnosen** (Q6): Wellenlinie je Schwere (Farben über `--ax-*`), Marke am Zeilenrand, Meldung
  beim Überfahren; Einstellung „Meldung am Zeilenende“, Vorgabe aus.
- **L7 — Tasten** (Q7): normal F12/⌘-Klick Definition, Überfahren Hover, F8/⇧F8 Probleme, ⌃Space
  Vervollständigung, ⇧⌥F Formatieren, F2 Umbenennen; Vi `gd`, `K`, `]d`/`[d`, ⌃Space im Insert,
  `:format`, `:rename <name>`.
- **L8 — Formatieren beim Speichern** (Q8): Einstellung, Vorgabe aus.
- **L9 — Snippets** (Q9): Platzhalter werden als ihr Vorgabetext eingesetzt, der Cursor steht auf dem
  ersten; Tab-Springen durch Platzhalter folgt später.
- **L10 — Sprachen und Abnahme** (Q10): die Tabelle deckt die 15 tree-sitter-Sprachen ab, wo ein gängiger
  Server existiert; fehlt einer, ein ruhiger Hinweis mit dem Installationsbefehl. Live abgenommen mit
  `rust-analyzer` und `pyright`; TypeScript/Svelte, sobald der Owner sie installiert — nichts wird
  ungefragt installiert.
- **L11 — Definition außerhalb der Wurzeln** (Q11): schreibgeschützt öffnen, aber nur Pfade, die der
  Server selbst in einer Definitions-Antwort genannt hat (Rust liest diese eine Antwortart mit — eine
  gezielte Ausnahme von L1), reguläre Datei, keine Verknüpfung. Der Webview kann keinen Pfad unterschieben.

**ED6.1 umgesetzt (2026-09-28):** `axiomata-files::lsp` (Tabelle + `lsp.json`, `LspHost` mit Framing,
Seitenkennung — dieselbe Seite teilt einen Server, eine neu geladene startet ihn neu —, 10-Minuten-Leerlauf),
Tauri `lsp_start/send/opened/closed` (keine `grant:`-Wurzel), Engine `src/editor/lsp/` (`rpc`, `client`
mit inkrementeller Synchronisation, `diagnostics`), `fileapp/lsp.ts`, Anzeige in `EditorSurface`
(Wellenlinie, Randpunkt, Hover, Meldung am Zeilenende als Einstellung), F8/⇧F8 und Vi `]d`/`[d`,
Zähler in der Statuszeile. Sicherheitsprüfung (HIGH behoben): Rust lässt nur die Methoden durch, die
der Client spricht (`ALLOWED_METHODS`, wächst je Checkpoint), plus Antworten auf Server-Anfragen —
`workspace/executeCommand` und Co. bleiben draußen; `lsp_send/opened/closed` nur für die startende
Seite; höchstens 8 Server, Neustart durch eine andere Seite höchstens alle 2 s; ausgehend ≤ 64 MiB,
geschrieben ohne die Host-Sperre; ohne auflösbares `$HOME` keine Projektwurzel. Review: zwei Editoren
auf einer Datei werden gezählt (erst das letzte Schließen meldet `didClose`), Starts je Wurzel laufen
nacheinander (sonst hing ein zweiter TS/TSX-Client in „starting“). Gefunden beim Bau: der App fehlt aus dem Finder der Shell-`PATH` — die
Servererkennung sucht zusätzlich an den üblichen Installationsorten. Live gegen echte Server geprüft
(`tests/lsp_live.rs`: `rust-analyzer` E0308, `pyright` reportUndefinedVariable). Offen: Live-Test in der App.

## 6. Verifikation (pro Meilenstein)

- Das TS-Paket ist von ED1 an ohne DOM testbar (`vitest`): Puffer, Undo, Cursor, später
  der komplette Vi-Automat als Tabelle „Tasten → erwarteter Text und Cursor".
- `axiomata-files` mit echten temporären Verzeichnissen getestet, besonders die
  Wurzel-Prüfung (Pfad-Ausbruch, Symlinks) — `security-auditor` ist ab ED0 Pflicht.
- Browser-Weg über `devmock.ts`, Live-Test am echten Mac für alles, was mit Gefühl zu
  tun hat (Tippen, Eingabemethode, Cursor-Animation, Scrollen) — ohne Oberflächen-
  Änderungen während des Tests.
