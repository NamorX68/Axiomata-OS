# Studio

Wie die Werkbank für Code-Projekte aufgebaut ist: Projekte, die Modi Editor, Canvas und Flow, Panes, Agenten mit eigenem Worktree, Git, Run/Tasks und Debug.

## Was es ist

Das Studio ist Axiomatas Entwicklungsumgebung. Es öffnet sich groß über allem und hat drei **Modi**:

- **„Editor“** — Dateibaum, Reiter, geteilte Gruppen ([Editor](editor.md)).
- **„Canvas“** — die Panes der laufenden Agenten und Terminals nebeneinander.
- **„Flow“** — Pläne, Agententeam und Graph ([Flow](flow.md)).

Ein Modus, den du nicht ansiehst, läuft weiter. **Agenten** sind Sitzungen im Projekt: eine **Rolle** (wofür) auf
einer **Engine** (Harness + Modell), mit eigenem Branch in einem eigenen Worktree unter `~/Worktrees/axiomata-agents`.
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
- **„Agents“:** oben die Master-Kacheln (Glut, Claude Code, Opencode), darunter „Start an agent“ mit Rollen nach
  „Planning / Implementation / Review / Testing / Documentation“, die Agenten-Karten mit Status, „New agent…“ und
  „Engines & roles…“ (Engines: „Add engine…“; Rollen: „Add role…“, Grenzen unter „Advanced“).
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
2. **Agenten starten:** „Agents“ → unter „Start an agent“ eine Rolle wählen — oder eine Karte im Kanban mit
   „Start …“ starten ([Kanban](kanban.md)).
3. **Arbeit eines Agenten übernehmen:** Agenten-Pane → „Diffs“ → prüfen → „Take over…“ → Squash oder Merge.

## Fallen

- Aufgaben aus `.axiomata/tasks.json`, Debug-Setups und Rollen aus `.axiomata/agents/` eines Projekts laufen erst,
  nachdem du sie geprüft hast („… need your review“ → „Review…“ → „Allow …“). Ändert sich die Datei, fragt die App
  erneut.
- Einen Agenten entfernen nimmt **Worktree und Branch** mit; ungesicherte Arbeit ist dann weg (die App fragt).
- „Take over…“ und „Commit & Push“ pushen nie mit Gewalt; „Take over…“ pusht gar nicht.
- Der Opencode-Master ist nur dein eigenes `opencode` im Projektordner, ohne Team-Werkzeuge.
- Mehrere Master arbeiten im **selben** Ordner: ändern zwei dieselbe Datei, kommen sie sich in die Quere. Für
  getrennte Arbeit einen Agenten mit eigenem Worktree starten.
- Projektwurzel darf nicht `$HOME`, `/Users` oder `/` sein.
- Debuggen von Node/TypeScript gibt es nicht.

## Siehe auch

[Flow](flow.md) · [Kanban](kanban.md) · [Glut](glut.md) · [Editor](editor.md) · [Kosten](kosten.md)
