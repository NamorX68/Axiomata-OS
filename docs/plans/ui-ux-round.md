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
