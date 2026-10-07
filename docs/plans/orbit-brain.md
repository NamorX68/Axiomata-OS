# Plan: Orbit und Second Brain zusammenlegen

Stand: **Entscheidungen geklärt am 2026-10-07 (mit dem Owner gegrillt), noch nichts gebaut.** Reihenfolge: erst die
Spotlight-Suche (`docs/plans/spotlight-search.md`, CP1–CP4), dann dieser Umbau in drei kleinen Schritten.

## Ausgangslage

- **Orbit** ist die Hauptsicht: `modules/second-brain.svelte`, die 3D-Wolke des Workspace hinter allen Kacheln (Renderer-Modus
  `orbit`, Radius `ORBIT_FIT` = 0,36 der kürzeren Seite) mit dem App-Ring außen herum (`APP_RING` = 1,18 × Orbit-Radius).
- **2Brain** ist heute eine zweite Vollbild-Ansicht (`shell/SecondBrainView.svelte`, 1179 Zeilen): Rings/Circle/Hex
  (Renderer-Modi `rings`/`hex`, `fit` 0,44 mit Zoom), Suche, Gruppierung, Detailfenster, „Back to the OS". Man erreicht sie
  per Klick auf die Wolke.
- „Dashboard" sagt der Owner nicht mehr: es gibt **zwei Hauptsichten**, Orbit und 2Brain — und die werden eine.

## Entscheidungen (Owner)

| # | Entscheidung |
|---|---|
| B1 | **Es gibt nur noch den Orbit.** Die Vollbild-Ansicht 2Brain (Titel „AXIOMATA SECOND BRAIN", „Back to the OS") entfällt. |
| B2 | Der Orbit sieht beim Start aus wie heute. Ein **Klick in die Wolke ersetzt die Wolke durch das 2Brain-Diagramm**; Kacheln und App-Ring bleiben. Ein **Klick auf freien Hintergrund** oder **Esc** bringt die Wolke zurück. |
| B3 | Das 2Brain-Diagramm bleibt **im Kreis des Orbits** (Radius **0,4**, die Wolke hat 0,36; der App-Ring liegt bei 0,425 — zu eng, dann 0,38) und verdeckt damit keine Kachel mehr als die Wolke heute. **Schwenken und Mausrad-Zoom bleiben**, das Bild wird im Kreis abgeschnitten (Linse). „Fly to"/der Sprung aus Spotlight zoomt den Knoten in die Linse. |
| B4 | Darstellungen im 2Brain: **Rings und Hex. Circle entfällt** (war im Code nur eine Variante von Rings). |
| B5 | **Routinen verschwinden aus dem Graphen**, in beiden Zuständen (Routine-Ring im 2Brain, Routine-Knoten auf dem Orbit-Ring). Ein-/Ausschalten bleibt im Routinen-Board; Spotlight findet Routinen. |
| B6 | **Einstellungen:** die App-Einstellungen (⚙ oben in der Icon-Leiste) bleiben, wie sie sind. Aus dem 2Brain: *Darstellung* (Rings/Hex) und *Gruppierung* (Bereiche/Ordner) kommen als kleine Leiste in den **vorhandenen Bereich unten links** im Orbit (heute: Pfad, ⚙, ×), sichtbar im 2Brain-Zustand. Ein **Schalter für die Bewegung** bleibt dort **in beiden Zuständen**. *Tempo* und *Beschriftungen* wandern in die App-Einstellungen in einen Abschnitt „Ansicht" neben dem Theme. Die Kachel-Einstellungen „Slow spin / Labels" entfallen. |
| B7 | **Suche:** das Suchfeld des 2Brain **entfällt komplett**; es gibt nur noch Spotlight (Pille in der Mitte, ⌘K). „Treffer hell, Rest gedimmt" gibt es nicht mehr. |
| B8 | **Detailfenster** (Bereich/Notiz) bleibt, ist **verschiebbar** und verhält sich wie das schwebende Datei-Fenster (am Kopf ziehen, alle Kanten, Position und Größe gemerkt); es bleibt offen, bis es geschlossen wird. |
| B9 | **Esc-Kette:** zuerst Detailfenster/Auswahl, dann zurück zur Wolke. Ein Klick auf freien Hintergrund geht direkt zur Wolke (Klick und Ziehen trennt eine Bewegungsschwelle, weil Ziehen schwenkt). |
| B10 | Der **App-Ring bleibt**, unverändert. Der Übergang bleibt ein schlichtes Überblenden; ein animierter Zoom ist Sache der späteren UI-Runde. |

## Checkpoints (nach Spotlight)

1. **Aufräumen — gebaut 2026-10-07:** Circle und Routine-Knoten aus Modell, Layout, Renderer, Legende, 2Brain-Detailfenster und Tests gestrichen (B4, B5). Ein gespeichertes `layout: "circle"` fällt auf Rings zurück. Das Glyph `routine` bleibt: der App-Ring nutzt es für das Routinen-Board.
2. **Einstellungen umziehen — gebaut 2026-10-07 (ohne die Darstellungs-Leiste, die erst mit Schritt 3 einen Platz hat):** neuer Store
   `core/brainView.ts` (Bewegung an/aus, Tempo, Beschriftungen, Dateinamen; gespeichert unter `settings.brainView`, die alten Werte des
   2Brain `spin`/`fileNames` werden übernommen); Abschnitt „Orbit & Second Brain" in den App-Einstellungen; der Bewegungsschalter ⏸/▶
   in der Ecke unten links des Orbits (`BackgroundHost`); die Kachel-Einstellungen „Slow spin / Labels" (`second-brain-settings.svelte`)
   und Rotation/File-names im 2Brain-Panel sind weg. **Darstellung (Rings/Hex) und Gruppierung** bleiben bis Schritt 3 im 2Brain-Panel und
   ziehen dann in die Leiste unten links (B6).
3. **Ein Orbit — gebaut 2026-10-07 (erste Fassung, am Mac noch nicht gesehen):** statt die Interaktion in eine neue Komponente zu ziehen,
   wurde `SecondBrainView` selbst zur **Schicht über der Wolke**: kein Vollbild, keine Kopfzeile, kein „Back to the OS"; der Graph sitzt in
   einer **Scheibe** (Radius `BRAIN_DISC` = 0,4 der kürzeren Seite des Orbit-Canvas, deren Mitte und Größe das Orbit-Modul über
   `orbitFrame` meldet) und deckt die Wolke ab; ob die Kacheln davor stehen, siehe Offenes. Zoom/Schwenken sind im Kreis abgeschnitten. Das Suchfeld samt Suchlogik ist raus (Spotlight); die Leiste (Layout, Gruppierung, Reset, Reload, ?) sitzt unten
   links neben den Ecken-Knöpfen; das **Detailfenster** ist verschiebbar und an den vier Kanten in der Größe änderbar, Rechteck gemerkt
   (`shell/floatingRect.ts`, Einstellung `brainDetail`). Esc: erst Auswahl, dann zurück zur Wolke; ein Klick auf den freien Orbit-Hintergrund
   (Bus-Ereignis `close-second-brain`) ebenfalls. Die alte Idee dieses Schritts: die Interaktion aus `SecondBrainView` (Schwenken, Zoom, Hover, Auswahl, Detailfenster) in eine gemeinsame
   Komponente ziehen und im Orbit-Widget einhängen; Klick in die Wolke/auf Hintergrund/Esc (B1–B3, B7–B9); die alte Vollbild-Ansicht
   samt `open-second-brain`-Weg entfernen — Spotlight-Einträge „Graph durchsuchen"/„im 2Brain zeigen" zeigen darauf.

## Offenes / Risiken

- **App-Ring-Symbole unter der Scheibe (Mac-Test 2026-10-07):** im Brain-Zustand waren nur Terminal, „+" und die Symbole auf Höhe der Mitte
  klickbar, alle auf den Diagonalen nicht (offene Hand). **Ursache:** WebKit gibt einem Element mit `border-radius` + `overflow: hidden` für
  Mausereignisse die ganze *rechteckige* Box; die Ecken des Quadrats (±0,4 × Seite) deckten die diagonalen Symbole des Rings (bei 0,425) ab.
  Chromium (Browser-Mock) nimmt den Kreis, deshalb zeigte der Mock nichts. Mit Radius 0,3 war das Quadrat kleiner als der Ring — das bestätigte es.
  **Lösung:** `clip-path: circle(50%)` an der Scheibe (macht den Kreis zur Form auch für den Treffertest) bei Radius 0,4. Davor waren zwei
  falsche Wege nötig: eine gemessene Orbit-Geometrie (veraltet) und ein Treffertest-Umweg (vom Owner verworfen). Die Scheibe wird in den Container des
  Orbits verschoben (`[data-background]`, Aktion `inOrbitHost`) und per CSS mittig gelegt, ihre Größe in `cqmin` (`container-type: size` am Host):
  derselbe Mittelpunkt und dieselbe Bezugsgröße wie die Wolke. Ohne Orbit bleibt sie in der Fenstermitte (`vmin`). Die Legende liegt im „?";
  der Ring mit den Bereichszahlen ist weg. **Am Mac mit 0,4 gegenprüfen** (clip-path-Treffertest in WebKit ungeprüft).
- **Scheibe und Kacheln:** im Mock mit einer ToDo-Kachel geprüft (die Kachel liegt vor der Scheibe); am Mac gegenprüfen.
- Die Leiste unten links kann in schmalen Fenstern unter der Chat-Leiste liegen.
- `/brain ? <Suche>` und die Modul-Aktion `search` öffnen das 2Brain jetzt ohne Suchtext (die Suche ist Spotlight).

- Der **größte Brocken** ist CP 3: die Interaktion steckt heute in einer 1179-Zeilen-Komponente, und das Orbit-Widget liegt
  „full-bleed" hinter den Kacheln (Klicks müssen die Kachel bevorzugen).
- Ob 0,4 neben dem App-Ring (0,425) eng wird, sieht man erst am Bild; die Zahl ist eine benannte Konstante.
- **UI/UX-Runde danach (app-weit):** Design-Kritik mit den `design:*`-Skills und das **Tastenkürzel-Schema** (⌘K, ⌘, …),
  Editor ausgenommen; ein animierter Zoom.
