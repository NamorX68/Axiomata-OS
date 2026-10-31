# Studio

Wie die Werkbank für Code-Projekte aufgebaut ist: Projekte, die Modi Editor, Canvas und Flow, Panes, Agenten mit eigenem Worktree, Git, Run/Tasks und Debug.

## Was es ist

Das Studio ist Axiomatas Entwicklungsumgebung. Es öffnet sich groß über allem und hat drei **Modi**:

- **„Editor“** — Dateibaum, Reiter, geteilte Gruppen ([Editor](editor.md)).
- **„Canvas“** — die Panes der laufenden Agenten und Terminals nebeneinander.
- **„Flow“** — Pläne, Agententeam und Graph ([Flow](flow.md)).

Ein Modus, den du nicht ansiehst, läuft weiter. **Agenten** sind Sitzungen im Projekt: eine **Rolle** (wofür) auf
einer **Stufe** (Light · Medium · Heavy) und einer **Engine** (Harness + Modell), mit eigenem Branch in einem eigenen
Worktree unter `~/Worktrees/axiomata-agents`. Die Rolle nennt keine Engine — die kommt aus der Stufe: deine
Standard-Engine der Stufe, sonst die billigste ([Modelle und Rollen](modelle-rollen.md)). Ein Agent heißt danach,
etwa „Glut Medium Allrounder (Sonnet 5.5)“, angezeigt in zwei Zeilen (oben Rolle · Stufe, darunter Harness · Modell);
eine zweite Sitzung desselben Agenten bekommt „#2“.
Ein **Master** ist dagegen die Engine so, wie du sie selbst benutzt — im echten Projektordner, ohne Worktree. Ein
weiterer Klick auf die Master-Kachel, während der Master offen ist, startet einen zweiten daneben (`master-glut-2`, …);
er steht als Karte ganz oben in der Agentenliste und lässt sich dort entfernen.

## Wo die Knöpfe sind

- **Öffnen:** App-Ring oder Dock „Studio“; „Zurück zum OS“ oben führt zurück. Daneben „Editor-Einstellungen“ und
  „Tastenkürzel“.
- **Projektleiste** (Seitenleiste oben): „Projekt öffnen…“, „Neues Projekt…“ („Projektname“, „Ein Git-Repository
  anlegen“, „Ort wählen…“), „Projekt schließen“, „Ordner ändern…“, „Aus der Liste entfernen“ (der Ordner bleibt),
  Farbmarke.
- **Leiste links:** „Dateien“, „Projekt durchsuchen“ (⇧⌘F), „Git“, „Run — bauen, testen, starten“, „Debug“, „Agenten“;
  dazu „Neues Terminal“ und „Datei öffnen…“ (⌘O).
- **„Agenten“** (in jedem Modus derselbe Reiter), von oben nach unten:
  - **„Master“** — die Master-Kacheln Glut, Claude Code und Opencode.
  - **„Favoriten“** — deine gemerkten Agenten als Kacheln; ein Klick startet. Ein Favorit, dessen Engine gerade nicht
    laufen kann, ist abgeblendet und wird nie still auf eine andere Engine gelegt.
  - **„Agenten starten“** — die sieben Rollen als Kacheln (Sechseck und Name). Ein Klick öffnet darunter das Menü, ein
    zweiter Klick auf dieselbe Kachel oder „Start“ schließt es: Light · Medium · Heavy (vorgewählt die Standard-Stufe
    der Rolle), dann die Engine aus einer Liste, die bei vielen Modellen scrollt („Standard der Stufe“ oder eine Engine
    der Stufe, die Standard-Engine mit „Standard“ markiert), „Start“. Kürzester Weg: Rolle → „Start“. Der Stern („Diese
    Wahl als Favoriten behalten“) merkt die Wahl.
  - **„Sitzungen“** — nur, was dich angeht: laufende und wartende Sitzungen und unter „Wartet auf dich: übernehmen oder
    verwerfen“ beendete mit Arbeit, die noch nicht übernommen ist. Ist nichts davon da: „Keine Sitzung läuft oder wartet
    auf dich.“ Jede Sitzung hat einen Stern („Diesen Agenten als Favoriten behalten“).
  - unten „New agent…“ und **„Modelle & Rollen…“** (Reiter „Modelle“ und „Rollen“, [Modelle und
    Rollen](modelle-rollen.md)).
- **Agenten-Pane:** Reiter „Terminal“, „Plan“, „Diffs“, „Posteingang“ (Nachrichten an die Sitzung, ⌘↩ sendet);
  „Agenten neu starten“. Unter „Diffs“: „Datei verwerfen“, „Commit…“, „Übernehmen…“ (Squash oder Merge, „Pusht nie.“).
- **„Git“:** Branch wechseln, „Neuer Branch von hier“, „Vom Remote holen …“, „Alle vormerken“, Commit-Nachricht (⌘↩), ✨
  schreibt einen Vorschlag (ein bezahlter Aufruf), „Commit“, „Commit & Push“, „Push“ / „Branch veröffentlichen“.
- **„Run“:** Aufgaben aus Cargo.toml, package.json, pyproject.toml und Makefile, gruppiert nach Starten / Bauen / Testen
  / Prüfen / Sonstiges; „Neue Aufgabe“.
- **„Debug“:** Python, Rust, C/C++ und Swift; „Was debuggt wird“, „Debug“, „Neue Konfiguration…“, Schritt-Knöpfe,
  Variablen, „Beobachten“, Haltepunkte, Konsole.
- **Tasten:** ⌘\ / ⇧⌘\ teilen, ⌘1–9 Reiter, ⌃⇥ nächster Reiter, ⌘P Datei, ⌘B Baum, ⌘W Reiter schließen, ⌘N neue
  Notiz (Windows/Linux die Buchstaben mit Ctrl+Shift).

## Häufige Handgriffe

1. **Projekt anlegen:** Projektleiste → „Neues Projekt…“ → Name, Ort, ggf. „Ein Git-Repository anlegen“.
2. **Agenten starten:** „Agenten“ → „Agenten starten“ → Rolle → ggf. Stufe und Engine → „Start“ — oder einen Favoriten
   anklicken, oder eine Karte im Kanban mit „Starten…“ starten ([Kanban](kanban.md)).
3. **Einen Agenten merken:** in „Agenten starten“ oder an einer Sitzung den Stern klicken; er steht dann unter
   „Favoriten“. Ein Favorit mit „Standard der Stufe“ zieht mit, wenn du die Standard-Engine der Stufe änderst.
4. **Arbeit eines Agenten übernehmen:** Agenten-Pane → „Diffs“ → prüfen → „Übernehmen…“ → Squash oder Merge.

## Fallen

- Aufgaben aus `.axiomata/tasks.json`, Debug-Setups und Rollen aus `.axiomata/agents/` eines Projekts laufen erst,
  nachdem du sie geprüft hast („… brauchen deine Prüfung“ → „Prüfen…“ → „Diese … erlauben“). Ändert sich die Datei,
  fragt die App erneut.
- Einen Agenten entfernen nimmt **Worktree und Branch** mit; ungesicherte Arbeit ist dann weg (die App fragt).
- Eine beendete Sitzung **ohne eigene Arbeit** (nichts Ungesichertes, kein Commit über ihre Basis, keine offene Karte)
  steht nicht mehr in der Liste. Die wöchentliche Wartung prüft sie erneut und räumt sie dann weg — Worktree, leerer
  Branch, Eintrag — und nennt sie unter Einstellungen → Speicher ([Storage](storage.md)). Arbeit wird nie von selbst
  gelöscht.
- Ein Modell ohne Stufe ist kein Agent und fehlt unter „Agenten starten“; schalte es unter „Modelle“ ein und gib ihm
  eine Stufe.
- „Übernehmen…“ und „Commit & Push“ pushen nie mit Gewalt; „Übernehmen…“ pusht gar nicht.
- Der Opencode-Master ist nur dein eigenes `opencode` im Projektordner, ohne Team-Werkzeuge.
- Mehrere Master arbeiten im **selben** Ordner: ändern zwei dieselbe Datei, kommen sie sich in die Quere. Für
  getrennte Arbeit einen Agenten mit eigenem Worktree starten.
- Projektwurzel darf nicht `$HOME`, `/Users` oder `/` sein.
- Debuggen von Node/TypeScript gibt es nicht.

## Siehe auch

[Modelle und Rollen](modelle-rollen.md) · [Flow](flow.md) · [Kanban](kanban.md) · [Glut](glut.md) ·
[Editor](editor.md) · [Kosten](kosten.md)
