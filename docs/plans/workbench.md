# Plan: Eine Arbeitsfläche statt Editor und IDE

Status: **beschlossen 2026-10-02 (Owner), Schritte 1 (gemeinsame Seitenleiste `fileapp/ProjectSidebar.svelte` in Editor und IDE) und 2 gebaut.** Editor und IDE sollen eine Anwendung mit einem Schalter werden
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
4. **Ein Einstieg im Ring**; die alte Editor-Ansicht fällt weg, das schwebende Datei-Panel übergibt an die Arbeitsfläche.
5. **Name und Aufräumen** der Begriffe in der App, `AGENTS.md`, den Plänen.
