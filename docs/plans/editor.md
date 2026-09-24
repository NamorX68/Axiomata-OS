# Plan: Die Datei-App — ein eigener Editor als Single Point of Truth

Status: **ED0, ED1 und ED2 fertig** (2026-09-23/24; ED2 wartet auf die Farbabnahme und den
Live-Test). Laut D15 kommt als Nächstes **M7.3 CP8/CP9** auf dem Editor, dann ED3 (Vi).

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
  - Offene Kleinigkeit für CP8: Der synchrone Bildlauf nutzt zwei verschiedene Echo-Sperren
    (Zeitfenster in der Ansicht, Frame-Flag in der Vorschau). Vereinheitlichen, wenn die
    Diff-Ansicht als dritte Stelle dazukommt.

## 6. Verifikation (pro Meilenstein)

- Das TS-Paket ist von ED1 an ohne DOM testbar (`vitest`): Puffer, Undo, Cursor, später
  der komplette Vi-Automat als Tabelle „Tasten → erwarteter Text und Cursor".
- `axiomata-files` mit echten temporären Verzeichnissen getestet, besonders die
  Wurzel-Prüfung (Pfad-Ausbruch, Symlinks) — `security-auditor` ist ab ED0 Pflicht.
- Browser-Weg über `devmock.ts`, Live-Test am echten Mac für alles, was mit Gefühl zu
  tun hat (Tippen, Eingabemethode, Cursor-Animation, Scrollen) — ohne Oberflächen-
  Änderungen während des Tests.
