# Eigenes Terminal-Modul

Status (2026-09-14): Checkpoints 0–4 sind umgesetzt und automatisiert
verifiziert (Engine-/Tauri-/Frontend-Tests, Sub-Agent-Reviews); der volle
interaktive Live-Test (Scrollback-Gefühl, `vim`/`htop` im Alternate-Screen,
Auswahl+Copy, Paste, mehrere gleichzeitige Terminals) steht noch aus — der
Owner testet das bei nächster Gelegenheit am Mac selbst. Checkpoint 5 ist
teilweise umgesetzt ("Mehrere Instanzen" brauchte keinen Code, Konfiguration
Schriftgröße/Shell-Wahl ist da); Performance-Tuning bei sehr hohem Output
ist noch offen. Checkpoint 5b (Settings-Erweiterung, siehe unten) ist der
aktuelle Arbeitsstand — Block A ist committet (`76a5715`); Block B ist
umgesetzt und steht vor seinem eigenen Verifikations-/Commit-Durchlauf.

Dieses Dokument ist bewusst so detailliert geschrieben, dass einzelne
Checkpoints auch ohne den ursprünglichen Chat-Kontext umsetzbar sind — z. B.
über den `opencode`-Harness mit einem günstigeren Modell (DeepSeek Flash),
wenn Claude-Kontingent gerade knapp ist. Phase 0 und 1 waren von Anfang an
vollständig spezifiziert; Phase 2–5 waren ein grober Fahrplan, der vor der
jeweiligen Umsetzung noch eine eigene kurze Planungsrunde bekam (gleiche
Arbeitsweise wie der Rest des Projekts, siehe `docs/architecture.md` und die
Meilenstein-Historie: ein Teil nach dem anderen, nicht alles vorab im
Detail) — das gilt unverändert für alles, was noch aussteht.

## Context

Axiomata-OS bekommt seine erste "richtige" App im Dashboard: ein Terminal.
Ursprünglich war angedacht, ein bestehendes Terminal (Ghostty) irgendwie
einzubetten — das würde aber eine weitere externe Abhängigkeit bedeuten, die
dieses Projekt bisher konsequent vermeidet (siehe z. B. die eigene
Argumentation gegen `asset://`-Einbettung in `docs/architecture.md` §5).
Der Owner hat sich stattdessen für ein **selbst implementiertes** Terminal
entschieden, aus drei Gründen:

1. **Kapselung**: Es soll so gebaut sein, dass es sich leicht aus Axiomata-OS
   herauslösen und als eigenständige Anwendung betreiben ließe — auch wenn
   das nie tatsächlich passiert, zwingt dieses Ziel zu einer sauberen
   Architektur ohne unnötige Kopplung an den Rest der App.
2. **Schrittweise**: Der Funktionsumfang wächst über mehrere, einzeln
   geplante Phasen, nicht alles auf einmal.
3. **Lernobjekt**: Der Owner möchte dabei selbst verstehen, wie ein
   Terminal-Emulator grundsätzlich funktioniert (PTYs, ANSI/VT100,
   Bildschirm-Zustandsmodell). Deshalb wird bewusst nicht auf eine
   fertige Terminal-Emulator-Bibliothek wie `xterm.js` zurückgegriffen (die
   genau die lehrreichen Teile — ANSI-Interpretation, Bildschirm-Modell —
   komplett verstecken würde), sondern nur auf kleine, gut abgegrenzte
   Bausteine für die technisch uninteressanten Teile (PTY-Verwaltung,
   Byte-Zerlegung von Escape-Sequenzen).

**Arbeitsweise für dieses Projekt** (Owner-Vorgabe): Claude schreibt den Code
und erklärt nur auf Nachfrage tiefer — kein automatisches Abarbeiten langer
Erklär-Exkurse, aber auf Wunsch jederzeit möglich.

## Architektur-Überblick

Drei Schichten, nach dem im Projekt bereits etablierten Muster "reiner
Rust-Kern (`crates/axiomata-core`) + dünner Tauri-Kleber
(`apps/dashboard/src-tauri`) + Frontend" — hier neu für die Terminal-Domäne:

```
crates/axiomata-terminal/          Neue, eigenständige Rust-Crate.
  │                                 KEINE Abhängigkeit zu axiomata-core oder
  │                                 zu Tauri — reines PTY- + Terminal-Domain-
  │                                 Wissen, für sich allein unit-testbar und
  │                                 sogar als CLI-Programm lauffähig.
  ├─ PTY spawnen & Shell-Prozess verwalten (`portable-pty`-Crate)
  ├─ ANSI/VT100-Bytes in Events zerlegen (`vte`-Crate)
  └─ Terminal-Zustandsmaschine: Bildschirm-Grid, Cursor, Scrollback
     (komplett selbst geschrieben — hier steckt der Lernwert)

apps/dashboard/src-tauri/src/terminal.rs    Neue, eigene Datei (NICHT in die
  │                                          schon sehr große commands.rs
  │                                          hineinwachsen lassen).
  ├─ hält eine Registry laufender Terminal-Sessions (Tauri-managed State,
  │  gleiches Muster wie `CoreState`/`SchedulerHandle` in lib.rs)
  ├─ ein paar #[tauri::command]s: spawnen, schreiben, resizen, schließen
  └─ streamt PTY-Ausgabe zum Frontend über einen Tauri-v2-`Channel`
     (die IPC-Streaming-API, extra für genau solche Fälle gedacht — statt
     vieler einzelner `invoke`/Event-Aufrufe für jedes Byte-Häppchen)

apps/dashboard/src/modules/terminal/        Neuer, eigenständiger Modul-
  │                                          Ordner (folgt dem bestehenden
  │                                          ModuleDefinition-Vertrag aus
  │                                          core/types.ts).
  ├─ terminal.svelte           Front-Face-Komponente (component in ModuleDefinition)
  ├─ TerminalScreen.ts         Canvas-Rendering des Bildschirm-Grids
  ├─ terminalProtocol.ts       Dünner Client für die Tauri-Befehle/den Channel
  └─ rührt NUR den bestehenden, bereits schmalen `ModuleContext`
     (instanceId, config, invoke, emit, requestResize) an — keine tieferen
     Axiomata-internen Importe (core/stores.ts, core/persist.ts etc.)
```

**Kapselungsprinzipien** (gelten für jede Phase, nicht nur Phase 0):

- Die Crate `axiomata-terminal` bekommt **niemals** `tauri`, `axiomata-core`
  oder irgendetwas Tauri-Spezifisches als Abhängigkeit. Sie kennt nur PTYs,
  Bytes und ihr eigenes Bildschirm-Modell.
- `src-tauri/src/terminal.rs` übersetzt **nur** zwischen Tauri-IPC und der
  Engine-Crate — keine Terminal-*Logik* darf hier landen, nur Verkabelung
  (Session-Registry, Fehler-zu-String-Mapping wie überall sonst in
  `commands.rs`).
- Das Frontend-Modul importiert aus `core/*` nur, was jedes Modul ohnehin
  über `ModuleContext` bekommt, plus ggf. Theming-Tokens (`--ax-*`
  CSS-Variablen, rein visuell, keine Logik-Kopplung).

## Technologie-Entscheidungen (mit Begründung)

| Baustein | Wahl | Warum |
|---|---|---|
| PTY-Verwaltung | [`portable-pty`](https://crates.io/crates/portable-pty) (aus dem wezterm-Projekt) | Rohes `openpty`/`fork`/`exec` von Hand zu schreiben lehrt wenig über *Terminals* (nur über POSIX-Prozessverwaltung) und ist eine reale Fehlerquelle (unsafe Syscalls). |
| ANSI-Byte-Zerlegung | [`vte`](https://crates.io/crates/vte) (dieselbe Crate, die Alacritty nutzt) | Das Tokenizing von Escape-Sequenzen (Zwischenbytes, private Marker, OSC-Terminatoren) ist notorisch fummelig und fehleranfällig, lehrt aber wenig über das eigentliche Terminal-Verhalten. Was mit einem geparsten Event *passiert* (Cursor bewegen, Farbe setzen, scrollen) schreiben wir komplett selbst — genau dort liegt der Lernwert. |
| Zeichenbreite (CJK/Emoji) | [`unicode-width`](https://crates.io/crates/unicode-width) | Notwendig für ein korrektes Zellgitter; reine Nachschlagetabellen-Logik ohne eigenen Erkenntniswert. |
| Rendering | Canvas-Grid (eigene Zeichenroutine, kein `<div>`-Gitter) | Konsistent mit dem bereits im App Ring genutzten Muster (`ctx.fillText`, Redraw-Loop); Vorbilder wie xterm.js/Alacritty/Kitty rendern ebenfalls über Canvas/GPU, gerade wegen der Performance bei hohem Output (z. B. `cat` einer großen Datei). Textauswahl wird bewusst erst in Phase 4 nachgebaut. |
| Farbtiefe | 24-Bit-Truecolor als Speicherformat | Moderne CLI-Tools nutzen das längst (git, eza, viele TUI-Programme); klassische 16-/256-Farb-Codes werden einfach auf konkrete RGB-Werte abgebildet, das Datenmodell muss später nicht erweitert werden. |
| Frontend↔Backend-Transport | Tauri-v2-`Channel` für die Ausgabe, normales `invoke` für Eingabe/Resize | `Channel` ist die für Byte-Streams gedachte IPC-API in Tauri v2, effizienter als viele Einzel-Events. |
| `TERM`-Wert für die Shell | `xterm-256color` | De-facto-Standard, zu dem praktisch jedes Programm sinnvolle Sequenzen sendet, ohne Funktionen vorzutäuschen (z. B. Kitty-Grafikprotokoll), die wir nicht unterstützen. |
| Shell-Auswahl | `$SHELL`-Umgebungsvariable, Fallback `/bin/zsh` | Respektiert die Nutzer-Shell von Anfang an, kostet in Phase 0 praktisch nichts extra. |
| Session-Lebenszyklus (v1) | Shell-Prozess wird beim Schließen der Kachel sofort beendet | Einfach für den Start; andockbare Sessions (wie `tmux`/`screen`) sind ein mögliches späteres Feature, kein Blocker für ein funktionierendes Terminal. |
| Mehrere Terminals | Eine Shell-Sitzung pro platzierter Kachel, kein eigenes Tab-System im Modul | Nutzt den bestehenden Mehrfach-Instanzen-Mechanismus des Dashboards (wie bei anderen Modulen schon), statt ihn im Terminal-Modul nachzubauen. |

## Checkpoint 0 — Engine-Crate-Skelett + eigenständiges CLI-Testbinary

**Ziel**: Beweisen, dass die Rust-Engine für sich allein steht — PTY
spawnen, eine echte Shell darin laufen lassen, Bytes roh durchreichen, noch
**ohne** ANSI-Verständnis. Kein UI, kein Tauri.

**Neue Dateien**:
- `crates/axiomata-terminal/Cargo.toml` — neues Workspace-Mitglied. In der
  Root-`Cargo.toml` unter `[workspace] members` ergänzen (Muster:
  `"crates/*"` deckt das schon automatisch ab, da `crates/*` als Glob
  eingetragen ist — prüfen, ob das für neue Unterordner tatsächlich
  automatisch funktioniert oder ob `crates/axiomata-terminal` explizit
  gelistet werden muss).
- `crates/axiomata-terminal/src/lib.rs` — öffentliche API der Crate.
- `crates/axiomata-terminal/src/pty.rs` — PTY-Spawning via `portable-pty`.
- `crates/axiomata-terminal/src/bin/term-poc.rs` — das eigenständige
  Testbinary (`cargo run -p axiomata-terminal --bin term-poc`): öffnet ein
  PTY, spawnt `$SHELL` darin, verbindet stdin/stdout des aufrufenden
  Prozesses roh mit dem PTY (ein minimaler "Terminal im Terminal"-Fall,
  ganz ohne ANSI-Interpretation — die Escape-Sequenzen der Shell laufen
  einfach unverändert durch, das aufrufende echte Terminal interpretiert
  sie ja bereits selbst).

**Cargo-Abhängigkeiten** (in `crates/axiomata-terminal/Cargo.toml`):
```toml
[dependencies]
portable-pty = "0.8"  # Version beim Umsetzen gegen crates.io prüfen/aktualisieren
```

**Öffentliche API von `lib.rs`** (Skizze, Details beim Umsetzen verfeinern):
```rust
/// Ein laufender PTY + Shell-Prozess.
pub struct PtySession {
    // portable-pty's PairMaster + Child-Handle
}

impl PtySession {
    /// Spawnt `$SHELL` (Fallback `/bin/zsh`) mit `TERM=xterm-256color` in
    /// einem neuen PTY der gegebenen Größe (Zeilen × Spalten).
    pub fn spawn(rows: u16, cols: u16) -> std::io::Result<Self> { .. }

    /// Schreibt Eingabe-Bytes an die Shell (Tastatureingaben, bereits in
    /// die richtigen Escape-Bytes übersetzt).
    pub fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> { .. }

    /// Liest verfügbare Ausgabe-Bytes von der Shell (blockierend oder mit
    /// eigenem Reader-Thread — beim Umsetzen entscheiden, vermutlich ein
    /// dedizierter Thread, der über einen Channel an den Aufrufer liefert,
    /// da `portable-pty`s Master-Reader blockierend ist).
    pub fn resize(&mut self, rows: u16, cols: u16) -> std::io::Result<()> { .. }
}
```

**Abnahmekriterien**:
- `cargo build -p axiomata-terminal` kompiliert sauber, `cargo clippy -p
  axiomata-terminal -- -D warnings` ohne Warnungen.
- `cargo run -p axiomata-terminal --bin term-poc` öffnet eine funktionierende
  Shell — man kann Befehle eintippen, Ausgabe erscheint, `exit` beendet das
  Programm sauber. Probehalber auch ein Programm mit einfachen ANSI-Farben
  laufen lassen (z. B. `ls -G` oder `echo -e "\e[31mrot\e[0m"`) — die Farb-
  Escape-Sequenzen werden zu diesem Zeitpunkt einfach unverändert
  durchgereicht (das äußere, echte Terminal interpretiert sie), das ist
  erwartetes Verhalten für Checkpoint 0, keine Regression.
- Mindestens ein Unit-Test, der `PtySession::spawn` + `write` +
  Ausgabe-Lesen gegen eine deterministische, nicht-interaktive Shell-
  Eingabe prüft (z. B. `echo hallo` schreiben, `"hallo\n"` in der Ausgabe
  erwarten) — kein Test, der auf ein echtes TTY/eine interaktive Sitzung
  angewiesen ist.

## Checkpoint 1 — Minimaler End-to-End-Durchstich (reiner Text, kein ANSI)

**Ziel**: Die komplette Pipeline (Rust-Engine → Tauri → Svelte-Modul) einmal
im echten Dashboard zum Laufen bringen — noch ohne Bildschirm-Modell, ohne
Farben, ohne Cursor-Positionierung. Rohe Bytes werden 1:1 als Text an ein
`<pre>`-ähnliches Element angehängt (kein Canvas-Rendering noch — das kommt
erst in Checkpoint 3, wenn es ein echtes Bildschirm-Modell gibt, aus dem sich
sinnvoll etwas zeichnen lässt).

**Neue/geänderte Dateien**:
- `apps/dashboard/src-tauri/Cargo.toml`: `axiomata-terminal = { path =
  "../../../crates/axiomata-terminal" }` als Abhängigkeit ergänzen.
- `apps/dashboard/src-tauri/src/terminal.rs` (neu):
  - `TerminalSessions` — Tauri-managed State, hält eine
    `HashMap<String, axiomata_terminal::PtySession>` (Session-Id → Session),
    hinter einem `Mutex` (gleiches Muster wie andere gemeinsam genutzte
    States in diesem Projekt).
  - `#[tauri::command] fn terminal_spawn(sessions: State<TerminalSessions>,
    rows: u16, cols: u16, on_output: Channel<Vec<u8>>) -> Result<String,
    String>` — spawnt eine neue Session, gibt eine Session-Id zurück, startet
    einen Hintergrund-Thread/Task, der PTY-Ausgabe liest und über den
    `Channel` ans Frontend streamt.
  - `#[tauri::command] fn terminal_write(sessions: ..., id: String, data:
    Vec<u8>) -> Result<(), String>`.
  - `#[tauri::command] fn terminal_resize(sessions: ..., id: String, rows:
    u16, cols: u16) -> Result<(), String>`.
  - `#[tauri::command] fn terminal_close(sessions: ..., id: String) ->
    Result<(), String>` — beendet den Shell-Prozess, entfernt die Session
    (Checkpoint-0-Entscheidung: sofort beenden, kein Andocken).
- `apps/dashboard/src-tauri/src/lib.rs`: die vier neuen Befehle in
  `generate_handler!` registrieren, `app.manage(terminal::TerminalSessions::default())`
  im `.setup()`.
- `apps/dashboard/src/modules/terminal/terminal.svelte` (neu): minimale
  Komponente — ruft beim Mounten `terminal_spawn` auf (Kachel-Größe grob in
  Zeilen/Spalten umgerechnet, exakte Zeichen-Metrik-Berechnung kommt erst in
  Checkpoint 3), hängt jedes über den `Channel` ankommende Byte-Häppchen als
  Text an ein scrollendes `<pre>`-Element an, ein `<input>` (oder
  `contenteditable`) leitet Tastatur-Eingaben roh an `terminal_write`
  weiter. Ruft `terminal_close` beim Zerstören der Komponente (`onDestroy`).
- `apps/dashboard/src/core/registry.ts` (oder wo auch immer Module aktuell
  registriert werden — beim Umsetzen den echten Registrierungsort
  verifizieren): neuer Eintrag mit `type: "terminal"`, `icon`: das
  `"terminal"`-Glyph aus `graph/render.ts`s `GLYPH_CODEPOINTS` gibt es
  schon (von der heutigen Material-Symbols-Umstellung) — für die
  `ModuleDefinition.icon`-SVG-Anforderung trotzdem ein eigenes kleines
  Inline-SVG bauen oder klären, ob `ModuleDefinition.icon` auch anders
  befüllt werden kann; `defaultSize` grob `{ w: 640, h: 400 }`,
  `singleton: false`.

**Abnahmekriterien**:
- `npm run check` und `npx vitest run` bleiben sauber.
- Im echten `cargo tauri dev`: Terminal-Kachel platzierbar (Modul-Auswahl-
  Dialog zeigt "Terminal"), zeigt nach dem Platzieren eine laufende Shell,
  Eingaben (auch mit Enter) kommen an, Ausgabe erscheint (unformatiert,
  Escape-Codes evtl. als sichtbarer Zeichenmüll — erwartet, wird erst in
  Checkpoint 2 behoben). Kachel schließen beendet den Shell-Prozess
  (prüfbar z. B. über `ps aux | grep zsh` vor/nach dem Schließen).

## Fahrplan für die weiteren Phasen (Umfang grob, Detailplanung folgt vor dem jeweiligen Start)

**Checkpoint 2 — ANSI/VT100-Interpretation**: `vte`-Parser einbinden,
Bildschirm-Zustandsmaschine (Zellgitter mit Zeichen + RGB-Vorder-/
Hintergrundfarbe + Attributen, Cursor-Position) bauen, grundlegende
SGR-Sequenzen (Farben, fett, unterstrichen) sowie Cursor-Bewegungssequenzen
interpretieren. Ab hier verschwindet der Escape-Code-Zeichenmüll aus
Checkpoint 1.

**Checkpoint 3 — Sauberes Rendering**: Canvas-Grid-Renderer, der das
Bildschirm-Modell aus Checkpoint 2 zeichnet (Monospace-Font, `--ax-font-mono`-
Token), echte Zeichen-Metrik-basierte Zeilen/Spalten-Berechnung aus der
Kachel-Größe (ersetzt die grobe Schätzung aus Checkpoint 1), Cursor-Anzeige
(blinkend), Resize-Reflow inkl. `SIGWINCH`-Propagation zur Shell.

**Checkpoint 4 — Scrollback, Alternate-Screen, Zwischenablage**:
Scrollback-Ringpuffer mit Größenlimit, Alternate-Screen-Unterstützung
(nötig für vim/less/htop & Co., die den Bildschirm beim Beenden wieder
"zurückgeben"), Textauswahl per Maus, Copy/Paste (inkl. Bracketed Paste,
damit eingefügter Text nicht versehentlich als Tastatureingaben interpretiert
wird).

**Checkpoint 5 — Feinschliff**: mehrere gleichzeitige Terminal-Instanzen im
Alltag ausgiebig testen, Konfiguration (Schriftgröße, ggf. Shell-Wahl) über
die `settings`-Rückseite des Moduls, Performance-Tuning bei sehr hohem
Output (z. B. `yes` oder große Log-Dateien), ggf. offene Kleinigkeiten aus
den vorherigen Checkpoints.

## Checkpoint 5b — Settings-Erweiterung (Owner-Wunschliste, 2026-09-14)

Erweitert die `terminal-settings.svelte`-Rückseite aus Checkpoint 5 um eine
Reihe weiterer Einstellungen — vom Owner freigegeben ("all deine Vorschläge
setzen wir um"). In zwei Blöcken umgesetzt, weil sie unterschiedlich tief in
den Stack reichen; jeder Block bekommt den üblichen Verifikations-/
Sub-Agent-Durchlauf und einen eigenen Commit, kein großer Rutsch.

**Block A — reicht bis in die Engine-Crate (`PtySession`/`Screen`):**
- **Scrollback-Größe**: `SCREEN_LIMIT`/`SCROLLBACK_LIMIT` ist aktuell eine
  feste Konstante (`crates/axiomata-terminal/src/screen.rs`) — wird ein
  Konstruktor-Parameter von `Screen::new`/`Terminal::new`, mit dem
  bisherigen Wert (2000) als Default, wenn das Config-Feld leer ist.
- **Start-Verzeichnis der Shell**: `PtySession::spawn` setzt aktuell kein
  `cwd` (`CommandBuilder`s eigener Default greift — vermutlich das
  Home-Verzeichnis oder das der App, nicht klar definiert). Neuer
  `cwd_override: Option<&Path>`-Parameter, analog zu `shell_override`
  (ungültiger Pfad = Spawn-Fehler, kein stiller Fallback). Der Standard,
  den die Einstellungsseite vorschlägt (aber nicht erzwingt): der
  Second-Brain-Workspace-Root — das Frontend müsste dafür `config.workspace_root`
  (schon Teil von `get_app_info`, siehe `apps/dashboard/src-tauri/src/commands.rs`)
  kennen; prüfen, ob `ModuleContext` das schon hergibt oder ob
  `terminal.svelte` dafür `get_app_info` selbst aufrufen muss (kein
  Präzedenzfall bisher — andere Module lesen so etwas nicht direkt).
- **Eigene Umgebungsvariablen**: `PtySession::spawn` bekommt eine
  `extra_env: &[(String, String)]`-artige Liste, angewandt nach `TERM`
  (damit ein eigener `TERM`-Eintrag in der Liste bewusst gewinnen kann,
  falls das je gewünscht ist — sonst gewinnt der letzte `cmd.env()`-Aufruf
  ohnehin, das ist portable-pty/`CommandBuilder`s eigenes Verhalten, nur
  hier explizit festgehalten). UI-seitig eine einfache
  `KEY=value`-pro-Zeile-Textarea, keine strukturierte Key/Value-Tabelle
  (Aufwand steht in keinem Verhältnis zum Nutzen bei aktuell einem
  Terminal-Modul).

**Block B — reine Darstellung, nur `TerminalScreen.ts`/`terminal.svelte`/
`terminal-settings.svelte`, keine Engine-/Tauri-Änderung nötig:**
- **Cursor-Stil**: `block` (aktuell, einzige Option), `outline` (nur
  Rahmen, kein Fill), `underline`, `bar` (schmaler vertikaler Strich am
  linken Zellrand) — ein `cursorStyle`-Parameter in `TerminalScreen.draw`,
  vier Zeichenpfade statt der aktuellen fest verdrahteten Fill-Rect-Logik.
  Plus ein Blinken-an/aus-Schalter (der bestehende `CURSOR_BLINK_MS`-Takt
  bleibt, wird nur übersprungen wenn Blinken aus ist — Cursor bleibt dann
  dauerhaft sichtbar statt im Wechsel).
- **Farbschema/Theme**: `ANSI_16` in `TerminalScreen.ts` wird von einer
  festen Konstante zu einer benannten Palette unter mehreren (mind.
  xterm-Default als aktueller Ist-Zustand, dazu Solarized Dark, Dracula,
  Nord, Gruvbox Dark — allesamt öffentlich dokumentierte 16-Werte-Tabellen,
  keine Lizenzfragen). Eine `<select>` in `terminal-settings.svelte`,
  gespeichert als `config.theme` (Name-String, z. B. `"nord"`), Default
  `"xterm"` = heutiges Verhalten unverändert.
- **Bold-Text in heller Farbe**: klassische Terminal-Konvention — wenn ein
  Zeichen fett *und* mit einer der unteren 8 Indexfarben (0–7) gefärbt ist,
  wird beim Zeichnen automatisch die helle Variante (+8) verwendet. Reine
  Render-Entscheidung in `TerminalScreen.resolveColor`/`draw` (ein
  `boldIsBright: boolean`-Flag in `DrawOptions`), das Bildschirm-Modell in
  Rust bleibt unverändert (`Cell.bold` und `Cell.fg` sind ja schon getrennt
  gespeichert). Default an (gängigste Erwartungshaltung), abschaltbar.
- **Visueller Bell**: `\x07` (BEL) ist aktuell ein reines No-op
  (`Screen::execute`s `_ => {}`-Zweig). Bleibt so im Bildschirm-Modell
  (kein Zustand nötig, den `visible_rows`/`rows()` transportieren
  müssten) — stattdessen bekommt `TerminalEvent::Screen` ein
  `bell: bool`-Feld, das pro `feed()`-Aufruf anzeigt, ob seit dem letzten
  Snapshot ein BEL durchkam (`Screen` braucht dafür ein kleines
  `bell_pending`-Flag, von `execute` gesetzt und von einer neuen
  `Screen::take_bell()`-Methode gelesen+zurückgesetzt — bewusst kein
  Ringpuffer/Zähler, ein Bell zwischen zwei Snapshots reicht als Signal).
  Frontend blitzt bei `bell: true` kurz einen Rahmen/Overlay auf der
  Kachel auf. Kein Ton (Audio-Wiedergabe aus einem Hintergrund-Tauri-
  Prozess ist ein eigenes Fass, für eine Backlog-Position dieser Größe
  nicht gerechtfertigt) — abschaltbar auf "aus" für wer’s stört.
- **Eigene Schriftart**: `config.fontFamily`, analog zu `config.fontSizePx`
  aus Checkpoint 5 — überschreibt `--ax-font-mono` in `currentFont()`,
  wenn gesetzt. Kein Font-Picker mit Vorschau (Scope-Explosion für wenig
  Mehrwert) — ein einfaches Textfeld mit dem CSS-`font-family`-Freitext
  (z. B. `"Fira Code, monospace"`), genau wie bei jeder anderen
  CSS-`font-family`-Angabe.
- **Transparenz/Deckkraft**: `config.opacity` (0–100, Default 100 = heutiges
  Verhalten), wirkt NUR auf den Terminal-Hintergrund (`defaultBg` bekommt
  einen Alpha-Kanal mit angewandt, Text/Cursor/Auswahl bleiben voll
  deckend — sonst wird’s bei dunklem Text auf dunklem, transparentem
  Hintergrund schnell unlesbar). Eigenständig von der schon vorhandenen
  globalen Fenster-Transparenz der App — dieser Regler betrifft nur die
  Terminal-Kachel selbst, nicht das ganze Fenster.

**Reihenfolge**: Block A zuerst (reicht tiefer, blockiert nichts an Block B
— aber andersherum macht es wenig Sinn, z. B. das Scrollback-Limit
Rust-seitig zu bauen, bevor der Rest der Settings-Seite überhaupt die
neuen UI-Muster hat). Nach beiden Blöcken: kurzer gemeinsamer Live-Test
aller neuen Einstellungen zusammen mit dem noch offenen CP4/5-Live-Test.

## Verifikation (gesamt, pro Checkpoint anwendbar)

- `cargo build -p axiomata-terminal`, `cargo test -p axiomata-terminal`,
  `cargo clippy -p axiomata-terminal -- -D warnings`, `cargo fmt --check` —
  die Engine-Crate ist von Checkpoint 0 an eigenständig testbar, unabhängig
  vom Rest der App.
- Ab Checkpoint 1 zusätzlich: `cargo build --workspace`/`cargo test
  --workspace` (Tauri-Kleber-Schicht), `cd apps/dashboard && npm run check
  && npx vitest run`.
- Manuelle Prüfung in `cargo tauri dev` bleibt für alles, was mit echtem
  Terminal-*Gefühl* zu tun hat (Tippgefühl, Darstellung, Farben) unverzichtbar
  — automatisierte Tests decken nur das Bildschirm-Modell/die reine Logik ab,
  nicht das visuelle Ergebnis.
