# Eigenes Terminal-Modul

Status (2026-09-15): Checkpoints 0–4 sind umgesetzt und automatisiert
verifiziert (Engine-/Tauri-/Frontend-Tests, Sub-Agent-Reviews); der volle
interaktive Live-Test (Scrollback-Gefühl, `vim`/`htop` im Alternate-Screen,
Auswahl+Copy, Paste, mehrere gleichzeitige Terminals) steht noch aus — der
Owner testet das bei nächster Gelegenheit am Mac selbst. Checkpoint 5 ist
teilweise umgesetzt ("Mehrere Instanzen" brauchte keinen Code, Konfiguration
Schriftgröße/Shell-Wahl ist da); Performance-Tuning bei sehr hohem Output
ist noch offen. Checkpoint 5b (Settings-Erweiterung) ist KOMPLETT — Block A
committet (`76a5715`), Block B committet (`9f44a71`), beide automatisiert
verifiziert und durch alle vier Pflicht-Sub-Agents gegangen. Checkpoint 5d
(Bugfixes + globale Settings-Datei, `5f22b51`) ist committet. Checkpoint 5e
(zweite Live-Test-Runde: Ghosting/Clear-Fix, Startgröße 120×60, erster
Autofokus-Versuch, `f6f945e`) ist committet — der Autofokus-Teil hat beim
echten Live-Test am Mac aber NICHT gehalten (Owner-Feedback: weiterhin
Klick nötig). Checkpoint 5f (robusterer Autofokus-Fix + Gating gegen
Fokus-Diebstahl, `e415426`) ist committet — Owner-Feedback danach: der
Fokus beim Öffnen/Start funktioniert jetzt, ABER ein verbleibender Fall
war noch offen (Flip zu den Settings und zurück erforderte weiterhin
einen Klick, siehe Checkpoint 5f). Checkpoint 5g (mehr Themes, mitgelieferte
Fonts inkl. Nerd Font) ist KOMPLETT (siehe unten) — auf ausdrücklichen
Owner-Wunsch vorgezogen, noch vor Behebung des verbleibenden Fokus-Falls.
Checkpoint 5f2 (Fix für genau diesen Flip-Fokus-Fall, siehe unten) ist
ebenfalls KOMPLETT, Chromium-verifiziert. Farbtiefe (Owner-Frage): volles
24-Bit-True-Color End-to-End (Rust-Parser → Screen-Modell → Canvas
`rgb()`), plus die 16-Farben-ANSI-Palette und den 256er-Cube/
Graustufen-Ramp — dieselbe Farbtiefe wie Ghostty; die vom Owner
beobachtete fehlende Abstufung lag an den Nerd-Font-Glyphen (5g), nicht an
der Farbtiefe. Der Owner hat außerdem die Frage aufgeworfen, ob die App
auf ein Chromium-basiertes Webview statt WKWebView umsteigen sollte, um
diese ganze Klasse von Bugs zu vermeiden — zurückgestellt, noch nicht
beantwortet. Checkpoint 5h (echtes Font-Gewicht-Setting + acht weitere
Mono-Fonts, zehn insgesamt) und Checkpoint 5i (Pfeiltasten/Home/End/
PageUp/PageDown/Delete wurden nie an die Shell weitergegeben — fixt u. a.
die vom Owner gemeldete fehlende Shell-Autosuggestion-Übernahme per →)
sind beide KOMPLETT. Checkpoint 5j (Font-Wechsel löste kein Neu-Vermessen
aus + Web-Font-Lade-Race, gefunden beim Erstellen von Demo-Screenshots für
den Owner, siehe unten) ist ebenfalls KOMPLETT. Checkpoint 5k
(Hintergrundfarbe folgte dem App- statt dem Terminal-Theme + kaputte
Blockzeichen, per Ghostty-Screenshot-Vergleich vom Owner gefunden, siehe
unten) ist ebenfalls KOMPLETT. Checkpoint 5l (COLORTERM=truecolor nie
gesetzt) und Checkpoint 5m (echter gepatchter Nerd Font statt
Icon-Fallback-Kette, direkte Ursache der abweichenden Kommandozeilen-
Symbole/-Farben) sind ebenfalls beide KOMPLETT. Checkpoint 5n
(Hintergrundfarben ignorierten das Theme komplett — echter Engine-Bug,
vom Owner selbst per `printf`-Repro am echten Mac bewiesen, siehe unten)
ist ebenfalls KOMPLETT — vom Owner selbst am echten Mac bestätigt
(korrektes Pastell-Blau, saubere abgerundete Pills). Checkpoint 5o
(Settings-Seite Redesign, siehe unten) ist ebenfalls KOMPLETT — vom Owner
selbst am echten Mac bestätigt und als "kann so bleiben" akzeptiert.
Checkpoint 5p (kaputte Box-Drawing-Linien — derselbe Bug wie 5k, nur für
Linienzeichen statt Füllzeichen, vom Owner per Screenshot der eigenen
Claude-Code-Darstellung im Terminal gemeldet, siehe unten) ist ebenfalls
KOMPLETT. Ein zweites, im selben Screenshot sichtbares Problem
(ungewöhnlich viel/durchgehende Unterstreichung) ist NICHT gefixt, aber
per Owner-Screenshot-Vergleich mit Ghostty jetzt als echter OSC-8-
Hyperlink-Unterschied bestätigt — Implementierung folgt als eigener
Checkpoint 5r. Checkpoint 5q (Shift+Tab — Claude Codes eigener Modus-
Wechsel-Shortcut — tat nichts, Owner-Feedback) ist ebenfalls KOMPLETT.
Checkpoint 5r (OSC-8-Hyperlinks — permanentes Unterstreichen
unterdrückt, Fortsetzung von Checkpoint 5p Problem 2, per
Ghostty-Vergleichs-Screenshots vom Owner bestätigt) ist ebenfalls
KOMPLETT. Owners erster echter Live-Test danach zeigte: der Screenshot
sah nach CP5r unverändert aus (Text weiterhin durchgehend
unterstrichen) — Ursache war aber gar nicht OSC-8, sondern ein
unabhängiger, älterer CSI-Parsing-Bug, jetzt als eigener Checkpoint 5s
gefixt (siehe unten) KOMPLETT. Owner hat direkt im Anschluss den ersten
echten Live-Test der ganzen Kette gemacht: Underline-Problem bestätigt
behoben; `vim`/`bpytop`/`neovim` laufen, Rendering laut Owner "nicht
wirklich smooth" (noch nicht weiter eingegrenzt — evtl. das offene
Performance-Tuning bei hohem Output, siehe unten); Maus-Auswahl,
`Cmd+C`/`Cmd+V` und mehrere gleichzeitige Kacheln funktionieren alle;
`Cmd+C` verursachte aber jedes Mal einen Systemton — gefixt als
Checkpoint 5t (siehe unten) KOMPLETT. Nächster Schritt: das
"nicht smooth"-Rendering bei vim/bpytop genauer eingrenzen (ruckelt es,
flackert es, hängt die Eingabe nach?), außerdem weiterhin ausstehend:
Scrollback-Gefühl explizit testen, danach Live-Test von
5f+5f2+5g+5h+5i+5j+5k+5l+5m+5n+5o+5p+5q+5r+5s+5t insgesamt als
abgeschlossen bestätigen (bei 5l zusätzlich `cargo test`, das in jener
Session wegen eines Umgebungs-Linker-Problems nicht laufen konnte; bei 5s
lief `cargo test` erstmals wieder durch und deckte dabei nebenbei auch
einen Bug in einem 5r-eigenen Test auf, siehe 5s) — diese ganze Kette
entstand aus genau solchen Live-Tests (oder, bei 5j/5k/5m/5p/5r, deren
Chromium-Ersatz), nicht aus automatisierter Verifikation allein.

## Checkpoint 5d — Bugfixes aus dem ersten echten Live-Test + globale
Settings-Datei (Owner-Feedback, 2026-09-14)

Der Owner hat zum ersten Mal die Checkpoint-5/5b-Settings-Seite am echten
Mac benutzt und mehrere Unstimmigkeiten gemeldet. Vier davon waren echte
Bugs, einer eine Architekturentscheidung (Settings-Persistenz), der Rest
(mehr Themes, mitgelieferte Fonts, Prompt-Design, oh-my-zsh) ist als
Checkpoint 5e vorgemerkt (siehe unten).

### Bugfixes

- **Transparenz-Regler ohne sichtbare Wirkung**: `TerminalScreen.draw`s
  `backgroundOpacity` hat schon immer korrekt funktioniert — das Problem war
  `terminal.svelte`s eigener `.terminal`-Wrapper-`<div>`, der eine fest
  undurchsichtige `background: var(--ax-surface-1)` hatte, exakt dieselbe
  Farbe, die der Canvas für unbefärbte Zellen zeichnet. Ein transparentes
  Loch im Canvas legte also nur einen gleichfarbigen Div darunter frei, nie
  das, was wirklich hinter der Kachel liegt. Fix: `backgroundOpacity` als
  echtes `$derived` extrahiert (vorher inline in `tick()` berechnet, nicht
  wiederverwendbar), zusätzlich über eine CSS Custom Property
  (`--terminal-bg-opacity`) + `color-mix()` auch auf den Wrapper-Div selbst
  angewandt. Live mit `agent-browser` gegen den `vite --port 1420`-Dev-Mock
  verifiziert (direkter DOM-Event-Dispatch nötig, da `fill` auf einem
  Range-Input im CDP-Setup keinen echten `change` auslöst — reines
  Test-Tooling-Artefakt, kein App-Bug).
- **Font-Size-Feld zeigte verwirrenden Text statt einer Zahl** (Owner-Screenshot):
  Placeholder war der String `"theme default"` in einem `type="number"`-Feld
  — liest sich wie ein Fehler, nicht wie ein Hinweis. Fix: einmalig beim
  Mount der echte aufgelöste `--ax-font-size-sm`-Wert gelesen
  (`getComputedStyle`, dieselbe Technik wie `graph/model.ts`s
  `readPalette()`) und als Zahl-mit-Einheit (z. B. `"12px"`) gezeigt.
- **Startverzeichnis-Feld ließ keine Eingabe/kein Löschen zu**: Ursache nicht
  abschließend bestätigt (reproduzierte nicht eindeutig gegen den
  Chromium-Dev-Mock, nur gegen die echte WKWebView beschrieben — dieses
  Projekt hat eine Vorgeschichte genau dieser Klasse von
  WebKit-only-Bugs, siehe `canvas/Tile.svelte`s `.tile-body`-Kommentar).
  Robuster Fix unabhängig von der genauen Ursache: jedes Freitext-Feld
  (Shell, Startverzeichnis, Umgebungsvariablen, Schriftart — nicht die
  beiden Zahlenfelder, die nicht betroffen waren) bekommt jetzt eine eigene
  lokale `$state`-Variable (einmalig beim Laden geseedet, danach nie wieder
  aus dem Store zurückgesynct) statt `value={$store.X}` direkt zu binden —
  ein Store-Write an anderer Stelle in derselben Komponente kann eine
  laufende Eingabe so nicht mehr zurücksetzen. Mit `agent-browser` verifiziert:
  Tippen, einzelnes Backspace, komplettes Leeren über Shift+Home+Backspace
  funktionieren alle.
- **Keine Persistenz beim Schließen** — siehe "Globale Settings-Datei" unten,
  direkt dieselbe Ursache wie der Architektur-Wechsel.

### Globale Settings-Datei statt Pro-Instanz-Config

Bisher lebten alle Terminal-Settings in `ctx.config` — dem Config-Blob der
platzierten Kachel selbst, Teil von `dashboard.json`s
`canvas.instances[].config` (Checkpoints 5/5b). Schließen/Entfernen einer
Kachel löschte die `CanvasInstance` und damit jede daran hängende
Einstellung — genau der gemeldete Bug. Owner-Wunsch zusätzlich: die
Einstellungen sollen "in einer Datei" liegen, weil ein eigenständig
nutzbares Terminal (außerhalb dieses Dashboards) später geplant ist.

**Entscheidung**: Terminal-Settings werden global — ein gemeinsamer
Einstellungssatz für jede platzierte Terminal-Kachel, nicht mehr pro
Kachel individuell. Das kostet die (bisher ungenutzte) Möglichkeit,
zwei Terminals mit unterschiedlichen Themes nebeneinander laufen zu lassen;
sollte das je gewünscht sein, ist Profile/Overrides ein eigenständiges
späteres Feature, kein rückwirkender Bruch dieser Entscheidung.

- **`crates/axiomata-core/src/json_state.rs`** (neu): die
  Lade-/Speicher-/Wiederherstellungs-Logik, die bisher exklusiv
  `dashboard.rs` gehörte (atomarer Write mit 0600, `O_EXCL`-Temp-Datei +
  Rename, kaputte Datei wird nach `.bak` verschoben statt die App zu
  brechen, Symlink-Ablehnung, Größenlimit), aus `dashboard.rs` extrahiert
  und parametrisiert (`load(path, default_json)`/`save(path, json)`) — jetzt
  von `dashboard.rs` UND dem neuen `terminal_settings.rs` genutzt, statt
  zweimal dieselbe ~150 Zeilen lange Logik zu pflegen. `dashboard.rs`s
  öffentliche API (`LoadedState`, `load_state`, `save_state`) bleibt
  unverändert nach außen (`LoadedState` ist jetzt ein Type-Alias).
  Wiederverwendet `AxiomataError::InvalidDashboardState` (schon vorher
  generisch genutzt, auch für `theme.css`-Validierung) statt einer
  Umbenennung, um unnötige Churn zu vermeiden.
- **`crates/axiomata-core/src/terminal_settings.rs`** (neu): dünner Wrapper
  um `json_state` mit dem eigenen Pfad (`~/.axiomata/terminal-settings.json`,
  neu in `paths.rs`) und Default (`{"version":1}`, keine
  vorausgesetzten Felder — jedes Feld ist eine optionale Owner-Präferenz).
- **`apps/dashboard/src-tauri/src/commands.rs`**: zwei neue Commands
  `get_terminal_settings`/`save_terminal_settings`, exakt dasselbe
  Übersetzungs-Muster wie `get_dashboard_state`/`save_dashboard_state` —
  registriert in `lib.rs`.
- **`apps/dashboard/src/modules/terminalSettings.ts`** (neu): der
  Frontend-Store — `terminalSettings: Writable<Record<string, unknown>>`,
  `ensureTerminalSettingsLoaded()` (einmaliger, idempotenter Lazy-Load beim
  ersten Bedarf, nicht beim App-Start), automatisches debounced Speichern
  bei jeder Änderung — spiegelt `core/persist.ts`s eigenes
  Load-once/Debounced-save-Muster, nur auf diese eine Datei beschränkt statt
  auf das ganze Dashboard.
- **`terminal.svelte`/`terminal-settings.svelte`**: jedes `$config.X` wurde
  zu `$terminalSettings.X`, `ctx.config` wird vom Terminal-Modul nicht mehr
  gelesen/geschrieben. `spawn()` `await`et `ensureTerminalSettingsLoaded()`
  ganz am Anfang — sonst würde der allererste Spawn nach App-Start mit
  leeren (noch nicht geladenen) Settings starten, nicht mit den wirklich
  gespeicherten. Die Settings-Seite selbst zeigt "Loading…", bis der Store
  geladen ist (seedet die Freitext-Felder erst dann, siehe Bugfix oben).
- **Kein neuer Cargo-Dependency.**

### Verifiziert

`cargo build/clippy/fmt/test --workspace`, `npm run check`, `npx vitest
run`, plus Live-Verifikation gegen den `vite --port 1420`-Dev-Mock mit
`agent-browser`: Tippen/Löschen im Startverzeichnis-Feld, Opazitäts-CSS
tatsächlich transparent, und — der eigentliche Kern des Bugfixes — Theme
+ Shell überleben ein komplettes Entfernen und Neu-Platzieren der
Terminal-Kachel.

## Checkpoint 5e — Zweite Live-Test-Runde: Ghosting/Clear, Startgröße,
Autofokus (Owner-Feedback, 2026-09-14)

Direkt im Anschluss an 5d, mit zwei Screenshots (unser Terminal vs.
Ghostty) zur Untermauerung — bestätigte unter anderem, dass der Prompt-
Unterschied tatsächlich an fehlenden Nerd-Font-Glyphen liegt (siehe
Checkpoint 5g unten), nicht an Farbtiefe.

- **Doppelt/verschmiert gezeichneter Text nach Schriftgrößen-Änderung, `clear`
  räumt nicht wirklich auf**: `TerminalScreen.draw` hat den Canvas nie
  explizit geleert — verließ sich komplett darauf, dass jede Zelle bei
  jedem Frame vollständig neu übermalt wird. Das hält nur, wenn ein Frame
  exakt dieselben Pixel abdeckt wie der vorherige; das gilt nicht mehr,
  sobald `canvasEl.width/height` (löscht den Backing-Store implizit,
  HTML5-Canvas-Spezifikation) und die tatsächlich vom Rust-Modell
  gelieferte Zeilen/Spalten-Anzahl (ein separater, debounced IPC-Roundtrip)
  zeitlich auseinanderlaufen — ein Frame kann mit neuen Zell-Metriken über
  alten Grid-Dimensionen landen oder umgekehrt, und hinterlässt dann
  Alt-Pixel außerhalb dessen, was der aktuelle Frame wirklich malt. Exakte
  Ursache nicht abschließend verifiziert (könnte auch mit HMR während
  aktiver Entwicklung zusammenhängen), robuster Fix unabhängig davon: ein
  echtes `ctx.clearRect(...)` (transform-unabhängig via
  `save`/`setTransform(1,0,0,1,0,0)`/`restore`) an den Anfang von `draw()`.
- **Startgröße soll sich an der Schriftgröße orientieren, immer von 120×60
  Zeichen ausgehend** statt der bisherigen festen `640×400`px: neues
  optionales `ModuleDefinition.computeDefaultSize?: () => {w,h}`
  (`core/types.ts`), von `core/lifecycle.ts`s `createInstance` bevorzugt,
  wenn vorhanden — Terminal registriert dafür `computeTerminalDefaultSize`
  (`modules/index.ts`): misst einen echten Zeichen-Zelle über einen
  Wegwerf-Canvas + `TerminalScreen.measureChar`, gegen die aktuell
  konfigurierte (oder Theme-Default-)Schrift, multipliziert mit 120×60 plus
  einer groben Kachel-Chrome-Schätzung. Mit `agent-browser` gegen echtes
  Chromium (nicht jsdom, da echte Font-Metriken gebraucht werden)
  verifiziert: 120 Spalten × gemessene Zeichenbreite ergab exakt die
  erwartete Pixelbreite; die zunächst kleiner wirkende Höhe war reine
  Viewport-Klemmung des Test-Fensters, kein Bug (bei größerem Fenster
  passte auch die Höhe). `ModulePicker`s Größen-Label zeigt weiterhin die
  statische `defaultSize` (rein informativ, kein Anspruch auf exakte
  Übereinstimmung).
- **Autofokus statt Pflicht-Klick**: `terminal.svelte`s `spawn()` ruft nach
  erfolgreichem `terminal_spawn` jetzt `inputEl?.focus()` — eine frisch
  erzeugte Terminal-Kachel ist sofort tippbereit, kein Klick mehr nötig.
  Das bestehende `onclick`-Fokussieren auf der ganzen Kachel bleibt
  unverändert für jeden späteren Klick (Kachel-Wechsel etc.).

**Verifikation**: `npm run check`, `npx vitest run` (inkl. neuer Tests für
`computeDefaultSize`-Präferenz in `lifecycle.test.ts`), Live-Verifikation
der Startgröße mit `agent-browser` gegen echtes Chromium. Kein Rust
betroffen.

## Checkpoint 5f — Autofokus wirklich robust machen (Owner-Feedback, 2026-09-14)

Checkpoint 5e's Autofokus-Fix (ein einzelnes `inputEl?.focus()` nach
`spawn()`s `terminal_spawn`-IPC-Aufruf) hat den gemeldeten Bug am echten
Mac NICHT behoben — der Owner meldete exakt dasselbe Symptom
("ich muss immer zuerst in das Fenster klicken") erneut, nachdem
Vergrößern/Verkleinern und Transparenz als erledigt bestätigt wurden.

- **Vermutete Ursache**: dieselbe Klasse von WebKit/WKWebView-Timing-Bug,
  die dieses Projekt schon öfter getroffen hat (siehe `canvas/Tile.svelte`s
  `.tile-body`-Kommentar) — ein `.focus()`-Aufruf, der erst nach zwei
  `await`s (`ensureTerminalSettingsLoaded`, dann der `terminal_spawn`-Round-
  Trip) kommt, bewegt in der echten WKWebView den tatsächlichen
  Tastatur-Fokus nicht zuverlässig, auch wenn `document.activeElement` im
  DOM selbst korrekt aussieht.
- **Fix**: neue Hilfsfunktion `focusInputSoon()` in `terminal.svelte` — statt
  eines einzelnen Aufrufs wird sofort UND auf den nächsten zwei
  `requestAnimationFrame`-Frames erneut fokussiert (`.focus()` auf einem
  bereits fokussierten Element ist ein No-Op, also kostet die Redundanz auf
  einer Engine, bei der der erste Aufruf schon funktioniert, nichts).
  Aufgerufen in `onMount` (so früh wie möglich, noch vor `spawn()`) und
  erneut in `spawn()`s Erfolgsfall (falls während des Roundtrips etwas den
  Fokus doch weggenommen hat).
- **Architektur-Review-Nachbesserung** (vor dem Commit, wie beim Rest dieser
  Checkpoint-Kette): die erste Fassung hätte bei jedem Mount ungebremst
  fokussiert — `Canvas.svelte` montiert aber alle platzierten Kacheln
  gleichzeitig, nicht eine nach der anderen hinter einem Tab. Ein
  wiederhergestelltes Layout mit mehreren Terminal-Kacheln (oder generell
  irgendein anderes fokussiertes Feld, z. B. die Suche oder ein
  Einstellungsfeld) hätte den Fokus ungefragt an die zuletzt gemountete
  Terminal-Kachel verloren. Fix: jeder automatische Fokus-Versuch (Mount +
  `spawn()`-Fallback) prüft jetzt zuerst, ob `document.activeElement`
  bereits etwas Sinnvolles ist (`!== document.body`) und bricht sonst ab;
  zusätzlich werden die ausstehenden `requestAnimationFrame`-IDs jetzt
  festgehalten und sowohl vor jedem neuen `focusInputSoon()`-Aufruf als
  auch in `onDestroy` abgebrochen, damit keine verspätete Fokussierung
  einer längst verlassenen Kachel nachträglich zuschlägt. Der bewusste
  Klick auf eine Kachel (`.terminal`s `onclick`) bleibt bewusst ein
  einzelner, ungegateter `inputEl?.focus()`-Aufruf statt über
  `focusInputSoon()` zu laufen — das war vor dieser Checkpoint nie kaputt,
  und ein Klick ist immer expliziter User-Intent, den man nicht abbrechen
  sollte.
- **Verifiziert** (`agent-browser` gegen echtes Chromium, `vite --port
  1420`): (1) frische Kachel ohne vorherigen Fokus → automatisch
  fokussiert; (2) Assistant-Input fokussiert, dann eine zweite
  Terminal-Kachel per synthetischem Klick erzeugt → Fokus bleibt im
  Assistant-Input, auch nach Ablauf des Retry-Fensters; (3) Klick auf eine
  Kachel → deren `.typer`-Input wird fokussiert. `npm run check` und `npx
  vitest run` (355 Tests) bleiben grün, kein Rust betroffen.
- **Wichtige Einschränkung**: Chromium kann das eigentliche
  WKWebView-Zeitproblem nicht reproduzieren — diese Verifikation zeigt nur,
  dass die DOM-/JS-Logik korrekt ist und die neuen Gating-/Cleanup-Regeln
  keine Regression einführen. Ob das echte Problem damit behoben ist, kann
  nur der Live-Test am Mac des Owners zeigen.

**Committet als `e415426`.**

### Owner-Rückmeldung nach dem Live-Test am Mac (2026-09-14) — teilweise behoben

- **Bestätigt behoben**: Öffnen einer neuen Terminal-Kachel ist jetzt
  sofort tippbereit, kein Klick mehr nötig.
- **Neuer, enger gefasster Fall, inzwischen behoben — siehe Checkpoint 5f2**
  (nach Checkpoint 5g weiter unten): Flippt man zu den Settings (Rückseite
  der Kachel) und wieder zurück zur Terminal-Vorderseite, war erneut ein
  Klick nötig, bevor Tippen wieder funktionierte.
- **Owner-Frage, ebenfalls zurückgestellt, noch nicht beantwortet**: sollte
  die App statt WKWebView ein Chromium-basiertes Webview voraussetzen/
  bündeln, um diese ganze Klasse von Timing-Bugs zu vermeiden? Tauri bietet
  auf macOS aktuell kein Chromium-Backend — das wäre praktisch ein Wechsel
  auf Electron (eigenes gebündeltes Chromium, deutlich größerer
  Programmordner/Speicherverbrauch), keine einfache Konfigurationsoption.
  Braucht eine eigene Abwägung, bevor daran gearbeitet wird.

## Checkpoint 5g — Mehr Themes + mitgelieferte Fonts inkl. Nerd Font (Owner-Feedback, 2026-09-14) — KOMPLETT

Owner-Wunsch: erst diesen Checkpoint fertig machen, bevor am offenen
Flip-Fokus-Fall (Checkpoint 5f, siehe oben) weitergearbeitet wird.

- **Mehr Themes**: `terminalThemes.ts`'s `THEMES` um `catppuccin-mocha` und
  `tokyo-night` erweitert (mechanisch, exakt wie die vier bestehenden
  Community-Paletten) — je die offiziell veröffentlichte 16-Farben-ANSI-
  Zuordnung des jeweiligen Projekts, nicht selbst zusammengestellt.
  `terminal-settings.svelte`s `THEME_LABELS` und `terminalThemes.test.ts`
  (inkl. eines neuen generischen Hex-Format-Tests über alle Paletten)
  entsprechend ergänzt.
- **Mitgelieferte Mono-Fonts**: `@fontsource/jetbrains-mono` und
  `@fontsource/ibm-plex-mono`, je Gewicht 100/400/700 (Thin/Regular/Bold —
  beide Familien haben, anders als z. B. Fira Code, ein echtes 100er-Gewicht),
  in `main.ts` importiert, exakt nach dem bestehenden Muster des schon
  gebündelten Material-Symbols-Icon-Fonts. Neuer "Bundled font"-`<select>`
  in `terminal-settings.svelte` (JetBrains Mono / IBM Plex Mono / Custom…)
  als Ein-Klick-Shortcut in das bestehende Freitextfeld — keine eigene
  Einstellung, keine zweite Quelle der Wahrheit. Ehrlichkeitshinweis: nur
  Regular und Bold werden aktuell tatsächlich gezeichnet
  (`TerminalScreen.draw` kennt nur "fett" oder "normal" pro Zelle, keine
  Gewichts-Auswahl in der UI) — das 100er/Thin-Gewicht ist mitgebündelt,
  weil der Owner es wörtlich so gewünscht hat, wird aber aktuell nirgends
  gerendert. Eine echte Gewichts-Einstellung wäre ein eigenes, noch nicht
  angefragtes Feature.
- **Nerd-Font-Glyphen**: neue Abhängigkeit `@azurity/pure-nerd-font` (MIT,
  exakt gepinnt statt Caret-Range — Architektur-Review, Checkpoint 5g:
  Einzelperson-Projekt, keine offizielle nerd-fonts.com-Distribution,
  seit über einem Jahr nicht aktualisiert, ~1 MB reines Icon-Glyphen-Font
  ohne normale Zeichen, gepinnt auf Nerd Fonts v2.2.0-RC laut eigener CSS).
  `terminal.svelte`s `currentFont()` hängt `"PureNerdFont"` als letzten
  Fallback an die jeweils konfigurierte Font-Familie an — Canvas `ctx.font`
  löst eine Fallback-Liste pro Glyph auf (wie CSS), normaler Text kommt
  weiter aus der Primärfamilie, nur wirklich fehlende Glyphen (Powerline-
  Trennsymbole, Devicons in Starship/p10k) fallen durch. **Wichtige
  Einschränkung**: da das gebündelte Glyphen-Set auf einer älteren
  Nerd-Fonts-Generation basiert, kann ein einzelnes, sehr neues Icon aus
  einem aktuellen p10k-/Starship-Preset trotzdem als leere Box erscheinen —
  ob das beim Owner konkret der Fall ist, kann nur der Live-Test zeigen.
- **Prompt-Design** (Git-Branch/Python-venv in der Kommandozeile) bleibt
  laut Owner-Klärung Sache der Shell-Konfiguration (Starship/Powerlevel10k),
  nicht etwas, das der Terminal-Emulator selbst hinzufügen sollte — kein
  Code-Änderungsbedarf.
- **oh-my-zsh "sieht anders aus als in Ghostty"**: Owner konnte nicht genauer
  spezifizieren, was; wahrscheinlichster Kandidat waren fehlende
  Nerd-Font-Glyphen — durch den Font-Fallback oben adressiert, aber nicht
  eigenständig verifizierbar ohne den Owner's echtes oh-my-zsh-Setup.

**Verifiziert**: `npm run check` und `npx vitest run` (357 Tests, 2 neu)
grün, kein Rust betroffen. Live mit `agent-browser` gegen echtes Chromium
(`vite --port 1420`): alle drei neuen/gebündelten Fonts (JetBrains Mono,
IBM Plex Mono, PureNerdFont) tatsächlich in `document.fonts` registriert;
Theme-`<select>` zeigt beide neuen Paletten; "Bundled font"-`<select>`
schreibt korrekt in das Font-family-Freitextfeld durch. Die tatsächliche
Canvas-Zeichnung mit den neuen Fonts/Glyphen konnte NICHT gegen den
Dev-Mock verifiziert werden (`terminal_spawn` existiert dort nicht, siehe
`devmock.ts` — dieselbe Einschränkung wie bei jeder Terminal-Checkpoint
zuvor), nur die Konfigurationskette bis zum `ctx.font`-String-Aufbau.
Architektur-Review (Checkpoint 5g) fand ein HIGH-Finding (Nerd-Fonts-
Versions-Staleness, oben dokumentiert) und ein MEDIUM (Caret- statt
Exakt-Pin) — beide vor dem Commit behoben. Ein docs-writer-Pass fand und
korrigierte zwei faktische Ungenauigkeiten in eigenen Kommentaren (Anzahl
der Paletten "vier" → "sechs"; die "Thin/Bold werden beide gerendert"-
Behauptung war falsch, siehe Ehrlichkeitshinweis oben).

## Checkpoint 5f2 — Fokus beim Zurückflippen von den Settings (Owner-Feedback, 2026-09-14) — KOMPLETT

Der in Checkpoint 5f offen gebliebene, enger gefasste Fall: `Tile.svelte`
mountet Vorder- (Terminal) und Rückseite (Settings) einer Kachel
gleichzeitig und wechselt zwischen ihnen nur per CSS (`rotateY`-Transform
auf `.tile-inner`, gesteuert über dessen `flipped`-Klasse) — es wird nie
neu gemountet. `terminal.svelte`s `onMount` feuert deshalb nur genau
einmal, beim allerersten Platzieren der Kachel; ein späteres Zurückflippen
von den Settings hatte keinerlei Mechanismus, der den Fokus erneut auf das
`.typer`-Input legt.

- **Fix**: `focusInputSoon()` bekommt einen optionalen `{ gated?: boolean }`-
  Parameter (Default `true`, bestehendes Verhalten an den beiden
  bisherigen Aufrufstellen unverändert). Neue Funktion `watchFlipBack()`
  (aus `onMount` aufgerufen) sucht per `root?.closest(".tile-inner")` die
  Kachel-Hülle und beobachtet deren `class`-Attribut mit einem
  `MutationObserver` — exakt dieselbe Technik wie der bestehende
  `themeObserver` für Theme-Wechsel. Beim Übergang von `flipped` zu nicht
  mehr `flipped` (= Rückflug zur Terminal-Vorderseite) wird
  `focusInputSoon({ gated: false })` aufgerufen — ungegated, weil
  `document.activeElement` direkt nach einem Klick auf "Flip back" der
  Button selbst ist, nicht `body`; das bestehende Gating (gegen
  Fokus-Diebstahl beim gleichzeitigen Mounten mehrerer Kacheln, Checkpoint
  5f) würde hier genau den gewünschten Rückfokus verhindern. Aufräumen via
  `flipObserver?.disconnect()` in `onDestroy`, exakt wie die anderen
  Observer.
- **Architektur-Review-Nachbesserung** (MEDIUM, vor dem Commit behoben):
  die Kopplung an `Tile.svelte`s konkrete DOM-Struktur (`.tile-inner`,
  `flipped`-Klasse) ist unsichtbar aus `Tile.svelte`s eigener Sicht — eine
  künftige Umbenennung dort würde diesen Mechanismus lautlos abschalten,
  ohne dass `terminal.svelte` überhaupt angefasst wird. Fix: ein
  Cross-Reference-Kommentar direkt bei `.tile-inner`/`class:flipped` in
  `Tile.svelte` selbst, plus ein dev-only `console.warn` in
  `watchFlipBack()`s "nicht gefunden"-Zweig, damit eine echte Regression
  sichtbar wird statt nur "Fokus geht plötzlich nicht mehr" zu sein.
- **Verifiziert** (`agent-browser` gegen echtes Chromium, `vite --port
  1420`): Kachel erzeugt → automatisch fokussiert; zu Settings geflippt →
  Fokus bleibt (erwartet) auf dem Flip-Button; zurückgeflippt → `.typer`
  sofort wieder fokussiert, auch nach Ablauf des Retry-Fensters; zweiter
  Flip-Zyklus (Settings → zurück) funktioniert erneut. `npm run check` und
  `npx vitest run` (357 Tests) bleiben grün, kein Rust betroffen.
- **Wichtige Einschränkung**: wie bei Checkpoint 5f kann Chromium das
  eigentliche WKWebView-Timing-Problem nicht reproduzieren — auch dieser
  Fix ist nur DOM-/JS-seitig verifiziert, nicht am echten Mac.

## Checkpoint 5h — Echtes Font-Gewicht-Setting + acht weitere Mono-Fonts (Owner-Feedback, 2026-09-15) — KOMPLETT

Owner-Wunsch: "Ich würde das Font-Gewicht-Setting noch angehen wollen und
auch gerne noch ein paar Monoschriften auch wenn sie kein Thin etc.
anbieten. Denke so 10 Fonts wären toll."

- **Acht weitere Fonts**: Fira Code, Source Code Pro, Roboto Mono, Space
  Mono, Ubuntu Mono, Inconsolata, Victor Mono, Anonymous Pro — zusammen mit
  den beiden aus Checkpoint 5g jetzt zehn. Bewusste Mischung: einige mit
  vollem Gewichtsspektrum (z. B. Roboto Mono, Victor Mono — echtes Thin),
  andere ausdrücklich mit nur Regular+Bold (Space Mono, Ubuntu Mono,
  Anonymous Pro) — genau der vom Owner explizit gewünschte Fall "auch wenn
  sie kein Thin etc. anbieten". Jeder Font bündelt sein leichtestes
  verfügbares Gewicht (nicht immer 100), 400 und 700 — alle zehn haben ein
  echtes 700, also ist "Bold" (sowohl die neue Gewichts-Auswahl als auch
  `TerminalScreen`s separates SGR-Bold-Rendering) über die ganze Liste
  konsistent.
- **Neues `terminalFonts.ts`**: die zehn Familien + ihre gebündelten
  Gewichte als eine einzige Quelle der Wahrheit (Architektur-Review,
  spiegelt exakt `terminalThemes.ts`s `THEMES`-Muster) —
  `terminal-settings.svelte`s "Bundled font"-Auswahl leitet ihre
  Namensliste jetzt daraus ab, statt eine eigene Kopie zu pflegen.
  `main.ts`s statische CSS-Imports bleiben zwangsläufig separat (Vite
  braucht literale Importpfade), verweisen aber jetzt per Kommentar auf
  diesen Katalog als deklarierte Quelle.
- **Echtes Font-Gewicht-Setting**: neue "Font weight"-Auswahl in
  `terminal-settings.svelte` mit der vollen CSS-Standardskala 100-900,
  unabhängig vom gerade gewählten Font (bewusst nicht auf dessen
  gebündelte Gewichte beschränkt — ein nicht gebündeltes Gewicht rendert
  trotzdem, über das normale Browser-Font-Matching auf das nächstliegende
  registrierte Gewicht, kein Sonderfall). `TerminalScreen.ts` bekam dafür
  eine neue reine Funktion `cellFont(font, bold, weight?)`: Bold-Zellen
  benutzen immer das literale `"bold"`-Schlüsselwort (ignoriert das
  konfigurierte Gewicht komplett — SGR-Bold bleibt ein eigener visueller
  Zustand), Nicht-Bold-Zellen bekommen das konfigurierte Gewicht als
  numerisches CSS-Token vorangestellt, wenn gesetzt. Eigens als kleine,
  pure Funktion herausgezogen und unit-getestet — `draw()` selbst braucht
  einen echten 2D-Canvas-Context, den jsdom nicht implementiert.
- **Architektur-Review-Nachbesserungen** (vor dem Commit behoben):
  - **HIGH**: `fontWeight` wurde ungeprüft aus dem untypisierten,
    Schema-losen `terminalSettings`-Store gelesen — eine von Hand
    editierte oder alte `terminal-settings.json` könnte einen ungültigen
    Wert enthalten (NaN, negativ, außerhalb des gültigen CSS-Bereichs),
    was `ctx.font` bei der Zuweisung nicht wirft, sondern nach Canvas-2D-
    Spec lautlos den *vorherigen* Font behält — ein deutlich schlimmerer
    Fehlerfall als ein Clamp. Fix: neues `fontWeight`-`$derived` in
    `terminal.svelte`, das genau wie das bestehende
    `backgroundOpacity`-`$derived` auf Lese-Seite klemmt (`[1, 1000]`,
    gerundet). Zusätzlich `cellFont`s eigener Truthy-Check (`weight ? ...`)
    auf `weight !== undefined` korrigiert, damit die Funktion auch isoliert
    aufgerufen korrekt bleibt, nicht nur durch die Klemmung des Callers.
  - **MEDIUM**: `measureChar`/`computeTerminalDefaultSize` messen die
    Zellbreite immer beim Standardgewicht der Schrift, während `draw()`
    Normal-Zellen jetzt tatsächlich im konfigurierten Gewicht zeichnet —
    funktioniert nur, weil echte Monospace-Fonts laut Definition dieselbe
    Laufweite über alle Schnitte behalten. War bisher nirgends
    dokumentiert; jetzt ein expliziter Kommentar bei `measureChar`, der
    diese Annahme benennt und den bekannten Grenzfall (ein
    selbst-getippter Systemfont, der die Annahme nicht erfüllt) offen als
    akzeptierte Lücke einordnet statt sie zu verschweigen.
  - **MEDIUM**: die Font-Namensliste existierte doppelt (Kommentar in
    `main.ts`, eigenes Array in `terminal-settings.svelte`) — behoben durch
    das neue `terminalFonts.ts` (siehe oben).
  - **LOW**: `currentFont()`s Kommentar war nach der Änderung veraltet
    (erwähnte nur noch Bold, nicht das neue Gewicht) — korrigiert; eine
    Template-Zeile über 120 Zeichen umgebrochen.
- **Verifiziert**: `npm run check` und `npx vitest run` (367 Tests, 10 neu)
  grün, kein Rust betroffen. Live mit `agent-browser` gegen echtes
  Chromium: alle zehn Fonts tatsächlich in `document.fonts` registriert,
  "Bundled font"- und "Font weight"-Auswahl beide vollständig sichtbar und
  funktionsfähig nach dem Refactor. Die tatsächliche Canvas-Zeichnung bei
  einem gewählten Gewicht ist wie bei CP5g nicht gegen den Dev-Mock
  verifizierbar (kein `terminal_spawn`) — Live-Test am Mac steht aus.

## Checkpoint 5i — Pfeiltasten/Navigationstasten wurden nie an die Shell weitergegeben (Owner-Feedback, 2026-09-15) — KOMPLETT

Owner-Beobachtung: "im Terminal [...] macht er ja schon bei einem Befehl
eine Vorhersage z.B. ich tippe ls dann steht im Terminal ls -altr das
-altr aber grau. In Ghostty brauche ich dann nur den Pfeil nach rechts zu
drücken und ich habe den ganzen Befehl." — die Shell-Autosuggestion
(zsh-autosuggestions o. ä.) rendert bei uns bereits korrekt (reine
Programmausgabe, nichts Terminal-Spezifisches), aber → zum Übernehmen tat
nichts.

- **Root Cause**: `terminalInput.ts`s `keyToBytes()` — die reine Funktion,
  die `KeyboardEvent.key` auf die an die PTY weiterzuleitenden Rohbytes
  abbildet — hatte für ArrowUp/Down/Left/Right (und Home/End/PageUp/
  PageDown/Delete) gar keinen Fall; sie fielen alle auf `default: return
  null` durch, `terminal.svelte`s `handleKeydown` verwarf sie also
  stillschweigend. Nicht auf Autosuggestion beschränkt: das bedeutete,
  Shell-History (↑/↓) und Cursor-Bewegung innerhalb der Zeile (←/→) haben
  in diesem Terminal die ganze Zeit über gar nicht funktioniert — nur
  bisher niemandem aufgefallen.
- **Fix**: alle neun Tasten bekommen jetzt die xterm-Standard-"Normal
  Cursor Key Mode"-Kodierung (CSI-Sequenzen, z. B. `\x1b[C` für
  ArrowRight) — exakt das, was eine Shell-eigene Zeileneditierung (zsh
  zle, GNU readline) immer erwartet. Architektur-Review bestätigte die
  Sequenzen gegen `infocmp xterm-256color`/xterms eigene `ctlseqs.txt` als
  korrekt (nicht die rxvt/VT220-Variante `\x1b[1~`/`\x1b[4~`, die falsch
  gewesen wäre, da die PTY immer mit `TERM=xterm-256color` gestartet
  wird).
- **Bekannte, bewusst offen gelassene Einschränkung**: die Rust-Engine
  trackt den DECCKM-Modus (`\x1b[?1h`/`\x1b[?1l`, "Application Cursor Key
  Mode", von Vollbild-Programmen wie vim gesetzt) noch gar nicht — Arrow-
  Tasten UND Home/End (laut xterms eigenem Terminfo dieselbe
  "Cursor-Key"-Gruppe) werden deshalb immer in der Normal-Modus-Kodierung
  gesendet, auch innerhalb eines Programms, das Application-Modus
  angefordert hat. PageUp/PageDown/Delete sind von dieser Einschränkung
  NICHT betroffen (immer dieselbe Kodierung, unabhängig vom Modus). In der
  Praxis funktioniert das laut Architektur-Review-Recherche für normale
  vim/htop/less-Nutzung (vim erkennt dokumentiert beide Kodierungsformen
  defensiv) — bewusste Entscheidung, den Shell-Prompt-Fall (der
  eigentliche, akute Bug) nicht auf eine vollständige DECCKM-Implementierung
  warten zu lassen. Sollte je ein konkretes Vollbild-Programm gefunden
  werden, bei dem Pfeiltasten/Home/End falsch reagieren, ist das der Anlass,
  DECCKM wirklich zu tracken.
- **Verifiziert**: reine Unit-Tests (`terminalInput.test.ts`) prüfen alle
  neun Tasten byte-genau, inkl. Korrektur eines jetzt falschen
  Alt-Tests (`keyToBytes("ArrowUp", false)` gab vorher `null` zurück) und
  eines neuen Ctrl-Modifier-Tests über die ganze Gruppe. `npm run check` +
  `npx vitest run` (367 Tests) grün. Kein Live-`agent-browser`-Test nötig
  oder möglich — reine Byte-Logik ohne DOM/Canvas-Bezug, vollständig durch
  Unit-Tests abgedeckt; ob es an der echten PTY/Shell tatsächlich greift,
  kann nur der Live-Test am Mac zeigen.

## Checkpoint 5j — Font-Wechsel löste kein Neu-Vermessen aus + Web-Font-Lade-Race (gefunden beim Screenshot-Erstellen, 2026-09-15) — KOMPLETT

Beim Erstellen von Demo-Screenshots für den Owner (Chromium-Dev-Mock mit
simuliertem Terminal-Output, da die echte WKWebView hier nicht bedienbar
ist) fielen zwei echte Bugs auf — kein Owner-Live-Test, aber real und
reproduzierbar:

- **Bug 1**: Das `$effect`, das die Zeichen-Zelle neu vermisst und bei
  Bedarf das Terminal-Grid resized, beobachtete nur `terminalSettings.fontSizePx`.
  Ein Font-*Familien*-Wechsel (Bundled-font-Auswahl oder Freitextfeld) oder,
  seit Checkpoint 5h, ein Gewichts-Wechsel lösten gar kein Neu-Vermessen
  aus — der Canvas zeichnete weiter mit den alten Zell-Maßen, während
  `ctx.font` längst auf den neuen Font umgestellt war. Fix: `$effect`
  beobachtet jetzt zusätzlich `fontFamily` und `fontWeight`.
- **Bug 2, auch nach Fix 1 noch reproduzierbar**: Ein Font, der in dieser
  Session noch nie benutzt wurde, ist beim ersten Vermessen oft noch nicht
  geladen — Browser laden `@font-face`-Dateien lazy bei tatsächlicher
  Erstnutzung, nicht beim App-Start, obwohl `main.ts` sie schon importiert.
  `measureChar`s synchroner `ctx.measureText("M")`-Aufruf lief also vor dem
  Laden ab, maß gegen einen Fallback-Font, und dieses falsche Zellmaß
  wurde ins Canvas-Backing-Store/Grid eingebrannt — während spätere Frames
  mit dem (inzwischen geladenen) echten Font zeichneten. Ergebnis: sichtbar
  überlappender/verschobener Text. Empirisch bestätigt über
  `document.fonts.check(font)`, das für einen in dieser Session noch nie
  genutzten Font `false` zurückgab. Fix: `measureAndSize` prüft das jetzt
  und lädt bei Bedarf explizit über `document.fonts.load(font)`, misst nach
  Abschluss neu und stößt bei Bedarf ein Resize an.
- **Architektur-Review-Nachbesserungen** (vor dem Commit behoben):
  - **HIGH**: der neue `document.fonts.load().then(...)`-Callback war als
    einziger asynchroner Vorgang in dieser Datei NICHT gegen ein
    zwischenzeitliches Entfernen der Kachel abgesichert (jeder andere
    hängende Callback — `focusRafIds`, `resizeDebounce`, alle Observer —
    ist es bereits). Fix: neues `destroyed`-Flag, in `onDestroy` gesetzt,
    im Callback geprüft.
  - **MEDIUM**: ohne Schutz hätte eine Font/Gewichts-Kombination, für die
    gar kein passendes `@font-face` gebündelt ist (einige Fonts haben nur
    Regular/Bold, siehe Checkpoint 5h), `document.fonts.check` theoretisch
    dauerhaft `false` liefern können — jeder weitere Aufruf von
    `measureAndSize` (jede Einstellungsänderung, jeder Resize) hätte dann
    erneut denselben unerfüllbaren `.load()`-Versuch gestartet. Fix: neues
    `attemptedFontLoads`-Set macht den Retry pro exaktem Font-String
    einmalig statt unbegrenzt wiederholbar.
- **Verifiziert**: `npm run check` + `npx vitest run` (367 Tests) grün,
  kein Rust betroffen. Live mit `agent-browser` gegen echtes Chromium: vor
  dem Fix reproduzierbar überlappender Text bei einem frischen
  Font-Wechsel (getestet mit Victor Mono und Fira Code), nach dem Fix
  sauber bei mehreren zuvor nie benutzten Fonts (Anonymous Pro, Victor
  Mono) inkl. gleichzeitigem Theme-Wechsel. Gezielte Isolations-Tests
  bestätigten: Theme-only- und Gewicht-only-Wechsel waren schon vorher
  sauber (kein Zellmaß-Einfluss) — nur Familie-ändernde Wechsel waren
  betroffen.

## Checkpoint 5k — Hintergrundfarbe folgte dem App-Theme statt dem Terminal-Theme + kaputte Blockzeichen (Owner-Feedback mit Ghostty-Vergleich, 2026-09-15) — KOMPLETT

Owner-Vergleich mit echten Ghostty-Screenshots deckte zwei Probleme auf:
"was ist mit der Hintergrundfarbe die passt zwar zum App Theme aber nicht
zu[m] Theme des Terminals?" und ein kaputt aussehendes `opencode`-Start-
Logo (Blockzeichen-ASCII-Art).

- **Bug 1 — Hintergrundfarbe**: `terminal.svelte` hat `defaultFg`/`defaultBg`
  (die Farbe einer unbeschriebenen Zelle) schon immer aus dem App-eigenen
  Chrome-Theme gelesen (`--ax-text`/`--ax-surface-1`), komplett unabhängig
  von `terminalSettings.theme` (dem 16-Farben-ANSI-Schema, z. B. "Catppuccin
  Mocha"). Ein Theme-Wechsel färbte also allen ANSI-indizierten Text um,
  aber der eigentliche Hintergrund blieb, was auch immer das App-Theme
  gerade war — sichtbarer Clash. Fix: neue `THEME_DEFAULT_COLORS`-Tabelle
  in `terminalThemes.ts` mit dem echten, offiziellen Hintergrund/Vordergrund
  jedes Themes außer `xterm` (das bewusst weiter dem App-Theme folgt, kein
  Versehen). `terminal.svelte`s neues `currentDefaultColors()` nutzt diese
  Tabelle für das aktive Theme, fällt sonst auf die bisherigen
  CSS-Farben zurück.
- **Bug 2 — kaputte Blockzeichen**: TUI-ASCII-Art mit Unicode-"Block
  Elements" (█▀▄ etc., U+2580-259F) zeigte ein "Schachbrett"-Muster statt
  durchgehender Flächen. Zwei Ursachen: (a) `measureChar`s Zellmaße waren
  Fließkomma-Werte, wodurch die meisten Zellpositionen auf Sub-Pixel-
  Grenzen landeten; (b) selbst bei Pixel-genauer Positionierung füllt das
  Glyph vieler Fonts für z. B. "█" (VOLLER BLOCK) sein eigenes
  Zeichen-Feld nicht randlos aus (Font-Design, kein Bug in der Schrift) —
  unsichtbar bei normalem Text, aber sofort sichtbar bei aneinandergereihten
  "durchgehenden" Blockzeichen. Fix: `measureChar` rundet jetzt auf ganze
  CSS-Pixel; neue `BLOCK_ELEMENT_RECTS`-Tabelle (jedes Blockzeichen als
  1-3 zellrelative Rechtecke, inkl. der vier Quadranten-Zeichen ▖▗▘▙▚▛▜▝▞▟
  als 1-3 Viertel-Zell-Rechtecke) + `SHADE_ALPHA` (die drei Schattierungs-
  zeichen ░▒▓, als alpha-geblendete Vollflächen angenähert) + neue
  `drawBlockElement()`-Funktion, die diese Zeichen prozedural als Rechtecke
  zeichnet statt über die Font-Glyphe — exakt wie echte Terminals (Kitty,
  Alacritty, Ghostty, iTerm2) das für genau diesen Zeichenbereich handhaben.
  Box-Zeichnungs-*Linien* (U+2500-257F, ┌┐└┘─│├┤┬┴┼ für TUI-Rahmen) sind
  bewusst NICHT Teil dieses Fixes — eigenständiges, noch nicht umgesetztes
  Folge-Thema (Liniensegmente statt Flächen, ein anderes Rendering-Problem).
- **Verifiziert**: `npm run check` + `npx vitest run` (374 Tests, 8 neu)
  grün, kein Rust betroffen. Live mit `agent-browser` gegen echtes Chromium,
  inkl. Pixel-genauer Verifikation via `getImageData`: Catppuccin-Mocha-
  Hintergrund exakt `rgb(30,30,46)` = `#1e1e2e`, Vordergrund exakt
  `rgb(205,214,244)` = `#cdd6f4` — beides die offiziellen Catppuccin-Werte.
  Blockzeichen-Testgrafik vorher sichtbar löchrig, nachher komplett
  durchgehend. Architektur-Review bestätigte `BLOCK_ELEMENT_RECTS`s
  Geometrie als korrekt gegen die echte Unicode-Definition (inkl. der
  komplexesten Drei-Quadranten-Fälle ▙▛▜▟) und `THEME_DEFAULT_COLORS`s
  sechs neue Werte als korrekt gegen die jeweils offiziellen Theme-Farben;
  nur LOW-Findings (Zeilenlänge, Kommentar-Genauigkeit, Test-Beschreibung,
  fehlender Cross-Reference-Kommentar) — alle vor dem Commit behoben.

## Checkpoint 5l — COLORTERM=truecolor nie gesetzt (gefunden bei der Ursachensuche zur Kommandozeile, 2026-09-15) — KOMPLETT

Owner-Nachfrage nach Checkpoint 5k: "Wie erklärst du dir aber die
Kommandozeile in der Shell dass sie sowohl was die Symbole angeht als
auch die Farben komplett von der Kommandozeile z.B in Ghostty abweicht?"
— mit Ghostty- und Kitty-Vergleichsscreenshots.

- **Fix**: `crates/axiomata-terminal/src/pty.rs`s `PtySession::spawn`
  setzt jetzt zusätzlich zu `TERM=xterm-256color` auch
  `COLORTERM=truecolor`. Es gibt keine formale Terminfo-Fähigkeit für
  24-Bit-Farbunterstützung — Capability-erkennende Tools
  (Powerlevel10k/Starship-Prompts, `chalk`-basierte Node-Tools,
  `git diff --color` u. a.) prüfen stattdessen diese De-facto-Standard-
  Variable, um zu entscheiden, ob sie echte RGB-Escape-Sequenzen senden
  oder auf eine blassere 256-Farben-Näherung zurückfallen. Diese Engine
  unterstützt echte 24-Bit-`38;2;r;g;b`-Sequenzen bereits seit Checkpoint 1
  — das wurde nur nie angekündigt. Echte Terminals mit True-Color
  (Ghostty, Kitty, iTerm2, Alacritty) setzen diese Variable alle selbst.
- **Zwei neue Rust-Tests**, die `TERM`+`COLORTERM` zusammen prüfen statt
  nur `COLORTERM` allein (Architektur-Review-Nachbesserung: `COLORTERM=
  truecolor` allein ist in praktisch jeder modernen Dev-Shell schon
  vererbt vorhanden — ein Test, der nur das prüft, könnte auch dann grün
  bleiben, wenn die eigentliche Fix-Zeile gelöscht würde. `TERM` ist als
  zweite Bedingung deutlich unwahrscheinlicher zufällig ererbt, da die
  meisten echten Terminals ihren eigenen, selbst-identifizierenden Wert
  setzen). Ein vollständig wasserdichter Test bräuchte `unsafe
  std::env::set_var` (in der 2024-Edition dieses Crates unsafe, mit
  echten Soundness-Fallstricken unter paralleler Testausführung) —
  bewusst nicht gemacht, stattdessen die Testabdeckung so weit gestärkt
  wie ohne dieses Risiko sinnvoll möglich.
- **Verifiziert**: `cargo check`/`cargo clippy -- -D warnings`/`cargo fmt
  --check` alle sauber. `cargo test` konnte in dieser Session NICHT
  laufen (ein Xcode-Lizenz-/Linker-Problem der Umgebung, unabhängig von
  dieser Änderung, blockiert jedes Linking hier) — die beiden neuen Tests
  sind also unausgeführt, nur durch sorgfältiges manuelles Review
  (Architektur-Review + eigene Nachbesserung) abgesichert. Live-Bestätigung
  am Mac (inkl. `cargo test`) steht aus.

## Checkpoint 5m — Echter gepatchter Nerd Font statt Icon-Fallback-Kette (gefunden bei der Ursachensuche zur Kommandozeile, 2026-09-15) — KOMPLETT

Direkter Vergleich (gezoomte Screenshots) von Ghostty, Kitty und unserem
Terminal zeigte: Ghostty UND Kitty (zwei unabhängige, echte Terminal-
Programme) rendern beide abgerundete Powerline-Pill-Segmente mit
korrekten Icons; unseres zeigte eckige Kanten und mindestens ein falsches
Icon-Glyph. Da Kitty (kein von uns gebautes Programm) genauso "richtig"
aussah wie Ghostty, war klar: das ist kein Rendering-Engine-Bug, sondern
eine Font-Fidelity-Lücke.

- **Root Cause**: Checkpoint 5g bündelte `PureNerdFont`, eine reine
  Icon-Schrift, als CSS-Fallback HINTER der eigentlich gewählten Schrift.
  Der Owner's eigene Ghostty-Config zeigt aber `font-family =
  "JetBrainsMono Nerd Font Mono"` — die ECHTE, offiziell von
  Nerd-Fonts gepatchte Version von JetBrains Mono, wo Buchstaben UND
  Icons vom selben Patch-Werkzeug in EINER Schriftdatei zusammengeführt
  wurden. Ein CSS-Fallback über zwei separat entworfene Schriften kann
  diese Pixel-genaue Konsistenz (Rundungen, Icon-Formen) grundsätzlich
  nicht garantieren, egal wie vollständig die Fallback-Schrift ist.
- **Fix**: die echten offiziellen `JetBrainsMonoNerdFontMono-{Thin,
  Regular,Bold}`-Dateien aus dem offiziellen `ryanoasis/nerd-fonts`-
  Release v3.5.1 (`JetBrainsMono.tar.xz`, "Mono"-Variante — erzwingt
  Icon-Glyphen auf exakt eine Zeichenzelle Breite, im Unterschied zur
  Standard-Variante) extrahiert, von TTF zu WOFF2 konvertiert
  (`fonttools ttLib.woff2 compress`, ~1 MB pro Gewicht) und unter
  `apps/dashboard/public/fonts/` gebündelt (Vite-Konvention für
  unverändert durchgereichte statische Assets — kein npm-Paket für die
  gepatchte Version verfügbar, nur für die ungepatchte Basisschrift). Neue
  `terminal-nerd-fonts.css` mit den drei `@font-face`-Regeln, importiert
  in `main.ts`. `terminalFonts.ts`s `BUNDLED_FONTS`-Katalog um einen
  neuen ersten Eintrag erweitert (elf Fonts insgesamt jetzt). Lizenzen
  (beide SIL OFL 1.1, wie jede andere gebündelte Schrift hier) liegen als
  `.txt`-Dateien neben den Font-Dateien, wie von OFL bei Weiterverbreitung
  gefordert.
- **Architektur-Review-Nachbesserung**: Kommentar ergänzt, der den
  bewussten Kompromiss `public/fonts/` (kein Cache-Busting) vs.
  `src/assets/` (das bestehende Muster für andere gebündelte Binär-Assets,
  MIT Cache-Busting über Vite's Modul-Graph) benennt — hier bewusst
  `public/` gewählt, da die Dateien ohnehin fest im Tauri-App-Bundle
  landen, nicht über einen langlebigen HTTP-Cache ausgeliefert werden.
  Zusätzlich klargestellt, dass der bestehende `PureNerdFont`-Fallback
  auch hinter dieser neuen, bereits vollständigen Schrift liegen bleibt —
  harmlos (greift nie), kein Zeichen von Unvollständigkeit.
- **Verifiziert**: `npm run check` + `npx vitest run` (375 Tests, 2 neu)
  grün, kein Rust betroffen. Live mit `agent-browser` gegen echtes
  Chromium: alle drei Gewichte in `document.fonts` registriert; eine
  synthetische Testzeile mit echten Powerline-/Icon-Codepoints
  (abgerundete Kappen U+E0B6/U+E0B4, Haus-Icon U+F015, Git-Branch-Icon
  U+F418) rendert jetzt korrekt abgerundet mit richtigen Icon-Formen —
  sichtbar besser als der alte Fallback-Ansatz. Reale Bestätigung am Mac
  (echter p10k-Prompt, nicht nur synthetische Testdaten) steht aus.

## Checkpoint 5n — Hintergrundfarben ignorierten das Theme komplett (Owner-Live-Test am echten Mac, 2026-09-15) — KOMPLETT

Nach Checkpoint 5m am echten Mac getestet: "Die Leiste sieht von den
Symbolen her gut aus aber nicht was die Farben angeht" — mit Screenshot.
Auf Nachfrage bestätigt: Catppuccin Mocha war die ganze Zeit ausgewählt.

- **Root Cause, per direktem Reproduktions-Test vom Owner bestätigt**: In
  `TerminalScreen.ts`s `draw()` bekam der Aufruf für die Zell-
  **Hintergrundfarbe** (`resolveColor(cell.bg, defaultBg)`) gar kein
  drittes Argument — `options` war also immer `undefined`, wodurch JEDE
  indizierte Hintergrundfarbe (SGR `4x`/`48;5;n` — genau das, was
  Prompt-Frameworks wie Powerlevel10k für ihre farbigen Segment-
  Hintergründe nutzen) stillschweigend auf `resolveColor`s eigenen
  Xterm-Standard zurückfiel, unabhängig vom gewählten Theme. Der
  Geschwister-Aufruf für die **Vordergrundfarbe** ein paar Zeilen weiter
  unten übergab `{ palette, bright: ... }` schon immer korrekt — genau
  deshalb sahen Icons/Text theme-korrekt aus, während Hintergrund-Pills
  es nie waren. Vom Owner selbst mit einem minimalen Repro bewiesen, das
  die Shell komplett umgeht: `printf '\033[44m    \033[0m\n'` (reines SGR
  44 = indizierter Hintergrund Blau) zeigte unter Catppuccin Mocha
  weiterhin Xterms `#0000ee` statt Catppuccins `#89b4fa` — damit
  zweifelsfrei als echter Engine-Bug bestätigt, nicht als p10k-/Shell-
  Konfigurationsproblem.
- **Fix**: neue exportierte, pure Funktion `resolveBgColor(color, fallback,
  palette)` (wrapt `resolveColor(color, fallback, { palette })`), extra
  herausgezogen für Unit-Testbarkeit — spiegelt das bestehende Muster
  von `cellFont`/`drawBlockElement` in dieser Datei (`draw()` selbst
  kann ohne echten Canvas-Context nicht getestet werden). `draw()`s
  Hintergrund-Fill nutzt jetzt `resolveBgColor(cell.bg, defaultBg,
  palette)` statt des kaputten nackten Aufrufs. Drei neue Tests
  (indizierte Farbe gegen ein bewusst abweichendes Custom-Palette,
  `"default"` fällt auf den Fallback zurück, True-Color-RGB bleibt
  unverändert durchgereicht).
- **Architektur-Review**: keine weiteren Fundstellen derselben Bug-Klasse
  im gesamten Codebase (`resolveColor` hat nur genau diese zwei
  Aufrufstellen; `terminal.svelte`s `defaultBg`/`cursorColor`/
  `selectionColor` laufen nie über `resolveColor`, sondern über
  CSS-Tokens/`THEME_DEFAULT_COLORS`). Nur zwei LOW-Findings (keine
  Handlung nötig).
- **Verifiziert**: `npm run check` + `npx vitest run` (378 Tests, 3 neu)
  grün, kein Rust betroffen. Live mit `agent-browser` gegen echtes
  Chromium, pixel-genau via `getImageData`: indizierte Hintergrundfarbe 4
  unter Catppuccin Mocha rendert jetzt exakt `rgb(137,180,250)` =
  `#89b4fa` (vorher `#0000ee`). Vom Owner am echten Mac als tatsächliche
  Ursache bestätigt (per eigenem `printf`-Repro), volle Bestätigung nach
  diesem Fix steht noch aus.

## Checkpoint 5o — Settings-Seite Redesign (Owner-Feedback, 2026-09-15) — KOMPLETT

"Das könnten wir im Anschluss noch etwas schöner Design.. sieht nicht
toll aus. Viele Felder/Dropdown zu klein. Irgendwie wie von einem
Anfänger Design." — mit Screenshot.

- **Root Cause**: jedes Feld (Text, Select, Zahl, Checkbox) lief durch
  dieselbe `<label>`-Flex-Zeile mit fest `width: 9em` für die Kontrolle,
  unabhängig vom tatsächlichen Feldtyp oder der Panel-Breite (diese Seite
  ist die Rückseite einer vom Owner frei skalierbaren Kachel). Ergebnis:
  Text-/Select-Felder mit längerem Inhalt ("JetBrainsMono Nerd Font
  Mono", volle Pfade, Theme-Namen) wurden mitten im Wort abgeschnitten,
  und bei einer breiten Kachel blieb ein großer, unausgewogener Leerraum
  zwischen Label und der winzigen Kontrolle.
- **Fix**: reines Layout-Redesign, keine Verhaltensänderung (jede
  `on*`/`bind:value`-Verdrahtung bleibt exakt wie vorher). Text-/Select-/
  Textarea-Felder sind jetzt standardmäßig gestapelt (Label über der
  Kontrolle, Kontrolle auf `width: 100%`) — genau die Feldtypen, bei denen
  eine feste kleine Breite der eigentliche Bug war. Zahlen, Checkboxen,
  der Opacity-Regler und zwei kurze Selects (Cursor-Stil, Cursor-Blink)
  behalten bewusst eine kompakte Zeile (`.field-inline`). Verwandte Felder
  sind jetzt in drei Abschnitte gruppiert (Session, Font, Appearance) mit
  Überschrift — spiegelt das bestehende Muster dieser App (siehe z. B.
  `routines-board-settings.svelte`s eigenes `h3`) statt einer flachen,
  ungruppierten Liste. Dabei nebenbei einen kleinen bestehenden
  Inhalts-Bug behoben: "Visual bell" war laut eigenem Hinweistext dieser
  Seite ein "erst beim nächsten Spawn"-Setting, stand aber visuell in der
  Gruppe der Live-Anwendungen — jetzt korrekt bei "Session" einsortiert.
- **Verifiziert**: `npm run check` + `npx vitest run` (378 Tests,
  unverändert — reine Präsentationsänderung) grün. Live mit
  `agent-browser` gegen echtes Chromium: alle drei Abschnitte
  gegengeprüft, keine abgeschnittenen Texte mehr, keine verwaisten
  CSS-Klassen. Kein Rust betroffen, kein Sub-Agent-Review (reines
  Layout, keine neue Logik, ein einzelnes File unter dem
  ">3 Dateien"-Trigger).

## Checkpoint 5p — Kaputte Box-Drawing-Linien (Owner-Feedback mit
Claude-Code-Screenshot, 2026-09-15) — KOMPLETT

"Du selbst ( Claude Code ) siehst aber in unserem Terminal echt nicht
gut aus... da passt so einiges nicht:" — mit Screenshot der eigenen
Claude-Code-TUI, die in unserem Terminal lief. Per ImageMagick-Zoom-Crop
zwei getrennte Probleme identifiziert:

1. Eine doppelt/versetzt wirkende horizontale Trennlinie über zwei
   Boxen ("Try 'refactor runner.rs'" / "auto mode on..."-Fußzeile) —
   **gefixt, siehe unten**.
2. Ungewöhnlich viel/durchgehende Unterstreichung auf den meisten
   Textsegmenten — **NICHT gefixt, zurückgestellt** (siehe eigener
   Abschnitt unten).

### Problem 1: Box-Drawing-Linien

- **Root Cause**: exakt dieselbe Bug-Klasse wie Checkpoint 5k, nur für
  Linienzeichen statt Füllzeichen. Checkpoint 5k hatte bereits
  dokumentiert (eigener Kommentar im Code), dass Unicode Block Elements
  (U+2580–259F, Füllzeichen/Shades) über `ctx.fillText()` je nach Font
  inkonsistent rendern — als Fix wurde `drawBlockElement()` eingeführt,
  das diese Zeichen prozedural über Canvas-Rect-Primitives statt über
  die Font-Glyphen zeichnet. Box Drawing Characters (U+2500–257F, das
  "leichte Einzellinien"-Subset `─│┌┐└┘├┤┬┴┼╭╮╰╯`, 15 Zeichen) waren
  als bekannter, noch offener Folgefall in genau diesem Kommentar
  vermerkt — jetzt vom Owner am echten Beispiel (Claude Codes eigene
  Box-Rahmen) bestätigt: dieselbe Font-Glyph-Inkonsistenz, dieses Mal
  bei Linien statt Flächen.
- **Fix**: gleiches Muster wie `drawBlockElement()`, für Linien statt
  Flächen. Neue `CORNER_TOP_LEFT`/`CORNER_TOP_RIGHT`/
  `CORNER_BOTTOM_LEFT`/`CORNER_BOTTOM_RIGHT`-Segment-Konstanten (je ein
  L-förmiges 2-Segment-Array in zellrelativen 0–1-Koordinaten
  `[x0,y0,x1,y1]`), eine exportierte `BOX_DRAWING_LINES`-Tabelle, die
  alle 15 Zeichen auf ihre Segment-Arrays abbildet — die vier
  gerundeten Eckvarianten (`╭╮╰╯`) verweisen dabei bewusst auf
  dasselbe Array-Objekt wie ihre scharfen Gegenstücke (keine Kopie, da
  wir Ecken ohnehin nicht tatsächlich runden — visuell ununterscheidbar
  bei `BOX_LINE_WIDTH = 1`), und eine neue `drawBoxDrawingLine(ctx,
  glyph, x, y, cellW, cellH, color): boolean`-Funktion, die pro Zeichen
  jedes Segment über `beginPath()/moveTo()/lineTo()/stroke()` zeichnet
  und `false` zurückgibt, wenn das Zeichen nicht in der Tabelle steht
  (nicht zuständig). In `draw()`s bestehender Fallback-Kette pro Zelle
  als zweite Prüfung nach `drawBlockElement()` verdrahtet: `if
  (!drawBlockElement(...) && !drawBoxDrawingLine(...)) { ...fillText-
  Fallback... }` — derselbe "bin ich zuständig?"-Dispatch, den 5k schon
  etabliert hat.
- **Architektur-Review**: Geometrie aller 15 Einträge unabhängig
  nachgerechnet, keine Fehler gefunden. Ein echtes HIGH-Finding: die neuen
  1px-Strokes hatten kein Pixel-Grid-Snapping, obwohl genau diese Datei
  bereits einen dokumentierten Präzedenzfall dafür hat (`outline`-Cursor,
  `x + 0.5`/`y + 0.5`-Inset) — je nach Parität von `cellW`/`cellH` konnte
  eine Segment-Mittelkoordinate exakt auf einer Pixelgrenze statt einem
  Pixelzentrum landen und dadurch als 2px-Unschärfe statt scharfer 1px-
  Linie rendern; wäre praktisch der "verschwommene Linie"-Nachfolgebug
  der hier eigentlich behobenen "doppelte Linie" gewesen. Gefixt mit einer
  neuen `snapToPixelCenter()`-Hilfsfunktion (`Math.floor(v) + 0.5`,
  idempotent für bereits korrekt zentrierte Koordinaten), angewendet auf
  jeden Segment-Endpunkt. Drei MEDIUM-Findings gefixt: veralteter 5k-
  Kommentar, der Box-Drawing-Linien fälschlich noch als "nicht
  implementiert" beschrieb, jetzt korrigiert und verweist auf
  `BOX_DRAWING_LINES`; die inline `!a(...) && !b(...)`-Dispatch-Kette in
  `draw()` zu einem `PROCEDURAL_GLYPH_HANDLERS`-Array samt `.some(...)`
  umgebaut (dieses Handler-Paar ist laut eigener Aufgabenstellung die
  Vorlage für künftige Glyph-Handler — jetzt erweiterbar ohne `draw()`
  selbst anzufassen); der fünffach ausgeschriebene `[number, number,
  number, number]`-Segmenttyp zu einem gemeinsamen `CellRect`-Typalias
  zusammengefasst. Ein LOW-Finding gefixt: eine 147 Zeichen lange
  Testzeile (Projekt-Standard 120 Zeichen) umgebrochen. Ein LOW/INFO-
  Hinweis (die geteilten `CORNER_*`-Arrays sind nur typseitig,
  nicht zur Laufzeit `readonly`) bewusst nicht behoben — spiegelt eine
  bereits bestehende, nicht neu eingeführte Lücke bei
  `BLOCK_ELEMENT_RECTS`, vom Review selbst als "nicht blockierend"
  eingestuft.
- **Verifiziert**: `npm run check` (0 Fehler) + `npx vitest run`
  (382/382, 4 neue Tests unter `describe("BOX_DRAWING_LINES (Checkpoint
  5p)")`: Zeichensatz-Vollständigkeit gegen die 15 erwarteten Zeichen,
  Segment-Gültigkeit (achsenparallel, im Bereich [0,1], Länge ≠ 0),
  Objekt-Identität der gerundeten Ecken mit ihren scharfen Gegenstücken
  (`toBe`, nicht `toEqual`), und eine Struktur-Prüfung speziell für
  `┼`) grün. Live mit `agent-browser` gegen echtes Chromium: temporäre
  Devmock-Fixture (`terminal_spawn`-Mock-Case mit einer kleinen
  `┌─────────┐ / │ hello │ / ├─────────┤ / │ world │ / └─────────┘`-Box,
  über ein Python-Skript mit `\uXXXX`-Escapes eingefügt, da der
  `Edit`-Tool literale Box-Drawing-Unicode-Zeichen in einem früheren
  Versuch stillschweigend zu leeren Strings verstümmelt hatte) plus
  temporärer `terminal.svelte`-Bypass für den echten Tauri-`Channel`
  (beide vor dem Commit zurückgesetzt) — Screenshot + 4×-Zoom-Crop
  zeigen ein sauberes, durchgezogenes Rechteck ohne Versatz/Doppelung,
  Ecken und das Kreuzungszeichen (`├─┤`) korrekt ausgerichtet.

### Problem 2: Unterstreichung — zurückgestellt

- Untersucht, ob unser Rust-Engine OSC-8-Hyperlinks unterstützt (Ghostty
  zeigt Hyperlink-Text typischerweise nur bei Hover unterstrichen) —
  per `grep` in `screen.rs` bestätigt: keine dedizierte OSC-8-Behandlung
  vorhanden. Konnte aber NICHT abschließend klären, ob die im Screenshot
  sichtbare Unterstreichung (a) legitimes SGR-4-Styling aus Claude Codes
  eigener TUI ist, das in Ghostty identisch aussehen würde, oder (b) ein
  echter Unterschied ist, der daher kommt, dass Ghostty OSC-8-verpackten
  Hyperlink-Text nur bei Hover unterstreicht, während wir ihn permanent
  unterstrichen zeigen.
- Bewusst NICHT spekulativ weitergebaut (OSC-8-Hyperlink-Tracking wäre
  ein nicht-triviales neues Feature: OSC-8-Parsing, Pro-Zelle/Pro-Lauf-
  Hyperlink-Zustand, geänderte Unterstreichungs-Logik für Text innerhalb
  eines Hyperlinks) ohne vorherige Bestätigung, dass das tatsächlich der
  Mechanismus ist. Owner gefragt, ob Ghostty bei derselben Claude-Code-
  Ausgabe dasselbe Unterstreichungsmuster zeigt.
- **Owner-Antwort (mit Screenshots)**: Ghostty zeigt dieselben Begriffe
  („Claude Code", „Sonnet 5 · Claude Pro", der Pfad, „auto mode on",
  „shift+tab to cycle", „agents", …) OHNE Unterstreichung — stattdessen
  nur eine Hervorhebung (farbiger Hintergrund-Kasten) beim Hover über
  einen einzelnen Begriff. Das Muster in unserem eigenen Screenshot
  bestätigt das zusätzlich unabhängig von der Owner-Aussage: es ist
  selektiv pro klickbarem Begriff unterstrichen (Satzzeichen/Leerzeichen
  dazwischen nicht) — genau die Struktur von OSC-8-Hyperlinks, nicht von
  durchgehendem SGR-4-Styling eines ganzen Satzes. Damit bestätigt:
  Mechanismus (b), siehe Checkpoint 5r unten (Hyperlink-Tracking selbst
  ist eigenständig genug für einen eigenen Checkpoint, nicht rückwirkend
  in 5p reingequetscht).

## Checkpoint 5q — Shift+Tab (Moduswechsel in Claude Code) tat nichts
(Owner-Feedback, 2026-09-15) — KOMPLETT

„Was auch noch in Claude nicht geht ist Shift Tab um zwischen den
Modien zu wechseln." — Claude Code (und viele andere volle TUIs)
verwenden Shift+Tab als festen Shortcut zum Durchschalten zwischen
Modi (Plan-Modus, Auto-Accept-Edits, …).

- **Root Cause**: `terminalInput.ts`s `keyToBytes()` kannte gar kein
  `shiftKey`-Argument — die `"Tab"`-Fallunterscheidung gab immer
  bedingungslos das reine `0x09`-Byte zurück, unabhängig davon, ob
  Shift gedrückt war. Ein Browser-`KeyboardEvent` liefert für ein
  „geshiftetes" Tab kein eigenes C0-Byte (anders als z. B. Backspace);
  das muss die Anwendung selbst unterscheiden und ausdrücklich anders
  kodieren.
- **Fix**: `keyToBytes()` bekommt einen neuen, auf `false` defaulteten
  dritten Parameter `shiftKey` (jeder bestehende Aufrufer — auch alle
  bisherigen Tests — bleibt dadurch unverändert kompilierbar). Bei
  Shift+Tab wird jetzt `CSI Z` (`\x1b[Z`, "Cursor Backward Tabulation")
  gesendet statt des reinen Tab-Bytes — die de-facto-Konvention, die
  Readline/zle und praktisch jede volle TUI (Claude Codes eigene CLI
  eingeschlossen) für Shift+Tab abfragt. `terminal.svelte`s
  `handleKeydown` reicht jetzt `e.shiftKey` mit durch.
- **Verifiziert**: `npm run check` (0 Fehler) + `npx vitest run`
  (384/384, 2 neue Tests: Shift+Tab → `CSI Z`, plain Tab sowohl explizit
  als auch über den `shiftKey`-Default weiterhin `0x09`, und Ctrl+Shift+
  Tab — die Ctrl-Branche greift ohnehin nur bei einzeichigen Keys, also
  weiterhin `CSI Z`) grün. Kein Sub-Agent-Review: drei Dateien
  (`terminalInput.ts`, `terminal.svelte`, `terminalInput.test.ts`),
  exakt am „mehr als drei Dateien"-Trigger, nicht darüber — dieselbe
  Argumentation wie Checkpoint 5o (reine, in sich geschlossene
  Erweiterung eines bereits unit-getesteten, reinen Funktions-Patterns,
  identisch zum bewährten Checkpoint-5i-Muster für dieselbe Datei).

## Checkpoint 5r — OSC-8-Hyperlinks: permanentes Unterstreichen unterdrücken
(Owner-Feedback mit Ghostty-Vergleichs-Screenshots, 2026-09-15) — KOMPLETT

Fortsetzung von Checkpoint 5p, Problem 2 (dort zurückgestellt). Owner hat
per Screenshot-Vergleich bestätigt: Ghostty zeigt „Claude Code", „Sonnet
5 · Claude Pro", den Pfad, „auto mode on", „shift+tab to cycle",
„agents" usw. OHNE Unterstreichung, nur eine Hervorhebung (Hintergrund-
Kasten) bei Hover. In unserem Screenshot war die Unterstreichung
selektiv pro klickbarem Begriff (Satzzeichen dazwischen nicht
unterstrichen) — das Muster von OSC-8-Hyperlinks, nicht von
durchgehendem SGR-4-Styling eines ganzen Satzes.

- **Root Cause**: Claude Codes CLI verpackt diese Begriffe in echte
  OSC-8-Terminal-Hyperlinks, dabei üblicherweise zusätzlich in ein
  literales SGR 4 (Unterstreichung) als Klartext-Fallback für Terminals
  ohne Hyperlink-Unterstützung. Ghostty (wie die meisten
  hyperlink-fähigen Terminals) unterdrückt dieses Fallback-Underline zu
  Gunsten der eigenen Hover-Darstellung. Unser Rust-Engine kannte OSC 8
  überhaupt nicht — jedes Unterstreichungs-Byte wurde einfach immer
  gezeichnet.
- **Fix (Rust, `crates/axiomata-terminal/src/screen.rs`)**: `Cell`
  bekommt ein neues `pub hyperlink: bool`-Feld. `Screen` bekommt ein
  neues `active_hyperlink: bool`-Feld — bewusst NICHT Teil von `pen`
  (dort leben `bold`/`underline`/Farben, und `CSI 0 m` ersetzt `pen`
  komplett via `self.pen = Cell::default()`): eine Hyperlink-Grenze ist
  keine SGR-Eigenschaft, eine App darf Farben mitten in einem Link frei
  ändern/zurücksetzen, ohne den Link zu beenden — nur ein erneutes OSC 8
  mit leerer URI beendet ihn wirklich. Neue `osc_dispatch()`-Methode
  erkennt OSC 8 (`params[0] == b"8"`) und setzt `active_hyperlink` je
  nachdem, ob `params[2]` (die URI) nicht-leer ist. `print()` übernimmt
  `active_hyperlink` explizit auf jede gedruckte Zelle (nicht über
  `pen`). `blank_cell()` (Erase/Clear) setzt `hyperlink` immer auf
  `false` — jetzt über `..Cell::default()` statt eines vollständig
  ausgeschriebenen Feld-Literals (Architektur-Review-Fix, siehe unten).
- **Fix (TS, `TerminalScreen.ts`)**: `TermCell` bekommt `hyperlink:
  boolean`. Eine neue, exportierte reine Funktion `shouldDrawUnderline
  (cell): boolean` (`cell.underline && !cell.hyperlink`) ersetzt die
  bisherige Inline-Bedingung in `draw()` — kein Hover-Highlight (bräuchte
  neues Maus-Tracking, das dieser Renderer noch nicht hat), aber
  Ghosttys „nicht gehovert"-Zustand (kein Underline) ist bereits eine
  klare Verbesserung gegenüber dem bisherigen Dauer-Underline.
- **Architektur-Review (Sub-Agent)**: Ein echtes **CRITICAL**-Finding —
  `active_hyperlink` wurde beim Alt-Screen-Wechsel (`CSI ?1049h`/`l`,
  `vim`/`htop`/etc.) weder gesichert noch wiederhergestellt, obwohl
  `pen` genau das an derselben Stelle bereits korrekt tut: ein auf der
  primären Ebene offener Hyperlink hätte in die ersten Zeichen der
  frischen Alt-Screen-Grid „durchgesickert", und ein vom
  Alt-Screen-Programm offen gelassener Hyperlink hätte beim Verlassen
  zurück auf die primäre Ebene durchgesickert — exakt dieselbe Bug-Klasse,
  vor der der eigene Code-Kommentar zu `active_hyperlink` bereits für
  `CSI 0 m` warnte, nur an der Alt-Screen-Grenze statt der
  SGR-Reset-Grenze. Gefixt: `SavedPrimary` bekommt ein eigenes
  `active_hyperlink`-Feld, `enter_alt_screen` sichert den alten Wert und
  setzt für die frische Alt-Screen-Grid `false`, `exit_alt_screen`
  stellt ihn wieder her — exakt gespiegelt an `pen`s eigener Behandlung.
  Zwei neue Rust-Tests (Leck in beide Richtungen ausgeschlossen, plus ein
  positiver Test: ein noch offener Hyperlink übersteht den
  Alt-Screen-Roundtrip tatsächlich). Zwei MEDIUM-Findings ebenfalls
  gefixt: `blank_cell()` auf `..Cell::default()` umgestellt (verhindert,
  dass ein künftiges neues `Cell`-Feld dort vergessen werden kann); die
  TS-Unterstreichungs-Bedingung in die benannte, jetzt unit-getestete
  `shouldDrawUnderline()`-Funktion ausgelagert (reine Boolean-Logik ohne
  Canvas-Abhängigkeit — hatte keinen Grund, ungetestet zu bleiben, anders
  als das umliegende `fillRect`/`fillText`, das echtes Canvas braucht).
  Ein LOW/INFO-Hinweis übernommen: Doc-Kommentar ergänzt, dass OSC 8s
  `id=`-Parameter und die URI selbst bereits geparst, aber bewusst nicht
  auf `Cell` gespeichert werden (nur als Bool für die
  Underline-Entscheidung gebraucht) — als Fingerzeig für ein künftiges
  Klick-zum-Öffnen- oder Hover-Feature, nicht als vergessene Arbeit.
- **Verifiziert**: `cargo check`/`clippy -p axiomata-terminal --tests --
  -D warnings`/`fmt --check` grün (kein `cargo test` in dieser Sandbox
  möglich, bekanntes Xcode-Lizenz-Problem — 7 neue Rust-Tests liegen
  bereit für den nächsten `cargo test` am Mac des Owners). `cargo check`
  für `src-tauri` ebenfalls grün (Wire-Format `Cell`↔`TermCell` bleibt in
  Sync). `npm run check` (0 Fehler) + `npx vitest run` (387/387, 3 neue
  Tests für `shouldDrawUnderline`) grün. Live mit `agent-browser` gegen
  echtes Chromium (vor dem Review-Refactor, mit temporärer
  Devmock-Fixture, seither zurückgesetzt): eine `underline: true,
  hyperlink: false`-Zelle blieb unterstrichen, eine `underline: true,
  hyperlink: true`-Zelle verlor die Unterstreichung — beide wie erwartet.

## Checkpoint 5s — Durchgehende Unterstreichung: eigentliche Ursache war kein
OSC-8-Problem, sondern ein CSI-Marker-Parsing-Bug (Owner-Screenshot nach
CP5r, 2026-09-15/16) — KOMPLETT

Owner meldete nach CP5r per Screenshot: Terminal-Text weiterhin komplett
unterstrichen, sichtbar unverändert gegenüber vorher. Statt direkt einer
zweiten Hyperlink-Theorie nachzugehen, wurde die echte `claude`-Binary
(exakt die App-Version, v2.1.273) mit genau den Env-Variablen gestartet,
die `pty.rs::spawn` setzt (`TERM=xterm-256color`, `COLORTERM=truecolor`,
kein `TERM_PROGRAM`), und die rohen PTY-Bytes des Begrüßungs-Banners
mitgeschnitten (`script -q -F ... claude`, dann `cat -v`/`od -c`).

- **Root Cause**: Der Banner-Text ("Claude Code", "Sonnet 5 · Claude
  Pro", Pfad, "auto mode on …") enthält in den echten Bytes **weder**
  ein SGR-4-Underline **noch** ein OSC-8 — CP5r/OSC-8 war von Anfang an
  die falsche Spur für dieses zweite Problem. Direkt am Sitzungsanfang,
  vor jedem gedruckten Zeichen, sendet die CLI aber
  `\x1b[>4m\x1b[<u` (xterms `modifyOtherKeys`-Konfiguration, Marker
  `>`, Endbyte `m`). `Screen::csi_dispatch` prüfte auf einen führenden
  Intermediate-Marker bisher **nur** `?` (für DEC-Private-Modes wie
  `?1049h`); jeder andere Marker (`>`, `<`, `=`) fiel durch in den
  generischen `match action`-Block, wo `'m' => apply_sgr(params)`
  ausschließlich auf das Endbyte prüft, unabhängig vom Marker.
  `CSI > 4 m` wurde dadurch als reines SGR-Code-4 (Underline)
  fehlinterpretiert und landete auf `pen` — und blieb dort für den Rest
  der Sitzung hängen, weil danach nie ein `CSI 0m`/`CSI 24m` kam. Jedes
  folgende gedruckte Zeichen erbte `underline: true`. Erklärt exakt das
  Symptom: nicht selektiv pro Begriff (das wäre OSC-8 gewesen), sondern
  durchgehend alles ab dem allerersten Zeichen.
- **Fix (`crates/axiomata-terminal/src/screen.rs::csi_dispatch`)**: Jeder
  nicht-leere Intermediate-Marker wird jetzt als eigene, von dieser
  Engine nicht modellierte Vendor-/Private-Sequenzfamilie behandelt —
  nur bei `?` wird weiterhin auf `h`/`l` (Private-Mode-Set/Reset)
  geprüft, jeder andere Marker (`>`, `<`, `=`, …) ist ein reines No-op,
  bevor der generische, markerlose `match action`-Block (SGR
  eingeschlossen) überhaupt erreicht wird — echte SGR trägt laut
  ECMA-48 nie einen Intermediate-Marker.
- **Nebenfund beim ersten echten `cargo test`-Lauf dieser Kette**: CP5rs
  eigener Alt-Screen-Leck-Test
  (`alt_screen_boundary_does_not_leak_hyperlink_state_either_direction`)
  war selbst fehlerhaft, nicht die geprüfte Funktionalität — `cargo
  test` konnte in CP5rs Session nie laufen (Sandbox-Linker-Problem), lief
  hier zum ersten Mal wirklich durch und deckte es auf: der Testaufbau
  öffnete einen Hyperlink auf der Primärebene und schloss ihn vor dem
  zweiten Teilszenario nie wieder, wodurch der zweite Teil (den
  Alt-Screen offen gelassenen Link) versehentlich mit einem *bereits vor
  dem Wechsel* offenen Primär-Hyperlink vermischte — dessen Wiederkehr
  beim Verlassen ist korrektes, gewolltes Verhalten (siehe den positiven
  Roundtrip-Test), keine echte Leck-Situation. Test in zwei unabhängige
  `Screen`-Instanzen aufgeteilt statt einer wiederverwendeten; die
  eigentliche `enter_alt_screen`/`exit_alt_screen`-Logik brauchte keine
  Änderung.
- **Neuer Regressionstest**: `a_non_dec_private_marker_csi_m_sequence_is_not_read_as_sgr`
  — `CSI > 4 m` gefolgt von einem Zeichen darf kein `underline: true`
  auf der Zelle hinterlassen.
- **Verifiziert**: `cargo test -p axiomata-terminal` lief diesmal
  tatsächlich durch (48/48 `screen`-Tests grün, inkl. Neuerung + Fix);
  vereinzelte PTY-Teardown-Timeouts sind das in `pty.rs` bereits
  dokumentierte Umgebungs-Flake (jeder Lauf ein anderer betroffener
  Test), keine Regression. `clippy -p axiomata-terminal --tests -- -D
  warnings` und `fmt --check` grün. `cargo check -p Axiomata-OS`
  (src-tauri) grün — Wire-Format unverändert. Kein Frontend-Fix nötig:
  `shouldDrawUnderline`/`cell.hyperlink` aus CP5r sind funktional
  korrekt, das Problem lag ausschließlich im Rust-Parsing vor der
  Cell-Erzeugung. Owner-Live-Test am Mac steht noch aus.

## Checkpoint 5t — Cmd+C piepste bei jedem Kopieren (Owner-Live-Test am Mac,
2026-09-16) — KOMPLETT

Owner-Feedback aus dem ersten echten Live-Test der 5f–5s-Kette: Maus-Drag-
Auswahl und `Cmd+C`/`Cmd+V` funktionieren beide (Text landet korrekt in der
Zwischenablage), `Cmd+C` verursacht aber jedes Mal zusätzlich einen
System-Ton.

- **Root Cause**: Die Terminal-Auswahl ist reiner interner JS-State
  (`selStart`/`selEnd`), nie eine echte `document.getSelection()`, und
  `src-tauri` definiert kein natives Edit-Menü (kein `Copy`-Menüpunkt mit
  Key-Equivalent). `Cmd+C` fällt dadurch komplett auf WKWebViews
  Standard-`copy:`-Verhalten zurück, findet dort aber nie eine echte
  DOM-Selektion zum Kopieren — macOS quittiert eine nicht greifende
  Tastenkombination mit dem bekannten Systemton ("Beep of Doom"), unabhängig
  davon, dass der Maus-Drag (`handlePointerUp`) die Zwischenablage vorher
  schon korrekt per `navigator.clipboard.writeText` befüllt hatte.
- **Fix (`apps/dashboard/src/modules/terminal.svelte`)**: neuer
  `oncopy={handleCopy}`-Handler auf demselben versteckten `<input
  class="typer">`, das auch `onpaste` schon trägt — ruft immer
  `e.preventDefault()` (verhindert, dass WKWebViews eigener,
  wirkungsloser Fallback überhaupt läuft, unabhängig davon ob etwas
  ausgewählt ist) und befüllt bei vorhandener Auswahl
  `e.clipboardData.setData(...)` direkt (synchron, im selben Event —
  bewusst nicht `navigator.clipboard.writeText`, das asynchron ist und mit
  dem bereits verhinderten Default-Verhalten race'n würde). Kein
  Verhaltens-Unterschied beim eigentlichen Kopieren, nur der Ton fällt weg.
- **Verifiziert**: `npm run check` (0 Fehler) + `npx vitest run` (387/387,
  unverändert — reines Event-Handling ohne neue reine Logik, die eigene
  Testabdeckung verdient hätte) grün. Owner-Live-Bestätigung am Mac steht
  aus.

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
