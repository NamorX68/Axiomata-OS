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
den Owner, siehe unten) ist ebenfalls KOMPLETT. Nächster Schritt: Live-Test
von 5f+5f2+5g+5h+5i+5j am Mac des Owners — diese ganze Kette entstand aus
genau solchen Live-Tests (oder, bei 5j, deren Chromium-Ersatz), nicht aus
automatisierter Verifikation allein.

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
