# Applikations-Ring im Orbit (App Ring)

## Checkpoint 5c — Tools und Apps verwaltbar machen (Owner-Wunsch, 2026-09-14)

Bis hierhin hatte die Ring-Seite links vom „+" (Builtins) keinerlei
Nutzerkontrolle: `isRingEligible` war eine feste Ausschlussliste
(`background`/`dev`/`md-file`, dazu bis hier `terminal` extra), jedes
verbleibende registrierte Modul erschien automatisch, und nur Claude
(als Code-Änderung an dieser Liste) konnte das ändern. Owner-Feedback:
"dem User steht keine Funktion zur Verfügung das zu tun" — sowohl für die
bestehenden Builtins ("Tools": Memory, Skills, Routines, ToDo, Calendar,
Reminders, Mail) als auch für das neue Terminal ("Apps" — Owner-Sprachgebrauch:
"Wenn ich von Apps spreche meine ich sowas wie unser neues Terminal").

**Begriffe** (Owner-Klärung): **Tool** = ein Singleton-Builtin
(`ModuleDefinition.singleton` `true`/unset) — Ring-Klick bringt eine
platzierte Instanz nach vorne, sonst erzeugt er eine. **App** =
`singleton: false` (aktuell nur Terminal) — jeder Ring-Klick erzeugt eine
neue Instanz, es gibt keine "die eine" Instanz zum Nach-vorne-Bringen. Die
Unterscheidung existierte technisch schon (`ModuleDefinition.singleton`),
wurde vor 5c nur nirgends für den Ring-Klick ausgewertet — `terminal` war
deshalb komplett vom Ring ausgeschlossen (`docs/plans/terminal.md`
Checkpoint 1), nicht weil es nicht ring-tauglich wäre, sondern weil der
Klick-Handler die Singleton-Unterscheidung noch nicht kannte.

**Umsetzung:**

- **`src/core/apps.ts`**: `isRingEligible` verliert die
  `terminal`-Sonderbehandlung (nur `background`/`dev`/`md-file` bleiben
  draußen — die Gründe dafür ändern sich nicht). Neue Funktion
  `listAllRingEligibleBuiltins()` (ring-fähig, unabhängig vom
  Sichtbarkeits-Status) neben dem bestehenden `listBuiltinApps()`, das jetzt
  zusätzlich nach dem neuen `hiddenBuiltins`-Store filtert. Neuer Store
  `hiddenBuiltins: Writable<string[]>` (Registry-`type`s, die der Owner
  versteckt hat) + Mutatoren `hideBuiltinApp(type)`/`showBuiltinApp(type)`/
  `loadHiddenBuiltins(list)` — spiegelbildlicher Default zu `userApps`
  (`userApps` startet leer, man opted pro App ein; `hiddenBuiltins` startet
  leer, jedes ring-fähige Builtin ist sichtbar bis man es explizit versteckt).
- **`src/core/persist.ts`**: neue Sektion `apps.hiddenBuiltins: string[]` in
  `dashboard.json`, `sanitizeHiddenBuiltins` (nicht-leere Strings,
  dedupliziert — Einzeleintrag verwerfen statt die ganze Datei zu
  gefährden, wie die übrigen Sanitizer hier).
- **`src/modules/second-brain.svelte`**: `handleAppClick` prüft jetzt
  `getModule(n.appType)?.singleton !== false`, bevor es eine platzierte
  Instanz nach vorne holt — bei `singleton: false` (Apps) wird immer eine
  neue Instanz erzeugt. Neue `hiddenBuiltins.subscribe(() => rebuild())`
  (analog zur bestehenden `userApps`-Subscription), damit ein Sichtbarkeits-
  Toggle im „+"-Dialog den Ring live aktualisiert, während der Dialog offen
  ist.
- **`src/modules/AppAddDialog.svelte`**: Umschalter „Intern"/„Extern" oben im
  Dialog (Owner-Vorschlag: "Radiostation"). „Extern" ist unverändert der
  bestehende Mac-App-Scan. „Intern" listet `listAllRingEligibleBuiltins()`
  — Tools und Apps in **einer gemeinsamen Liste** (Owner-Entscheidung: die
  Unterscheidung zeigt sich nur im Ring-Klick-Verhalten, nicht in dieser
  Liste), Zeile pro Modul mit demselben instant-ohne-Bestätigung-Toggle wie
  die externe Seite — aber mit eigenen Verb-Labels „Anzeigen"/„Ausblenden"
  statt „Hinzufügen"/„Entfernen": ein Builtin wird nie wirklich aus der App
  entfernt, nur vom Ring versteckt, und ist hier jederzeit wiederfindbar
  (anders als eine Mac-App, die `userApps` wirklich verlässt). Umschalten
  zwischen den Modi leert das Suchfeld. Rechtsklick-Kontextmenü
  (`AppContextMenu`/`menuActionsFor`) bleibt unverändert — Builtins bekommen
  weiterhin kein „Entfernen" dort, Verstecken geht ausschließlich über den
  „+"-Dialog.
- **`src/graph/model.ts`**: `glyphForModuleType` bekommt einen
  `"terminal"`-Fall (`GLYPH_CODEPOINTS` hatte den Material-Symbols-Codepoint
  für "terminal" schon, ungenutzt, seit der App-Ring-Gruppierung ihn für den
  Icon-Picker brauchte).

**Entschiedene Detailfragen** (Owner, kurz vorab bestätigt):
- Tools/Apps im Intern-Tab eine gemeinsame Liste, nicht zwei getrennte.
- Ein verstecktes Tool/App verschwindet komplett vom Ring (nicht nur
  ausgegraut) — gleiches Verhalten wie das Entfernen externer Apps.
- Terminal-Klick im Ring spawnt immer eine neue Instanz ohne Rückfrage.

**Kein Rust nötig** — `apps.hiddenBuiltins` läuft über denselben generischen
`get_dashboard_state`/`save_dashboard_state`-Mechanismus wie jede andere
Frontend-Sektion von `dashboard.json` (das Schema gehört dem Frontend, Rust
schreibt nur atomar; siehe `persist.ts`'s eigener Modul-Kommentar).

**Tests**: `src/core/apps.test.ts` (Terminal jetzt in
`listAllRingEligibleBuiltins`, `hideBuiltinApp`/`showBuiltinApp`/gefilterte
vs. ungefilterte Liste), `src/core/persist.test.ts`
(`sanitizeHiddenBuiltins`, `parseState`/`buildState`-Rundtrip). Kein
dedizierter Test für `handleAppClick`s neue Singleton-Verzweigung — die
Datei hat wie jedes andere Second-Brain-Modul keine eigene Testdatei
(DOM/Canvas-lastig, gleiche Konvention wie `terminal.svelte`).

**Verifikation**: `npm run check`, `npx vitest run`, `architecture-reviewer`
+ `docs-writer` (kein Rust berührt, daher kein `rust-test-engineer`/
`rust-dependency-auditor` nötig). Manueller Live-Test am Mac des Owners noch
offen (gleiche Einschränkung wie beim Terminal-Modul — diese Session hat
keine Accessibility-/Screen-Recording-Rechte).

Status: implementiert (alle 4 Checkpoints) + abschließender `architecture-reviewer`-
Lauf über den Gesamt-Diff samt `rust-dependency-auditor` für die neue
`home`-Dependency abgeschlossen, alle Befunde behoben. **Kurswechsel bei den
Builtin-Icons** (`appIcons.ts`/Bild-Rasterisierung ersetzt durch
handgezeichnete Vektor-Glyphen, siehe „Kurswechsel: Icons" ganz unten) — jede
Erwähnung von `appIcons.ts`/`iconSvg`/`getIcon` weiter oben in diesem
Dokument beschreibt den **verworfenen** ersten Ansatz, nicht den aktuellen
Stand. Bereit zur Durchsicht/zum Commit (Owner: Roman).

### Review-Nachbesserungen (nach Checkpoint 4)

Zwei HIGH- und drei MEDIUM-Befunde aus dem Gesamt-Review, die nur beim
Zusammenspiel aller Checkpoints sichtbar wurden bzw. aus dem CP2-Review noch
offen waren:

- **Kontextmenü-Status leckte beim Retargeting**: ein zweiter Rechtsklick auf
  eine andere User-App, bevor der erste Löschvorgang bestätigt/abgebrochen
  wurde, ließ `AppContextMenu`s `confirming`-State bestehen (Svelte
  aktualisiert nur die Props, montiert die Komponente nicht neu) — sprang
  also direkt zur Bestätigungsstufe des *neuen* Ziels. Fix:
  `{#key menu.node.id}` um `<AppContextMenu>` in `second-brain.svelte`
  erzwingt einen Remount pro Zielwechsel.
- **Offenes Menü konnte hängen bleiben**: `.brain` liegt hinter jeder Kachel
  (`#particle-slot`s z-index); ein Klick auf eine Kachel statt auf den
  Hintergrund erreichte `.brain`s eigenen schließenden `onclick` nie. Fix:
  ein globaler `window`-`click`-Listener (nur aktiv solange `menu` gesetzt
  ist) schließt das Menü bei jedem Klick außerhalb von `.app-context-menu`.
- **`oncontextmenu` unterdrückte das native Menü nur im App-Fall**: jetzt
  `e.preventDefault()` unconditional am Anfang von `onContextMenu`.
- **`discR`/Renderer-`R`-Kopplung fragil, aber nicht falsch**: dokumentiert
  statt umgebaut (die Annahme `view.zoom === 1` gilt aktuell überall, ein
  Kommentar an `discR`s Deklaration macht das jetzt explizit, statt es
  stillschweigend vorauszusetzen).
- **Aus CP2 noch offen, jetzt erledigt**: `Legend.svelte` nutzt jetzt
  `Exclude<NodeKind, "app">` statt eines nie gelesenen `app`-Pflichteintrags;
  `appIcons.test.ts` verifiziert jetzt echte Cache-Dedup (Konstruktions-
  zählung über ein gestubtes `Image`) statt nur „wirft nicht"; der
  User-App-Farbtest in `appRing.test.ts` prüft jetzt den exakten
  `areaColor(...)`-Wert, nicht nur Stabilität.
- Zusätzlich (LOW, aus dem Gesamt-Review): ein achter Rust-Test verifiziert
  das dokumentierte Symlink-Verhalten von `scan_apps` mit einem echten
  `std::os::unix::fs::symlink`.

### Nachträgliche Anpassungen (Owner-Feedback nach visueller Prüfung)

Nach `cargo tauri dev` kam echtes visuelles Feedback, das eine weitere
Fix-Runde plus einen zweiten (letzten) `architecture-reviewer`-Lauf über
genau diese Runde nach sich zog — inklusive `security-auditor` (LOW, keine
Pflicht-Fixes), `rust-performance-analyzer` (keine Änderung nötig),
`docs-writer` (keine Lücken) und `refactoring-specialist`:

- **App-Icons waren leer** — echter Doppel-Bug in `appIcons.ts`: `;utf8,`
  ist kein gültiger MIME-Parameter (WebKit lehnte jede Data-URL sofort ab),
  UND dimensionslose SVGs (nur `viewBox`, kein `width`/`height`) sind bei
  einem per `Image()` geladenen Data-URI in WebKit unzuverlässig
  größenbestimmt. Fix: `withExplicitSize()` injiziert eine feste Größe,
  `toDataUrl()` nutzt Base64 statt Prozent-Encoding. Beide Funktionen
  exportiert + getestet, inklusive eines Basis64-Rundtrip-Tests.
  `withExplicitSize` ist zusätzlich gegen künftige Icon-Strings abgesichert
  (überspringt, wenn schon ein `width=` vorhanden ist; warnt in DEV, wenn
  das `<svg `-Präfix nicht passt, statt still einen leeren Icon zu
  produzieren).
- **Beide Ringe verkleinert** — `ORBIT_FIT` (0.42 → 0.36) als eine einzige
  benannte Konstante statt zweier duplizierter Literale; da der App-Ring-
  Radius ein Vielfaches davon ist, schrumpfen beide Ringe proportional.
- **„+"-Button ist jetzt Teil des Rings** — Größe über die neue exportierte
  `appNodeRadiusPx(R)` (dieselbe Funktion, die die echten Ring-Icons
  skaliert) statt eines fixen 24px-Werts; Optik an einen ruhenden
  Builtin-Knoten angeglichen (Akzent-Rand via `color-mix`, Surface-Füllung).
- **`ORBIT_MAX`** (Cap für Skills/Routinen/zuletzt geänderte Dateien auf dem
  *inneren* Ring) von 36 auf 60 erhöht.
- **Gesamt-Review deckte eine Wechselwirkung der letzten beiden Punkte auf
  (HIGH)**: ein kleinerer Ring *und* mehr mögliche Icons gleichzeitig ließ
  die feste, nur von `R` abhängige Icon-Größe des inneren Rings bei
  Annäherung an `ORBIT_MAX` überlappen — kein Rand-, sondern der normale
  Zustand eines gut gefüllten Vaults. Fix: neue exportierte, getestete
  `rimNodeRadiusPx(R, onOrbitCount)` begrenzt die Icon-Größe zusätzlich
  über die tatsächlich pro Icon verfügbare Bogenlänge, nicht nur über `R` —
  bei wenigen Icons unverändertes Verhalten (die größenbasierte Grenze
  bleibt die bindende), bei vielen schrumpfen die Icons kontrolliert statt
  sich zu überlappen.
- **`refactoring-specialist`** hat dabei `drawNodeDisc`/`drawHotLabel` aus
  `render.ts`s Rand-Loop und `drawAppRing` extrahiert (waren Byte-für-Byte
  identischer Code) — reine Verhaltens-Erhaltung, mit den obigen Fixes ohne
  Konflikt zusammengeführt.
- `security-auditor`: LOW (unvalidierte, aber im bestehenden Vertrauens-
  modell für `dashboard.json`-Hand-Edits bereits akzeptierte
  `path`/`name`-Persistenz) — keine Pflicht-Fixes.
  `rust-performance-analyzer`: keine Änderung gerechtfertigt bei dieser
  Größenordnung (≤300 Einträge, einmalig pro Dialog-Öffnung).
  `docs-writer`: keine Dokumentationslücken gefunden.

## Ziel

Ein zweiter, äußerer Ring im Orbit-Dashboard (`second-brain`-Modul), der
links vom „+" (12 Uhr) die In-App-Module und rechts davon per „+" hinzugefügte,
auf dem Rechner installierte Mac-Apps zeigt.

Anforderungen (Owner-Feedback):

1. **In-App-Apps erscheinen automatisch** — die in der App angebotenen Module
   (Registry), ohne die `background`-Module, ohne Dev-Dummies und ohne
   `md-file` (braucht einen `path`, den es ohne Kontext nicht hat — ein
   blanker Ring-Klick würde eine kaputte Kachel erzeugen). Folge (v1): jedes
   verbleibende eingebaute Modul war damals ein Singleton (`memory-status`,
   `skills-deck`, `routines-board`, `todo`, `calendar`, `reminders`, `mail`)
   — keine Sonderbehandlung für Nicht-Singleton-Klicks nötig. **Überholt seit
   Checkpoint 5c** (siehe oben): Terminal (`singleton: false`) ist jetzt auch
   ring-fähig, `handleAppClick` unterscheidet Tool/App-Klickverhalten
   explizit über `ModuleDefinition.singleton`.
2. **Installierte Apps über „+-Button" anlegen** — der Button sitzt auf 12 Uhr
   dieses Rings; er öffnet einen Dialog, in dem man eine App des Rechners
   hinterlegen kann.
3. **Links/Rechts-Trennung** — In-App-Apps links des „+", Rechner-Apps rechts.
4. **Der neue Ring rotiert nicht.**
5. **Externe Apps sind entfernbar** — Rechtsklick auf einen Ring-Knoten öffnet
   ein Kontextmenü mit „Entfernen" → „„{label}" aus dem Ring entfernen?" /
   „Abbrechen" (Abbrechen schließt das Menü komplett); zusätzlich eine
   umschaltende „Hinzufügen"/„Entfernen"-Zeile im „+"-Dialog, dort **instant,
   ohne Bestätigung** (ein gezielter Listen-Klick ist strukturell weniger
   versehentlich als ein Rechtsklick mitten im Canvas). Beide Wege rufen
   dieselbe `removeUserApp(path)`; der Dialog-Weg ist der einzige, der auch
   funktioniert, wenn ein Ring-Knoten von einer Kachel verdeckt ist (der
   Hintergrund-Canvas liegt hinter Kacheln), und der einzige tastatur-/
   VoiceOver-erreichbare. Eingebaute Module reagieren auf Rechtsklick nicht.

## Architektur

### Frontend

- **`src/core/apps.ts` (neu)** — Daten-Layer:
  - `BuiltinApp` aus `registry.listModules()`, exkl. `background` (Second
    Brain), `md-file` (braucht einen `path`, den es ohne Kontext nicht hat)
    und Dev-Scaffolding — über ein neues `ModuleDefinition.dev?: boolean`
    (Review-Ergebnis aus Checkpoint 1: eine `dummy*`-Namenskonvention wäre ein
    impliziter Vertrag gewesen, den ein künftiges Dev-Modul ohne den Prefix
    stillschweigend verletzen könnte; die beiden `dummy`/`dummy-singleton`-
    Registrierungen in `modules/index.ts` setzen das Flag jetzt explizit).
    Damit erscheinen neu registrierte Module automatisch.
  - `UserApp { path, name }` + `userApps`-Store (`addUserApp`/`removeUserApp`).
    Identität einer `UserApp` = der Dateisystempfad (keine synthetische ID —
    bei einer lokalen, selbst über den Scan-Dialog befüllten Liste ist eine
    Pfad-Kollision rein theoretisch).
  - Persistenz: neue Top-Level-Sektion `apps.user` in `~/.axiomata/dashboard.json`
    (via `core/persist.ts`). Keine neue Rust-Command für die eigenen Apps.
- **`src/core/persist.ts`** — `apps.user` durch `parseState`/`buildState` tragen,
  Subscription auf `userApps` säubern (an `scheduleSave` hängen). Neue
  `sanitizeUserApps`, analog zu `sanitizeInstances`: verwirft Einträge ohne
  nicht-leeren String-`path`/`name` einzeln und dedupliziert nach Pfad, statt
  bei einem Hand-Edit von `dashboard.json` den ganzen Ring-Zustand zu
  gefährden.
- **`src/graph/model.ts`** — `NodeKind` um `"app"` erweitern; `GraphNode`
  bekommt `userApp?: boolean` (gestempelt bei jedem Modell-Rebuild aus dem
  `userApps`-Store, `true` nur für extern hinzugefügte Mac-Apps — trägt
  sowohl die Klick-Weiche als auch das Kontextmenü-Gate, ohne zur Klickzeit
  einen Store-Lookup oder ID-Pfad-Parsing zu brauchen), `appType?: string`
  (Builtins: Registry-`type` für `createInstance`/`bringToFront`), `appPath?:
  string` (User-Apps: absoluter Pfad für `openPath`/`removeUserApp` — bewusst
  nicht das bestehende `path`-Feld, das als workspace-relativ dokumentiert
  ist), `iconSvg?: string` (Builtins: das rohe `ModuleDefinition.icon`-SVG,
  s. u.). Neue Funktion `buildAppNodes(builtins, userApps, palette)` baut die
  fertigen Knotenobjekte (Farbe, `iconSvg`/`appType` bzw. `appPath`/`userApp`,
  Position `(0,0)` als Platzhalter) — spiegelt die bestehende Aufteilung
  „`buildModel` konstruiert, ein `layout.ts`-Aufruf positioniert" statt beides
  in einer Funktion zu vermischen. Die Hash-Twinkle-Phase-Funktion, die
  `buildModel` bisher als lokale Closure hatte, wurde dafür nach `nodePhase`
  extrahiert (kein Verhaltensunterschied, nur DRY zwischen den Knoten-Arten).
- **`src/graph/layout.ts`** — Konstante `APP_RING = 1.18` (Ringradius in
  Graph-Einheiten) + pure Funktion `layoutAppRing(builtinNodes: GraphNode[],
  userNodes: GraphNode[])`, die nur `x`/`y` in-place setzt (das Gegenstück zu
  `buildAppNodes` oben, wie `layoutRings`/`placeRing` es für die anderen
  Knotenarten schon tun):
  - „+" bleibt frei auf `-π/2` (12 Uhr).
  - Builtins links vom „+": Winkel in `(-π, -π/2)`, erster direkt links neben dem +.
  - User-Apps rechts vom „+": Winkel in `(-π/2, 0)`, erster direkt rechts neben dem +.
  - Keine `angle`-Abhängigkeit → statisch (nur der neue Ring; der innere Orbit
    behält seinen bestehenden `spin`-Schalter).
  - Fester Winkel-Schritt (`APP_RING_STEP`) pro Icon ab dem „+" (nicht über
    den ganzen Halbkreis verteilt), bewusst ohne Obergrenze in v1 — ab ~12
    Apps pro Seite wird's eng, siehe `ORBIT_MAX` als Präzedenzfall, falls das
    je relevant wird.
- **`src/graph/appIcons.ts` (neu)** — `getIcon(type, svg)`: rasterisiert ein
  Builtin-Icon (`GraphNode.iconSvg`) als `HTMLImageElement` für `drawImage`,
  gecacht nach `type`. Notwendig, weil der Canvas-2D-Kontext keine beliebige
  SVG-Markup direkt zeichnen kann — die bestehende `drawGlyph`-Funktion in
  `render.ts` ist ein kleines, handgeschriebenes Vektor-Pfad-Vokabular
  (`"hub"`/`"skill"`/`"code"`/…), kein SVG-Interpreter, und `ModulePicker`s
  `{@html m.icon}`-Ansatz funktioniert nur im echten DOM. Laden ist
  asynchron (`data:image/svg+xml`-URL + `Image.onload`); ein Cache-Miss
  liefert `null`, der Aufrufer überspringt das Zeichnen für den Frame — der
  ohnehin laufende `requestAnimationFrame`-Loop holt das Bild von selbst
  nach, sobald es geladen ist.
- **`src/graph/render.ts`** — neue private Methode `drawAppRing`, aus
  `drawOrbit` heraus aufgerufen: App-Ring bei `R · APP_RING` zeichnen
  (dezenter Kreis zur Abgrenzung), App-Knoten mit fixen Winkelpositionen,
  `sx/sy` setzen (damit `hitTest` greift). App-Knoten setzen nie `onOrbit`,
  bleiben also automatisch außerhalb der bestehenden Rim-Schleife, und werden
  in `drawAppRing` ohne `+ this.angle` gezeichnet (sonst würde der Ring trotz
  „statischem" Layout mit dem globalen Spin mitrotieren). Icons: Builtins
  über `appIcons.ts`s `getIcon(appType, iconSvg)` + `drawImage`; externe
  Mac-Apps bekommen ein Kürzel/Monogramm aus dem Label (neue Hilfsfunktion
  `monogram(label)`) — mangels Icon-Extraktion (echte App-Icons = spätere
  Verfeinerung). Farben: Akzentfarbe für Builtin-Knoten (wie Skills/Routinen),
  eine pfadstabile Muted-Farbe für User-App-Knoten (`areaColor(path, light)`
  direkt wiederverwendet, kein eigener Farb-Algorithmus).
- **`src/modules/second-brain.svelte`** — App-Knoten in `rebuild()` nach
  `layoutOrbit` ans Modell anhängen (`buildAppNodes(listBuiltinApps(),
  get(userApps), palette)` + `layoutAppRing`), sodass jeder Rebuild
  (Workspace-Refresh, Theme-Wechsel, oder eine `userApps`-Änderung) sie
  mitträgt. Die `userApps`-Reaktivität läuft über ein `userApps.subscribe(()
  => rebuild())` in `onMount` (unsubscribed im Cleanup) — dasselbe
  imperative Store-Subscription-Muster, das die Komponente für den
  `MutationObserver`/das `setInterval` schon nutzt, statt eine `$derived`/
  `$effect`-Kette gegen den Store aufzuziehen. Klick-Verhalten
  (`handleAppClick`):
  - Builtin: `createInstance(type)`; bei bereits platziertem Singleton → `bringToFront`.
  - User-App: Launch via `@tauri-apps/plugin-opener` `openPath(path)` (macOS startet
    `.app`-Bundles; kein neues Rust-Command, kein neues Plugin). Ein
    Fehlschlag (Modul lehnt `createInstance` ab, `.app` wurde verschoben/
    gelöscht) wird als Toast angezeigt, wie bei `ModulePicker`s eigenem
    Fehlerpfad.
  - „+" als DOM-Button, `top: calc(50% - {discR * APP_RING}px)` (12-Uhr-
    Position des App-Rings — `discR` entspricht exakt dem Renderer-eigenen
    `R`, da `view.zoom` in diesem Hintergrund-Canvas nie von seinem
    Default `1` abweicht), verschachtelt in `.brain` mit
    `onclick`-`stopPropagation` (wie `ModulePicker`s inneres `.dialog`-Div),
    öffnet `<AppAddDialog bind:open>`.
  - Der bestehende `onclick`-Handler auf dem `.brain`-Div feuert heute
    unconditional `open(hover)` (→ `open-second-brain`) — bekommt eine Weiche
    ganz oben (`onClick`): ist ein Kontextmenü offen, schließt der Klick nur
    das Menü (Priorität vor allem anderen); sonst routet
    `hover.kind === "app"` auf `handleAppClick` statt auf `open`.
  - Neuer State `menu = $state<{ node: GraphNode; x: number; y: number } | null>(null)`;
    `oncontextmenu` auf `.brain`: Treffer auf `hover.kind === "app" &&
    hover.userApp` → `e.preventDefault()`, `menu` setzen (Position vorab
    über `clampMenuPos` an den Viewport geklemmt); sonst schließt ein
    Rechtsklick ein offenes Menü. `svelte:window onkeydown` schließt `menu`
    bei `Escape` (Muster aus `ModulePicker.svelte`). Rendert bei gesetztem
    `menu` `<AppContextMenu>` **als Geschwister von `.brain`**, nicht darin
    verschachtelt (Begründung siehe die `AppContextMenu.svelte`-Zeile oben);
    `onRemove`-Callback ruft `removeUserApp(menu.node.appPath)` und schließt
    das Menü, `onCancel` schließt nur. Kein Toast — `removeUserApp` ist eine
    rein synchrone Store-Mutation ohne eigenen Fehlerfall, ein echter
    Speicherfehler wird bereits global von `persist.ts`s `flush()` getoastet.
- **`src/modules/AppAddDialog.svelte` (neu)** — rendert `list_installed_apps()`
  mit Suchfeld; ein Button pro Zeile, umschaltend zwischen
  „Hinzufügen"/„Entfernen" je nachdem ob der Pfad schon in `userApps` steht —
  in beiden Zuständen aktiv/klickbar (anders als `ModulePicker`s reines
  `disabled` für platzierte Singletons), **instant, ohne Bestätigung**.
  Lade-Zustand `<p class="muted">Loading…</p>` während des Scans (Konvention
  aus `mail.svelte`/`mail-settings.svelte`), expliziter Leer-Text bei null
  Treffern, ein Hinweis bei `truncated: true` („weitere Apps nicht angezeigt,
  Suchfeld nutzen"). Styling an `ModulePicker` angelehnt.
- **`src/modules/AppContextMenu.svelte` (neu)** — Ein-Zweck-Präsentationskomponente
  (kein generisches Menüsystem): Props `x`, `y`, `label`, Callback-Props
  `onRemove`/`onCancel` (Svelte-5-Konvention dieser Codebase, z. B.
  `CalendarCreateForm.svelte`s `onCancel` — kein `createEventDispatcher`).
  Default-Ansicht ein Eintrag „Entfernen"; nach Klick darauf, in derselben
  Komponente, Text „„{label}" aus dem Ring entfernen?" + Buttons „Entfernen"
  (Klasse `danger`, wie `SecondBrainView.svelte`) / „Abbrechen" (schließt das
  Menü komplett, kein Zurückspringen auf Stufe 1). Styling:
  `--ax-surface-2`/`--ax-border-strong`/`--ax-radius-md`/
  `box-shadow: var(--ax-shadow-pop)` (dieselben Tokens wie `ModulePicker`s
  Dialog), aber `position: fixed`, an den Viewport geklemmt (Klemmung passiert
  im Aufrufer, `second-brain.svelte`, nicht in dieser Komponente selbst),
  `z-index: var(--ax-z-dialog)` (10000 — sicher über allen Kachel-Ebenen:
  `tile-drag:9000`, `staging:9500`, `assistant:9600`). Schließt sich nicht
  selbst; die Elternkomponente steuert Sichtbarkeit. **Wird als Geschwister
  von `second-brain.svelte`s `.brain`-Div gerendert, nie darin verschachtelt**
  — sonst würde ein Klick auf einen ihrer eigenen Buttons zum `.brain`-Div
  hochbubbeln, dessen Klick-Handler dann (mit noch immer auf den
  ursprünglichen Knoten zeigendem `hover`) ein zweites Mal auf denselben
  Knoten reagieren würde, direkt nachdem „Entfernen"/„Abbrechen" schon
  gefeuert hat. `position: fixed` macht die DOM-Verschachtelung für die
  eigentliche Positionierung ohnehin irrelevant.
- **`src/core/devmock.ts`** — Fixtures für `list_installed_apps`.

### Rust (src-tauri)

- **`src-tauri/src/commands.rs`** — eine neue Command
  `list_installed_apps -> { apps: Vec<InstalledApp { path, name }>, truncated: bool }`
  (Muster von `WorkspaceGraph.truncated`, `graph.rs`, statt still abzuschneiden):
  - scannt `/Applications`, `/System/Applications`, `/Users/<user>/Applications`
    nach `*.app`, nur Top-Level (keine Rekursion in gefundene `.app`-Bundles
    hinein — sonst tauchen interne Helper-/Sub-Apps aus `Contents/…` als
    eigene Zeilen auf). Symlinks werden wie normale Einträge behandelt.
  - ein fehlendes/unlesbares Wurzelverzeichnis (v. a. `~/Applications`,
    existiert bei den meisten Nutzern gar nicht) wird übersprungen, nicht als
    Command-Fehler durchgereicht.
  - `name` = Bundle-Ordnername, nur `.app`-Endung abgeschnitten + `.trim()`
    (keine weitere Normalisierung), sortiert, dedupliziert nach Pfad, Cap 300.
  - die Scan-Kernlogik (`scan_apps(roots: &[PathBuf], max: usize)`) nimmt die
    zu durchsuchenden Verzeichnisse als Parameter statt sie hart zu kodieren,
    damit sie mit einem Temp-Dir-Fixture unit-testbar ist; der öffentliche
    Command ruft sie mit den drei echten macOS-Pfaden auf.
  - `~/Applications` wird über die bereits vorhandene Workspace-Dependency
    `home` (`home::home_dir()`, schon von `axiomata-core::paths` genutzt)
    aufgelöst — `home.workspace = true` in `src-tauri/Cargo.toml` ergänzt,
    keine neue externe Crate.
  - Lebt bewusst direkt im `src-tauri`-Crate, nicht in `axiomata-core`
    (obwohl dort sonst die meiste Business-Logik hinter einem dünnen
    Command-Wrapper sitzt) — es gibt schon ein Präzedenzfall dafür
    (`merge_view_update` + dessen Tests, direkt in `commands.rs`), und diese
    Funktion ist eine reine Dashboard-UI-Angelegenheit ohne CLI-Bezug.
- **`src-tauri/src/lib.rs`** — Command registrieren.

### Tests (Vitest)

- `src/graph/appRing.test.ts` — `buildAppNodes`: Builtin-Knoten tragen nie
  `userApp: true`, User-App-Knoten immer, Farbe/Typ/Pfad korrekt durchgereicht,
  pfadstabile Farbe unabhängig vom Anzeigenamen; `layoutAppRing`:
  Winkel-Gruppen links/rechts, „+" frei, erste Position pro Seite am
  nächsten dran, deterministisch, alle Knoten exakt auf `APP_RING`-Radius.
- `src/graph/appIcons.test.ts` — `getIcon` liefert synchron `null` (Laden ist
  immer asynchron), wirft nicht bei wiederholtem Aufruf für denselben Typ.
- `src/core/apps.test.ts` — `listBuiltinApps` exkludiert `background`/`dev`/
  `md-file`, Registry-Reihenfolge bleibt erhalten, `title`/`icon` korrekt
  durchgereicht; `removeUserApp(path)` entfernt genau einen Pfad aus
  `userApps`, andere bleiben unangetastet.

### Tests (Rust)

Sieben Tests für `scan_apps` in `commands.rs`s bestehendem `mod tests`
(Temp-Dir-Fixtures, `unique_temp_dir`-Muster wie die übrigen Tests dort):
Top-Level-Funde sortiert nach Name; ein fehlendes Wurzelverzeichnis wird
übersprungen, ohne die anderen zu beeinträchtigen; keine Rekursion in ein im
Bundle verschachteltes `.app`; Dedup bei doppelt übergebenem Pfad;
Namensbereinigung (nur `.app` abgeschnitten + trim); Cap + `truncated`;
Nicht-`.app`-Einträge und eine `.app`-benannte Datei (kein Verzeichnis)
werden ignoriert.

## Entschieden (vormals offen)

- **Farben der App-Knoten:** Akzentfarbe für Builtins, pfadstabile Muted-Farbe
  für User-Apps (wie `areaColor`) — siehe `src/graph/render.ts` oben.

## Betroffene Dateien

Neu:
- `src/core/apps.ts`
- `src/core/apps.test.ts`
- `src/graph/appIcons.ts`
- `src/graph/appIcons.test.ts`
- `src/graph/appRing.test.ts`
- `src/modules/AppAddDialog.svelte`
- `src/modules/AppContextMenu.svelte`
- `docs/plans/app-ring.md` (diese Datei)

Geändert:
- `src/core/persist.ts` / `src/core/persist.test.ts`
- `src/core/types.ts` (`ModuleDefinition.dev`)
- `src/graph/layout.ts`
- `src/graph/model.ts`
- `src/graph/render.ts`
- `src/graph/Legend.svelte` (`Record<NodeKind, string>`-Vollständigkeit für `"app"`)
- `src/modules/index.ts` (`dev: true` auf den beiden Dummy-Registrierungen)
- `src/modules/second-brain.svelte`
- `src/core/devmock.ts`
- `src-tauri/src/commands.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/Cargo.toml` (`home.workspace = true`)
- `AGENTS.md` (kurzer Hinweis auf den App-Ring / die neue Command)

## Verifikation

- `npm run check` (svelte-check)
- `npm test` (Vitest)
- `cargo test --workspace` (neuer Scan-Unit-Test)
- `cargo clippy --workspace -- -D warnings`
- `cargo fmt --check`
- manuell: `cd apps/dashboard && cargo tauri dev`

## Kurswechsel: Icons doch nicht rasterisiert, sondern Vektor-Glyphen

Nach zwei Fix-Runden (Data-URL-`;utf8,`-Bug, dann dimensionslose-SVG-Fix)
zeigte der Owner per Screenshot: die sieben Builtin-App-Icons im Ring waren
**immer noch** leer — nur der Akzent-Rand-Kreis, kein Icon darin. Statt einer
dritten Runde am selben Bild-Lade-Mechanismus zu debuggen (der sich in der
echten WKWebView nicht verlässlich verifizieren lässt, ohne dass der Owner
jedes Mal selbst testet und Feedback gibt), wurde die gesamte Bild-
Rasterisierung durch denselben handgezeichneten Vektor-Glyph-Mechanismus
ersetzt, der im selben Screenshot bei Mail-/Ordner-Icons bereits nachweislich
zuverlässig rendert (`render.ts`s `drawGlyph`, bisher für Hub/Skill/Routine/
Bereichs-Icons genutzt).

**Was das ändert, gegenüber allem, was oben zu `appIcons.ts`/`iconSvg`/
`getIcon` steht (alles davon: entfernt/überholt):**

- `src/graph/appIcons.ts` + `appIcons.test.ts`: **gelöscht**, komplett.
- `GraphNode.iconSvg` (Feld): **entfernt** — Builtins nutzen jetzt wieder das
  bestehende `glyph`-Feld (bisher nur für Bereichs-Icons dokumentiert, Doku
  entsprechend erweitert).
- `core/apps.ts`s `BuiltinApp`: kein `icon`-Feld mehr (wurde nirgends mehr
  gebraucht).
- **Neu in `model.ts`**: `glyphForModuleType(type: string): string` —
  ordnet jedem Ring-fähigen Builtin-Typ eine `drawGlyph`-Glyphe zu:
  `skills-deck`→`"skill"`, `routines-board`→`"routine"`, `mail`→`"mail"`
  (dieselben Glyphen, die diese Kinds an anderer Stelle im Graphen schon
  nutzen — visuell konsistent), `memory-status`→`"book"`,
  `todo`→`"check"` (neu), `calendar`→`"calendar"` (neu),
  `reminders`→`"list"` (neu). Unbekannte Typen fallen auf `"folder"` zurück
  (wie `glyphForArea`), damit ein künftiges Builtin nie einfach nichts zeigt.
- **Neu in `render.ts`s `drawGlyph`**: drei zusätzliche Vektor-Glyphen-Fälle
  (`"check"`, `"calendar"`, `"list"`), im selben Stil wie die bestehenden
  (`"mail"`, `"tray"`, …).
- `drawAppRing` zeichnet Builtins jetzt über
  `drawGlyph(ctx, n.glyph ?? "folder", x, y, nodeR * 0.62, farbe)` statt über
  `getIcon`/`drawImage`. Die Monogramm-Logik für externe Mac-Apps ist
  unverändert (reines `ctx.fillText`, nie Teil des Problems).
- Tests: `appRing.test.ts`s Builtin-Test prüft jetzt `glyph:
  glyphForModuleType(...)` statt `iconSvg`; `apps.test.ts`s Icon-Assertion
  entfernt.

**Konsequenz für „echte App-Icons = spätere Verfeinerung"**: Diese Aussage
gilt weiterhin, ist jetzt aber ausdrücklich als **eigenständige, spätere
Arbeit** zu verstehen, nicht als „schon fast fertig, nur der Rasterisierungs-
Bug noch" — der Rasterisierungs-Ansatz wurde verworfen, nicht nur repariert.
Ein künftiger Anlauf sollte dafür visuell in der echten App testbar sein
(z. B. über einen Screenshot-Loop mit dem Owner), bevor er als erledigt gilt.