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

- **Öffnen:** App-Ring oder Dock „Studio“; „Back to the OS“ oben führt zurück. Daneben „Editor settings“ und
  „Keyboard shortcuts“.
- **Projektleiste** (Seitenleiste oben): „Open project…“, „New project…“ („Project name“, „Start a git repository“,
  „Choose where…“), „Close project“, „Change folder…“, „Remove from list“ (der Ordner bleibt), Farbmarke.
- **Leiste links:** „Files“, „Search the project“ (⇧⌘F), „Git“, „Run — build, test, start“, „Debug“, „Agents“;
  dazu „New terminal“ und „Open a file…“ (⌘O).
- **„Agents“** (in jedem Modus derselbe Reiter), von oben nach unten:
  - **„Master“** — die Master-Kacheln Glut, Claude Code und Opencode.
  - **„Favorites“** — deine gemerkten Agenten als Kacheln; ein Klick startet. Ein Favorit, dessen Engine gerade nicht
    laufen kann, ist abgeblendet und wird nie still auf eine andere Engine gelegt.
  - **„Start an agent“** — eine Rolle wählen, dann Light · Medium · Heavy (vorgewählt die Standard-Stufe der Rolle),
    dann die Engine („Default of the tier“ oder eine Engine der Stufe, die Standard-Engine mit „standard“ markiert),
    „Start“. Kürzester Weg: Rolle → „Start“. Der Stern („Keep this choice as a favorite“) merkt die Wahl.
  - **„Sessions“** — nur, was dich angeht: laufende und wartende Sitzungen und unter „Waiting for you: take over or
    discard“ beendete mit Arbeit, die noch nicht übernommen ist. Ist nichts davon da: „No session runs or waits for
    you.“ Jede Sitzung hat einen Stern („Keep this agent as a favorite“).
  - unten „New agent…“ und **„Models & roles…“** (Reiter „Models“ und „Roles“, [Modelle und Rollen](modelle-rollen.md)).
- **Agenten-Pane:** Reiter „Terminal“, „Plan“, „Diffs“, „Inbox“ (Nachrichten an die Sitzung, ⌘↩ sendet);
  „Restart the agent“. Unter „Diffs“: „Discard file“, „Commit…“, „Take over…“ (Squash oder Merge, „Never pushes.“).
- **„Git“:** Branch wechseln, „New branch from here“, „Fetch …“, „Stage all“, Commit-Nachricht (⌘↩), ✨ schreibt
  einen Vorschlag (ein bezahlter Aufruf), „Commit“, „Commit & Push“, „Push“ / „Publish branch“.
- **„Run“:** Aufgaben aus Cargo.toml, package.json, pyproject.toml und Makefile, gruppiert nach Run / Build / Test /
  Check / Other; „New task“.
- **„Debug“:** Python, Rust, C/C++ und Swift; „What to debug“, „Debug“, „New configuration…“, Schritt-Knöpfe,
  Variablen, Watch, Haltepunkte, Konsole.
- **Tasten:** ⌘\ / ⇧⌘\ teilen, ⌘1–9 Reiter, ⌃⇥ nächster Reiter, ⌘P Datei, ⌘B Baum, ⌘W Reiter schließen, ⌘N neue
  Notiz (Windows/Linux die Buchstaben mit Ctrl+Shift).

## Häufige Handgriffe

1. **Projekt anlegen:** Projektleiste → „New project…“ → Name, Ort, ggf. „Start a git repository“.
2. **Agenten starten:** „Agents“ → „Start an agent“ → Rolle → ggf. Stufe und Engine → „Start“ — oder einen Favoriten
   anklicken, oder eine Karte im Kanban mit „Start …“ starten ([Kanban](kanban.md)).
3. **Einen Agenten merken:** in „Start an agent“ oder an einer Sitzung den Stern klicken; er steht dann unter
   „Favorites“. Ein Favorit mit „Default of the tier“ zieht mit, wenn du die Standard-Engine der Stufe änderst.
4. **Arbeit eines Agenten übernehmen:** Agenten-Pane → „Diffs“ → prüfen → „Take over…“ → Squash oder Merge.

## Fallen

- Aufgaben aus `.axiomata/tasks.json`, Debug-Setups und Rollen aus `.axiomata/agents/` eines Projekts laufen erst,
  nachdem du sie geprüft hast („… need your review“ → „Review…“ → „Allow …“). Ändert sich die Datei, fragt die App
  erneut.
- Einen Agenten entfernen nimmt **Worktree und Branch** mit; ungesicherte Arbeit ist dann weg (die App fragt).
- Eine beendete Sitzung **ohne eigene Arbeit** (nichts Ungesichertes, kein Commit über ihre Basis, keine offene Karte)
  steht nicht mehr in der Liste. Die wöchentliche Wartung prüft sie erneut und räumt sie dann weg — Worktree, leerer
  Branch, Eintrag — und nennt sie unter Settings → Storage ([Storage](storage.md)). Arbeit wird nie von selbst
  gelöscht.
- Ein Modell ohne Stufe ist kein Agent und fehlt unter „Start an agent“; schalte es unter „Models“ ein und gib ihm
  eine Stufe.
- „Take over…“ und „Commit & Push“ pushen nie mit Gewalt; „Take over…“ pusht gar nicht.
- Der Opencode-Master ist nur dein eigenes `opencode` im Projektordner, ohne Team-Werkzeuge.
- Mehrere Master arbeiten im **selben** Ordner: ändern zwei dieselbe Datei, kommen sie sich in die Quere. Für
  getrennte Arbeit einen Agenten mit eigenem Worktree starten.
- Projektwurzel darf nicht `$HOME`, `/Users` oder `/` sein.
- Debuggen von Node/TypeScript gibt es nicht.

## Siehe auch

[Modelle und Rollen](modelle-rollen.md) · [Flow](flow.md) · [Kanban](kanban.md) · [Glut](glut.md) ·
[Editor](editor.md) · [Kosten](kosten.md)
