# App-weite UI/UX-Runde — „Glut + Hex“

Stand: 2026-10-11, **gegrillt (Runden 1–2, alle Empfehlungen vom Owner angenommen)**. Anlass (Owner): „Die ganze App soll in sich stimmig sein und das
Design soll State of the Art sein, gerne mit optischen Highlights, die der App einen eigenen, unverwechselbaren
Wiedererkennungswert bescheren.“ Vorgänger: `docs/plans/editor-look.md` (K1–K14, gilt weiter), der Design-Pass vom
2026-10-07/08 (Legende, Live-Puls, Studio-Kopfzeile).

## 1. Befund (Screenshots 2026-10-11, 2560×1080, Graphite + Paper + Catppuccin Latte, Browser-Mock)

1. **Flache Schrift-Hierarchie.** ~400 der ~430 `font-size` in Komponenten sind `xs`/`sm` (12,5/13 px). Daneben
   riesige gesperrte Versal-Titel in Kacheln (CALENDAR) und winzige in Panels (KANBAN, SETTINGS, AGENT, FILE). Mono
   steht 84-mal, oft ohne Code (Uhrzeiten, „in 2 h“, Pfade, Zähler).
2. **Akzent-Inflation.** Orange markiert Titel-Icons, „NEXT“, „TOMORROW“, „STALE“, jeden Skill-Knopf, Agent-Karten-
   Ränder, den vollen Balken „New plan …“, den gewählten Plan. Was überall leuchtet, bedeutet nichts.
3. **Drei Chrome-Dialekte.** Schwebende Panels (Kanban, Datei, Chat, Settings, Modul-Picker) haben je eigene
   Kopfzeilen-Höhen, Titel und Schließknöpfe; das Studio hat seine eigene Sprache; die Bereichsköpfe im Flow
   („Planning“, „Agents“, „Flow view“) sind Pillen wie Knöpfe.
4. **Keine Tiefe.** Panels und Kacheln liegen tonal fast gleich auf dem Hex-Muster; Schatten tragen im Dunkeln nicht.
5. **Kopfleiste.** Sechs unbeschriftete 14-px-Icons, „DEV | AXIOMATA-WORKSPACE“, der Theme-Knopf schaltet blind
   durch 13 Themes.
6. **Hinweise als Prosa.** 8–10-zeilige orange gerahmte Textblöcke in den Seitenleisten Agents und Run.
7. **Leere Zustände** sind zentrierter grauer Satz (Studio-Canvas, „No project“, Outline).
8. **Helle Themes.** Paper-Serif taucht im Studio zufällig auf („Studio“, „Plan #1“, „Proposals“); deaktivierte
   Primärknöpfe werden zu blassen Flecken (Commit); Latte zeigt, dass die Akzent-Inflation themenübergreifend ist.
9. **Terminal** (Screenshot des Owners aus der echten App): Inhalt gut lesbar, aber der Titel „TERMINAL“ samt Icon steht
   ~12 px weiter rechts als der Terminaltext — gegen die Regel „Titel bündig mit dem Inhalt“
   (`axiomata-os-owner-ui-feedback`); Versal-Titel wie überall.

## 2. Entscheidungen (Owner, 2026-10-11)

- **U1 — Signatur „Glut + Hex“.** *Glut:* Der Akzent bedeutet nur noch „hier lebt/arbeitet etwas oder braucht
  dich“ — laufende Agenten, laufende Skills/Routinen, wartende Fragen, die eine Primäraktion, die Auswahl. Dort
  glimmt er (weiches Leuchten, langsamer Atem), sonst ist die Oberfläche ruhig-neutral. *Hex:* Das Hexagon des Logos
  wird die Systemform — Avatare (OC/CC), Status-Marken, leere Zustände, Fokus-/Auswahlzeichen, der Orbit.
- **U2 — Tiefe „deutlich“.** Layouts bleiben (Orbit, Flow-Aufteilung, Kachel-Regeln aus
  `axiomata-os-owner-ui-feedback`); Chrome, Schrift, Kopfleiste, Reiter, Bereichsköpfe und Akzent werden sichtbar
  neu. **K7 (Pillen-Reiter) und K14 (farbige Reiter) aus `editor-look.md` dürfen geändert werden, wenn es Sinn
  macht** (Owner, 2026-10-11).
- **U3 — Eingebettete UI-Schrift, wählbar: Geist, Inter, IBM Plex Sans** (alle OFL, offline, im Repo, mit
  Lizenzhinweis wie Grammatiken/Icons; Einstellung unter Appearance). Code und Terminal behalten ihre eigenen Schriften.
- **U4 — Orbit-Ring:** links weiterhin die internen Apps, rechts die Apps außerhalb (bleibt so).
- **U5 — Hex bleibt Systemform.**
- **U6 — Umfang: alles** (Owner, 2026-10-11: „ich nehme wirklich alles … auch deine nicht prototypisierten“). Alle
  Prototypen aus `ux0-mockup.html` und `ux0-signature.html` sowie der ganze Ideenkatalog (F7/F8) gehören in die Runde.
- **U7 — 2Brain behält sein Funkeln.** Der heutige Effekt (`graph/render.ts`: jede Hex-Zelle atmet mit eigener
  Phase, `0.75 + 0.25·sin(1.7t + phase)`) wirkt wie gekippte Metallplättchen und bleibt — auf der Territorienkarte,
  dazu als Steigerung „Metall“ (wanderndes Licht + Lichtkante je Plättchen). **Auch die Orbit-Waben-Kugel
  funkelt** (Owner); dort beleuchtet „Metall“ jede Wabe nach ihrer echten Neigung mit Glanzlicht. Welche Stufe Vorgabe
  ist: F9.
- **Nach der Runde, nicht darin:** #Tags und [[Wikilinks]] mit Sprung ins verlinkte Dokument wie in Obsidian
  (Board #1, Karte #75).

## 3. Bausteine (Vorschlag, zum Grillen)

- **Schrift-Rollen** statt nur Größen: `caption` (Marken/Zähler), `body`, `label` (Kapitälchen-Stil, eine Spur statt
  vieler), `title` (Panel-/Kacheltitel, *ein* Stil überall), `heading`, `display` (große Zahlen als Helden: „2 open“,
  „42 files“). Mono nur für Code, Pfade, Ids, Befehle.
- **Akzent-Budget** als Tokens: `--ax-accent` bleibt, dazu `--ax-glow-*` (Glut: Farbe, Radius, Atem-Dauer) und eine
  neutrale Stufe für „ausgewählt, aber nicht lebendig“. Regel im Token-Kopf festgeschrieben.
- **Ebenen**: `--ax-elev-0..3` (Grund, Kachel, Panel, Popover) mit Tonwert + 1-px-Lichtkante oben (Glas) statt nur
  Schatten — trägt im Dunkeln wie im Hellen.
- **Komponenten-Familie** in `ui/`: `Panel` (Kopfzeile, Titel, Aktionen, Schließen, ⌘W/Esc-Vertrag), `SectionHead`,
  `Notice` (eine Zeile + Aktion, aufklappbar), `EmptyState` (Hex-Motiv + Satz + Primäraktion), `HexAvatar`,
  `StatusMark`, `Glow`-Action.
- **Kopfleiste → Dock**: Logo links/mittig, beschriftete oder mit Tooltip+Kürzel versehene Ziele, ⌘K-Hinweis,
  Theme-Knopf öffnet eine Auswahl statt blind zu schalten.

## 4. Checkpoints (Vorschlag, nach U6 neu geschnitten)

- **UX0 — Richtung festmachen:** Mockup-Seite (Vorher/Nachher: Kopfleiste, Kacheln, Kanban, Studio-Flow, Reiter;
  Graphite + Paper, fünf Schrift-Kandidaten umschaltbar),
  Schriftwahl, dann Grillen dieses Plans.
- **UX1 — Fundament:** Tokens (Schrift-Rollen, Ebenen, Glut, Akzent-Regel) in allen 13 Themes; Geist/Inter/Plex
  eingebettet + Einstellung; Bewegungs-Tokens (Federn, „Bewegung aus“ respektieren).
- **UX2 — Chrome:** Komponenten-Familie (`Panel`, `SectionHead`, `Notice`, `EmptyState`, `HexAvatar`, `StatusMark`,
  `glow`-Action); alle schwebenden Panels, Studio-Bereichsköpfe, Reiter (F2) darauf; Panels öffnen aus ihrem Ursprung.
- **UX3 — Dashboard:** Dock statt Kopfleiste (+ Herzschlag-Leiste, Kosten-Tankanzeige), Kachel-Titel bündig,
  Kalender als Tagesbogen, Routinen-Zündschnur, Skill-Glut, denkende Assistent-Leiste mit Hex-Chips, generative leere
  Leinwand, Licht unter dem Zeiger, Tageszeit-Licht.
- **UX4 — Orbit:** Waben-Kugel (Goldberg) mit Glut-Alterung, Zeitreise, Agenten-Funken (Backend-Ereignisse vorher
  klären); Ring links intern / rechts extern; Start-Zündung.
- **UX5 — 2Brain:** Territorienkarte im Linsenkreis mit Funkeln/Metall, Grenzen, Namen, Straßen, semantischem Zoom,
  Zeitreise; Detailfenster bleibt.
- **UX6 — Studio, Flow, Kanban:** Hex-Avatare, Glut an Karten und Agenten, Flow-Glutpulse, Erledigt-Funken, Hinweise
  einzeilig, Projektfarbe als Waben-Ton, Agent-Cursor mit Hex-Fahne im Editor (eigener Unterschritt, größter Brocken).
- **UX7 — Spotlight (⌘K mit „Ask Axiomata“), Chat, Einstellungen (Theme-Wahl als Live-Vorschau), Terminal,
  Fokus-Modus, Töne (aus per Vorgabe);** Gegenprobe in allen hellen Themes.

Je Checkpoint: Screenshots hell + dunkel, Abnahme am Mac, Commit, installierte App neu bauen.

**Stand UX2 (2026-10-11, gebaut, Abnahme offen):** Familie in `ui/`: `PanelHead` (44 px, Titelrolle, ⌘W-Taste
sichtbar), `SectionHead`, `Notice` (eine Zeile + Aktion, Gründe aufklappbar), `EmptyState` (Hex + Glutkern, glimmt nur
mit Aktion), `HexAvatar`, `StatusMark` (+ `status.ts`: eine Ton-Sprache), `glow`-Action, `fromOrigin`-Transition
(`core/origin.ts`: der Klick der letzten 1,5 s, sonst Bildschirmmitte; Full Feder, Calm kurz, Off nur Blende). Alle
schwebenden Panels (`Window`, `StagingPanel`) auf `--ax-elev-2` + Lichtkante + `PanelHead`; ⌘W schließt jetzt auch
Dialoge (`core/closeStack.ts`). Studio: Reiter nach G4 (Einstellung „Reiter tönen“ entfernt), Flow-Bereiche als
Kapitälchen (`isAreaTab`), Seitenleisten-Köpfe als `SectionHead`, die drei Vertrauens-Hinweise (Rollen, tasks.json,
debug.json) einzeilig, leere Zustände mit Hex. „Ausgewählt“ ist neutral (`--ax-selected-bg`): Modus-Umschalter,
`IconButton pressed`, Einstellungs-Reiter und -Abschnitte; der Git-Zähler ist grau, nur der Agenten-Zähler glimmt.
Bewusst später: Agent-Karten-Rahmen, „New plan“-Balken, Kanban-Karten (UX6), Kachel-Titel (UX3).

**Stand UX3 (2026-10-11/12, gebaut, Abnahme läuft):** Dock (`shell/Dock.svelte`, Suche nur als Lupe — Owner) mit
Theme-Auswahl, Herzschlag-Leiste + Kosten-Tank (`core/heartbeat.ts`; eine Karte glüht nur mit offener Sitzung laut
`open_card_sessions`, beim Überfahren eine Liste aller Einträge), Kacheltitel (G6), Skills-Glut, Routinen-Zündschnur
(`core/fuse.ts`), Kalender-Tagesbogen (`core/dayArc.ts`, ersetzt „analog“, sitzt neben dem Monat, Stunden 6/22), Licht
unter dem Zeiger + Tageslicht (`shell/Ambient.svelte`, `core/effects.ts`, Appearance → Light), denkende Assistent-Leiste
(umlaufende Glut, Modul-Aktionen des Agenten als Hex-Chips — nur die über die Bridge sichtbaren), generative leere
Leinwand (`canvas/EmptyCanvas.svelte`). Offen: Herzschlag fragt alle Boards inkl. Archiv ab (später eine Backend-Abfrage
„Sitzungen heute“); Terminal-Titel-Versatz in der echten App prüfen.

**Stand UX4 (2026-10-12, abgenommen):** Waben-Kugel (`graph/hive.ts`), Glut-Alterung, Schimmer, Zeitreise, Vault-Watcher
+ Funken, Start-Zündung. Mac-Feedback eingearbeitet: Waben waren kaum wählbar (Trefferradius kam vom alten Punkt, und
die Ring-Schleife überschrieb die Position frischer Notizen — Mails nie wählbar) → eigener Waben-Treffertest
(`nearestCell`, `GraphRenderer.hitCell`); Kugel mit gehaltener Maustaste drehen und neigen (`dragTurn`, Nachlauf nur
bei Motion Full, der Ring bleibt stehen); Zündung länger (~2,5 s) und kräftiger: Funken fliegen auf die Ecken, Kern
zündet mit Druckwelle und Aufleuchten, der Name zieht sich zusammen, die Marke öffnet sich zur App. Auf Wunsch danach
noch 1,5 s länger (~4 s, jede Stufe gestreckt).

**Stand UX5 (2026-10-12, gebaut, Abnahme offen):** Layout „Map“ ersetzt „Hex“ (G16) und ist die Vorgabe; ein altes
„hex“ in den Einstellungen wird zur Karte. `graph/territory.ts`: jede Area (bzw. jeder Ordner) ein zusammenhängendes
Land auf einem axialen Hexgitter — das größte in der Mitte, die übrigen auf einem Ring darum, je über einen Bogen so
breit wie ihr Anteil; sie wachsen Feld für Feld (immer das Land, das seiner Größe am weitesten hinterher ist, ins freie
Nachbarfeld nächst seinem Kern) und **in Geburtsreihenfolge**: die ältesten Notizen im Herzen, die jüngsten an der
Küste — in der Zeitreise wachsen die Länder als Inseln und dann zu einem Kontinent zusammen. `layoutMap` skaliert die
Karte bis `MAP_EDGE` in die Linse; Skills als Symbolring außen (`MAP_SKILLS`), der Hub fällt weg, ein Area-Knoten sitzt
in der Mitte seines Landes (dort steht sein Name; Klick darauf = Area-Detail wie bisher). Zeichnen (`drawMap`): Felder
mit Glut-Alterung (nur halbe Abkühlung, damit jedes Land seine Farbe behält) und Schimmer (Metall: wanderndes Licht +
Lichtkante oben je Plättchen; Calm/Off: stehendes Licht), Grenzen in Landesfarbe, wo Länder sich treffen oder das Land
endet. Semantischer Zoom: weit draußen Ländernamen mit Notizzahl (bis Zoom 2,8), ab 1,3 Links als gestrichelte Straßen
(die der überfahrenen/gewählten Notiz immer, als Glut), ab 2,4 trägt jedes Feld seinen Namen. Zoom per Mausrad jetzt
**zum Zeiger hin** (auch in Rings). Zeitreise: ein Store `travelAt` (`core/brainView.ts`) für Orbit und Karte; der
Uhr-Knopf des Orbits bleibt auch über dem 2Brain sichtbar, ohne Orbit bringt die Karte ihren eigenen mit. Detailfenster
unverändert. `docs/architecture.md` wird am Ende der Runde nachgezogen.

**Stand UX6a (2026-10-12, gebaut, Abnahme offen):** Eine Abfrage der offenen Kartensitzungen für die ganze App
(`core/studioActivity.ts` `watchActivity`, gehalten vom immer sichtbaren Dock): `studioActivity` (Zahl) und neu
`studioCards` (welche Karten). Orbit-Puls jetzt Glut-Orange (G22; `activityColor`/`studioProject` entfallen).
Kanban: Karten mit offener Sitzung glimmen (`use:glow`, Kartenregel `:global([data-ax-glow])`, weil die eigene
Schattenregel die globale überstimmt); Projektkante G19 als kurze 2-px-Linie oben in der Farbe des Projekts, in dem
der Plan der Karte läuft (Karte → Plan → Projekt → `accent_slot`); Erledigt-Moment G18 (`core/doneMoments.ts`): eine
Karte, die in eine Done-Spalte kommt (egal durch wen), zerfällt bei Motion Full in neun Hex-Funken, die in den Zähler
der Spalte fliegen (Zähler pulst), sonst glimmt sie kurz; wer das Brett neu öffnet, sieht die seit dem letzten Blick
erledigten Karten einmal nachglimmen (`kanbanSeen` in den Einstellungen). Mock: Karte #4 hat eine offene Sitzung, der
Plan ein Projekt mit Farbe. Karten-Look selbst unverändert gelassen.

**Stand UX6b (2026-10-12, gebaut, Abnahme offen):** Agenten-Karten (`ide/AgentsPanel.svelte`) als Platte mit
Lichtkante; der dicke Harness-Rand links entfällt (die Harness-Farbe steht im Avatar-Ring), eine arbeitende Karte
glimmt und atmet, eine wartende bekommt den Warnrahmen. Flow-Graph (`ide/panes/GraphPane.svelte`): Knoten mit
laufender Sitzung (Arbeiter auf der Karte, Prüfer auf ihrer Review-Stufe) bekommen eine Glut-Kontur, die atmet (nur die
Kontur, nicht die Fläche), ihr Lebenspunkt ist Glut statt Grün; auf jeder Kante in einen solchen Knoten läuft ein
Glutpuls (`pathLength` + Strichversatz), bei Calm/Off steht er als stille Glutlinie; geprüfte/fertige Knoten kühlen ab
(Band und Titel gedämpft). Projektmarke am Projekteintrag als Hex statt Punkt (Studio-Kopf, Agentenpane und Team trugen
die Projektfarbe schon seit #70). Planungsspalte: „New plan …“ neutral mit Plus statt orangener Balken, der offene Plan
neutral ausgewählt (Akzentregel). Hinweise im Studio waren schon einzeilig (UX2). Eigene Entscheidung: Avatar-Ring
bleibt Harness-Farbe (G19 nennt den Avatar-Rand für die Projektfarbe — innerhalb eines Projekts sagt sie aber nichts,
und die Harness-Unterscheidung CC/OC ginge verloren).

**Stand UX6c (2026-10-12, gebaut, Abnahme offen):** Agent im Editor (G8), ehrlich statt gespielt. Lädt eine saubere
offene Datei still neu, weil sie sich auf der Platte geändert hat (`fileapp/session.ts` `onExternalChange`), hält die
Sitzung die Spur fest: die Zeilen des neuen Texts, die neu oder anders sind (`editor/trail.ts` `trailOf`, Zeilen-Diff
über den vorhandenen Myers; eine reine Löschung markiert die Zeile, die jetzt an der Lücke steht) — nicht für Dateien
im leichten Modus (über 2 MiB). Wessen Spur (`fileapp/trailAuthor.ts`): im Worktree `worktree:<id>` dessen Agent; im
Projektordner `project:<id>` nur der eine Agent dieses Projekts ohne eigenen Worktree, der gerade arbeitet — sonst
niemand (eine eigene Änderung des Owners anderswo bekommt nie eine Agenten-Fahne). Die Editorfläche lässt die Zeilen
aufglühen und in 2,6 s abkühlen (bei Motion Off stehen sie und gehen dann) und setzt an das Ende der letzten Zeile die
Fahne: Hex-Avatar des Harness + Name, 8 s, dann verblasst sie; auf einer eingeklappten Zeile keine Fahne. Mock:
`__ax.mockExternalWrite("project:1", "notes.txt", …)` zeigt es mit „OC Builder“.

**UX7 geschnitten (2026-10-12):** 7a ⌘K „Ask Axiomata“ + Fokus-Modus → 7b Theme-Wahl als Kärtchen mit echtem
Mini-Ausschnitt (G21) + Einstellungs-Look → 7c Chat und Terminal → 7d Töne (aus per Vorgabe) + Gegenprobe helle Themes.

**Stand UX7a (2026-10-12, gebaut, Abnahme offen):** ⌘K (`shell/Spotlight.svelte`): unter den Treffern immer die
Zeile „Ask Axiomata“ mit glimmender Hex-Marke und der Frage; ohne Treffer ist sie die einzige und gewählt — Return
schickt die Frage sofort an den Chat (`core/chat.ts` `send`, der Chat öffnet sich; läuft schon eine Antwort, ein
Hinweis). Gewählte Zeile neutral (Akzentregel); Gruppennamen englisch. Fokus-Modus (G13, `core/focusMode.ts`):
⌘⇧. bzw. Strg+⇧+. (`core/globalKeys.ts` „focus“) setzt `data-focus="on"` am Dokument; `styles.css`: Kacheln ohne
Fokus und der Orbit-Hintergrund werden unscharf und blass (die 2Brain-Linse und alle Panels darüber bleiben), Glut,
Atem und umlaufende Glut stehen still; das Umgebungslicht geht (`shell/Ambient.svelte`); im Dock ein neutraler Chip
„Focus“, der ihn auch per Klick beendet.

**Stand UX7b (2026-10-12, gebaut, Abnahme offen):** Theme-Wahl als Kärtchen mit echtem Mini-Ausschnitt (G21):
`ui/ThemePreview.svelte` zeichnet eine kleine Szene — Dock-Pille mit glimmender Hex-Marke, eine Kachel, eine
arbeitende Karte mit Glut und Hex-Avatar — im jeweiligen Theme, indem sie `data-theme` auf sich selbst setzt (die
Themes sind auf `[data-theme]` gescoped). Nur Grund-Tokens, die jedes Theme selbst setzt (die abgeleiteten aus
`tokens.css` werden auf `:root` im aktiven Theme berechnet); die Glut wird mit Spezifität 0 aus dem Akzent abgeleitet,
sodass ein Theme mit eigener Glut gewinnt (G2). In den Einstellungen und im Dock-Popup (jetzt drei Spalten, scrollt ab
70 vh); das aktive Theme neutral markiert; Darüberfahren färbt nichts um, ein Klick wechselt.

## 5. Gegrillt (2026-10-11)

Runde 1 (F1–F6, F9 + Stillschweigendes):

- **G1 Schrift:** Vorgabe **Geist**; Inter und IBM Plex Sans wählbar. **Paper** behält die Serif nur für die
  Titel-Rolle (Panel-/Kacheltitel, Display-Zahlen), nie im Studio.
- **G2 Glut in fremden Paletten:** Die Glut ist immer die Orange-Familie der Palette (Catppuccin Peach, Tokyo Night
  Orange, GitHub Orange); Auswahl und Links bleiben in der Palettenfarbe.
- **G3 Bewegung:** App-weite Einstellung **Motion: Full / Calm / Off** (Vorgabe Full), beachtet macOS „Bewegung
  reduzieren“; `brainView.motion` geht darin auf. *Calm:* Glut glimmt still, keine Funken, keine Zündung. *Off:* wie
  Calm, dazu drehen sich Kugel und Karte nicht von selbst. Glutstärke wie im Mockup, nie flächig.
- **G4 Reiter:** wie im Mockup — ruhige Leiste, aktiver Reiter = Editorfläche, **farbiges Datei-Icon** (LK4-Sätze)
  statt Pillen-Tönung, Agent-Reiter mit Hex-Avatar + Glut, Bereichsköpfe als Kapitälchen. Die Einstellung „Reiter
  tönen“ (K14) entfällt.
- **G5 Dock:** beschriftet, oben mittig.
- **G6 Kacheltitel:** einheitlicher Titelstil (Satzschreibung, halbfett) statt Versal-Sperrung; bündig mit dem Inhalt.
- **G7 Schimmer:** Vorgabe **Metall**, wählbar Ruhig / Funkeln / Metall (unter „Orbit & Second Brain“); bei Motion
  Calm/Off steht das Licht still.
- **G8 Agent im Editor, ehrlich:** Ändert ein Agent eine offene Datei, glimmen die geänderten Zeilen kurz nach (Diff
  zum vorigen Stand), an der letzten Änderung steht die Hex-Fahne des Agenten (aus Worktree/Sitzung). Kein gespieltes
  Tippen.
- **G9 Vault-Watcher:** neuer schlanker, entprellter Watcher über den ganzen Workspace im Backend (Rust, `notify`,
  rekursiv) — meldet Datei + Zeit; speist Agenten-Funken (Zuordnung zum gerade laufenden Skill/Agenten, sonst vom
  Hub) und die Glut-Alterung live.
- **G10 Vorgaben der Effekte:** Start-Zündung bei jedem Kaltstart, Töne aus, Tageszeit-Licht an (sehr subtil), Licht
  unter dem Zeiger an; alles einzeln abschaltbar unter Appearance.
- **G11 Kalender:** Der Tagesbogen ersetzt die Analoguhr-Option; Digital bleibt.
- **G12 Arbeitsweise:** eigener Worktree (der `cargo tauri dev` des Owners läuft im Hauptbaum); je Checkpoint ein
  Commit (kleine dürfen gebündelt werden), vorher ein schlanker Sonnet-Review; danach App neu installieren, Abnahme am
  Mac, gepusht erst nach Abnahme.

Runde 2:

- **G13 Fokus-Modus:** ⌘⇧. (⌘. gehört im Editor den Code-Aktionen).
- **G14 Herzschlag-Leiste:** Funken für Skill- und Routine-Läufe, Agenten-Sitzungen, Karten nach Review/Done (keine
  Chat-Runden); Zeitraum heute; Länge = Dauer, Fehlschlag glüht rot; Klick öffnet Lauf/Karte.
- **G15 Kosten-Tank:** im Dock neben dem Herzschlag, Hex-Segmente gegen `daily_usd_cap`; ohne Limit nur der Betrag;
  ab 80 % rote Glut.
- **G16 2Brain-Layouts:** Die Territorienkarte ersetzt „Hex“; „Rings“ bleibt. Länder folgen der Gruppierung
  (Areas/Folders).
- **G17 Zeitreise-Datum:** das früheste aus Frontmatter `created`, Birthtime und mtime. Die Import-Schwemme
  (3. September, Obsidian-Import) erscheint ehrlich als „Gründungstag“ in einem Schwall.
- **G18 Erledigt-Funken:** wenn eine Karte in eine Done-Spalte kommt, egal durch wen — aber nur bei sichtbarem Kanban;
  sonst glimmt sie beim nächsten Öffnen einmal nach.
- **G19 Projektfarbe:** gewählt im Studio-Projektmenü (zwölf Farbrad-Töne oder „keine“ = Vorgabe); sichtbar als Ton
  der Hex-Marke am Projekteintrag, Studio-Kopf, Agent-Avatar-Rand und als 2-px-Kante an den Karten des Projekts. Der
  Orbit-Puls bleibt Glut-Orange.
- **G20 „Ask Axiomata“ in ⌘K:** Enter schickt sofort ab und öffnet den Chat (Kosten über den Tagesdeckel begrenzt).
- **G21 Theme-Wahl:** jedes Kärtchen zeigt einen kleinen echten App-Ausschnitt (Kachel, Karte, Glut); kein
  Umfärben der ganzen App beim Darüberfahren; Klick wechselt.

Runde 3 (2026-10-12, vor UX6):

- **G22 Orbit-Puls:** Glut-Orange wie G19 — die Projektfarbe aus Karte #70 weicht dort; sie zeigt sich nur an
  Hex-Marken, Avatar-Rändern und Kartenkanten.
- **G23 Schnitt UX6:** UX6a Kanban (Glut an Karten mit offener Sitzung, Erledigt-Funken G18, Projektkante G19,
  Karten-Look) → UX6b Studio/Flow (Hex-Avatare mit Glut, Glutpulse im Flow-Graph, Projektton der Hex-Marken, Hinweise
  einzeilig, „New plan“-Balken, Agent-Karten-Rahmen) → UX6c Agent im Editor (G8); je Commit + Mac-Abnahme.

Von mir ohne eigene Frage festgelegt (Einspruch jederzeit): Das Dock blendet unter ~1500 px Fensterbreite die
Beschriftungen aus (Icons + Tooltip mit Kürzel); Glut in hellen Themes ohne additives Leuchten (Ton + Schatten statt
Schein); Töne werden per WebAudio erzeugt (keine Audiodateien); die Goldberg-Kugel hat mehr Waben als Notizen — die
übrigen bleiben als dunkle, „unbesiedelte“ Waben stehen; Panels ohne bekannten Ursprung (Tastatur) öffnen aus der
Bildschirmmitte.

## 6. Fragen-Archiv (vor dem Grillen)

- **F1** Schrift: welche ist die Vorgabe der drei, und behält Paper die Serif für Überschriften (nach welcher Regel)?
- **F2** Reiter: Vorschlag im Mockup — Dokument-Reiter als ruhige Leiste (aktiver Reiter = Editorfläche), Sprachfarbe
  nur als Punkt/Icon statt Pillen-Tönung (ersetzt K7/K14), Agent-Reiter mit Hex-Avatar + Glut, Bereichsköpfe
  („Planning“, „Agents“, „Flow view“) als Kapitälchen-Überschriften. Passt das, oder Teile davon behalten?
- **F3** Wie stark darf die Glut sein, und soll sie mit `brainView.motion` (Bewegung aus) ganz still werden?
- **F4** Catppuccin/Tokyo Night/GitHub haben keinen Orange-Akzent — bleibt das so (Paletten-Treue) oder gilt die
  „überall orange“-Regel aus `axiomata-os-theme-wishes`?
- **F5** Kopfleiste: Beschriftungen sichtbar, oder Icons mit Tooltip + Kürzel? Bleibt sie oben mittig?
- **F6** Kachel-Titel: bleibt die Versal-Sperrung als Dashboard-Signatur, nur einheitlich?
- **F9** 2Brain-Schimmer: Vorgabe „Funkeln (heute)“ oder „Metall“, und wird er wählbar?
- **F7** (entschieden durch U6: alles) Wolke und 2Brain neu (Owner: „was Ausgefallenes“, „trau dich auch bei allem anderen“). Prototypen
  (Scratchpad `ux0-signature.html`, 2026-10-11): **W1** Waben-Kugel (Notiz = Wabe, Bereich = Landmasse; im echten Bau
  als lückenlos kachelnde Goldberg-Kugel), **W2** Glut-Alterung (heute angefasst glüht, Ruhendes kühlt zu Asche),
  **W3** Agenten-Funken vom Ring-Icon zur Wabe (braucht Datei-Ereignisse aus dem Backend — prüfen), **B1**
  Territorienkarte im Linsenkreis (Länder aus Hexfeldern, Grenzen, Namen, Links als Straßen, semantischer Zoom),
  **B2** Zeitreise-Regler. Welche davon?
- **F8** (entschieden durch U6: alles) Weitere Signatur-Momente (prototypisiert): Zündschnur für die nächste Routine, denkende Assistent-Leiste mit
  Hex-Chips für Werkzeug-Aufrufe, Glutpulse auf Flow-Kanten, Start-Zündung (nur Kaltstart, überspringbar),
  Erledigt-Funken im Kanban, Agent-Cursor mit Hex-Fahne im Editor. Nur als Katalog: Herzschlag-Leiste im Dock, Licht
  unter dem Zeiger auf dem Hex-Muster, Tageszeit-Licht, Panels aus dem Ursprung, ⌘K mit „Ask Axiomata“, Fokus-Modus,
  Kosten-Tankanzeige, Projektfarbe als Waben-Ton, Töne (aus), generative leere Leinwand, Kalender als Tagesbogen,
  Theme-Wahl als Live-Vorschau. Welche kommen in die Runde, welche später?
