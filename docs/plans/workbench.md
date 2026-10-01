# Plan: Eine Arbeitsfläche statt Editor und IDE

Status: **beschlossen 2026-10-02 (Owner), Schritte 1–5 gebaut (Live-Test auf dem Mac offen).** Name: **Studio** (Owner, 2026-10-02; später „Axiomata Studio“ als eigene App), Modi **Editor** und **Agents**. Editor und IDE sollen eine Anwendung mit einem Schalter werden
(„Editor“-Modus und „Agenten“-Modus), nicht zwei Programme. Auslöser: das Git-Panel und die Seitenleiste mussten doppelt gedacht
werden, der Editor wird selbst IDE-artig, die IDE ist die „agentische“. Der Name der einen Sache ist offen (Owner: „irgendwann“);
vorgeschlagene Begriffe stehen noch nicht fest.

## Befund (Analyse vom 2026-10-02)

- Das **Layout-Modell ist schon geteilt**: `fileapp/fileDock.ts` baut auf `ide/layout.ts` (Baum aus Gruppen, Tabs, Teilungen); auch
  `ide/paneStore.ts` (Panes verschieben, ohne sie neu zu bauen) gehört beiden.
- Doppelt war die **Darstellung**: `FileDockNode`/`DockNode` (nahezu identisch), `FileGroup`/`PaneGroup` (ähnlich) und die Drag-Logik
  für Tabs und Trennlinien (je ~150 Zeilen in `FileAppView` und `IdeView`).
- Der Editor kann, was das IDE-Dock noch nicht kann: Vorschau-Tab, eine Datei höchstens einmal offen, Rückfrage beim Schließen mit
  ungespeichertem Text, Wiederherstellung, Schnellöffnen (⌘P) und die Tasten (⌘O ⌘N ⌃Tab ⌘1–9 ⌘\), „Zuletzt geöffnet“, Pfad-Kopf mit
  Breadcrumbs, Übergabe vom schwebenden Datei-Panel, Umbenennen/Löschen mitbekommen, die Einstellungs-/Tastenspalte rechts.
- Die IDE hat, was dem Editor fehlt: Terminals, Agenten (Plan-Tab, Diffs), Layout je Projekt in der Datenbank.

## Beschlüsse

- **Der Schalter** (Editor / Agenten) bedeutet: **zwei Layouts je Projekt**. Editor-Modus: nur Datei-Panes, Seitenleiste, Git, Diff.
  Agenten-Modus: das volle Layout mit Terminals und Agenten. Beim Umschalten werden die Panes des anderen Modus nur **ausgeblendet,
  nie beendet** — ein laufender Agent stirbt nicht. Jedes Layout wird je Projekt gemerkt.
- **Alte Editor-Tabs** (`settings.editor.dock`) werden beim Umstieg verworfen; die Wiederherstellung ungespeicherter Dateien
  (Recovery) bleibt unberührt.
- **Grundlage ist das Dock der IDE** (es kann mehr und speichert je Projekt). Die eigenständige Editor-App (ED7) wird später dieselbe
  Arbeitsfläche, nur mit den Datei-, Git- und Such-Panes gebaut; Agenten und Terminal werden dann nicht mitkompiliert.

## Schritte

1. **Gemeinsame Seitenleiste** (Projektleiste, Files/Search/Git, Outline) als ein Bauteil für beide Ansichten.
2. **Doppelten Dock-Code zusammenziehen** — **gebaut (2026-10-02):** `ide/dockDrag.svelte.ts` (`DockDrag`: Tab- und
   Trennlinien-Ziehen, Geometrie einmal beim Start gemessen, jedes Ende beendet das Ziehen) und ein einziges `ide/DockNode.svelte`
   mit `group`-Snippet (IDE: `PaneGroup`, Editor: `FileGroup`); `FileDockNode.svelte` entfällt. Mit Tests (`dockDrag.test.ts`).
   `FileGroup`/`PaneGroup` bleiben vorerst zwei (sie zeigen verschiedene Tab-Inhalte); sie wachsen mit Schritt 3 zusammen.
3. **Die Arbeitsfläche selbst:** `IdeView` bekommt die Datei-Fähigkeiten des Editors (Liste oben), Layout je Projekt und Modus, den
   Schalter im Kopf. Der große Brocken, in Checkpoints zu zerlegen.
   - **3a gebaut:** die Tasten ⌃Tab, ⌘1–9, ⌘\ / ⇧⌘\ und ⌘O in der IDE (`ide/dockKeys.ts`, auf der Gruppe des zuletzt benutzten Tabs). **3b gebaut:** Rückfrage beim Schließen einer Datei mit ungespeichertem Text (×, ⌘W nur auf Datei-Tabs, Vi `:q`; `ide/fileHandles.ts`). **3c gebaut:** Umbenennen im Baum (`files:renamed`) zieht Pfad und Titel der offenen Datei-Tabs nach (`layoutAfterRename`). **3d gebaut — der Schalter:** im Kopf *Editor | Agents*; zwei Layouts je Projekt in einer `layout_json` (`ide/modes.ts`: `{mode, layouts:{editor, agents}}`, alte Zeilen lesen sich als Agents-Layout), das nicht gezeigte Layout ist *geparkt* — seine Panes bleiben gemountet und laufen weiter (`projectSession.switchMode`). Ein Agent oder Terminal öffnet sich immer im Agents-Layout (schaltet dorthin). Offen: Vorschau-Tab, ⌘N, „Zuletzt geöffnet“, Pfad-Kopf, Übergabe vom Datei-Panel, Umbenennen/Löschen, Layout je Modus.
4. **Ein Einstieg im Ring**; die alte Editor-Ansicht fällt weg, das schwebende Datei-Panel übergibt an die Arbeitsfläche.
5. **Name und Aufräumen** der Begriffe in der App, `AGENTS.md`, den Plänen.


## Stand Schritt 3/4 (gebaut, nicht auf dem Mac getestet)

- **Datei-Fähigkeiten in der IDE:** Vorschau-Tab (Einzelklick im Baum/Treffer ersetzt den Vorschau-Tab; Bearbeiten oder Doppelklick auf den Tab
  heftet ihn an; kursiv), ⌘N (neue Notiz, ein Entwurf zugleich; nach ⌘S zieht der Tab auf die Datei um), Übergabe vom schwebenden
  Datei-Panel (`fileapp/handoff.ts` → `IdeView.takeWaiting`, schaltet in den Editor-Modus; die Live-Sitzung geht mit), „Zuletzt geöffnet"
  im ⌘P, Tasten ⌘N/⌘O/⌘W/⌃Tab/⌘1–9/⌘\ auch ohne Projekt. Dateien lassen sich ohne offenes Projekt öffnen (Dock bleibt sichtbar, solange
  Tabs da sind; gespeichert wird das Layout nur mit Projekt).
- **Ein Einstieg:** die Ring-Einträge „Editor" und „IDE" öffnen dieselbe Ansicht (`ide/modeRequest.ts`): „Editor" im Editor-Modus, „IDE" im
  Agents-Modus. Die alte Editor-Ansicht (`FileAppView`, `FileGroup`, `FileTab`, `fileDock`) ist entfernt; `settings.editor.dock` wird nicht mehr gelesen.
- **Pfad-Kopf:** die Kopfzeile zeigt Pfad und Symbol-Breadcrumbs der vordersten Datei (aus denselben Daten wie die Outline). Einstellungs-/Tastenspalte ist als Inspector da.

## Stand Schritt 5 (Name)

- **Studio** ist der eine Ring-Eintrag (`shell:studio`, öffnet im zuletzt benutzten Modus des Projekts); Kopfzeile, Fenster-Label und IconBar heißen so.
  Der Typ-Id bleibt `view:ide`, damit gespeicherte Ring-Einstellungen gelten; der alte Eintrag `view:editor` wird beim Laden darauf abgebildet
  (`core/ringTypes.ts`, in `hiddenBuiltins` und in Gruppen). Die Events `shell:ide` (Agents-Modus) und `shell:editor` (Editor-Modus, Panel-Übergabe) bleiben als Einstiege.
- Interne Namen (`ide/`, `IdeView`, `axiomata-ide`) bleiben vorerst; sie umzubenennen lohnt erst mit der Extraktion (ED7).

## Stand: Aktivitätsleiste (nach dem ersten Mac-Test, 2026-10-02)

- **Linke Icon-Leiste** (`ide/ActivityRail.svelte`): Dateien, Suche, Git (mit Zähler), darunter *Terminal* (Aktion: jeder Klick öffnet ein neues
  Terminal), abgesetzt *Agents* (mit Zähler der arbeitenden Agenten). Ein Klick auf das gezeigte Icon klappt die Seitenspalte ein (auch ⌘B).
  Einstellungen und Tastenkürzel bleiben rechts im Kopf. Die Spalte zeigt je Ansicht (`settings.ide.tree.view`) Files, Search, Git oder das
  **Agents-Panel** (`ide/AgentsPanel.svelte`, aus dem früheren `AgentPicker`); eingeklappt wird sie nur versteckt, nicht entladen.
- **Dock-Gruppen haben kein „+" mehr**; Files/Search/Git-Panes gibt es nur noch aus alten Layouts. Ein Files-Pane aus einem alten Layout wird
  beim Öffnen entfernt (`projectSession.withoutFilesPanes`) — das war der doppelte Baum.
- **Die zwei Modi sind zwei Arbeitsflächen** (je ein Dock-Layout pro Projekt), alles andere ist gemeinsam. Beim Öffnen eines Agenten oder
  Terminals wird nicht mehr automatisch in den Agents-Modus gewechselt. Mehr als zwei Flächen wären nur eine Liste statt zweier Felder
  (`ide/modes.ts`) — bewusst nicht gebaut.
- **Vi:** mit Vi-Modus stehen dessen Tastengruppen oben in der Kürzelliste; „File app" und „IDE" sind eine Gruppe „Studio".
- **Agenten-Kommunikation (M7.5, A2A über MCP)** kommt später (Owner); das Agents-Panel ist der Platz, an dem Nachrichten und Aufgabe je Agent
  einmal erscheinen können.
