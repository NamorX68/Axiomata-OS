# Detailplan: M7.2 CP6 + CP6b — Lebenszyklus-Status und Plan-Tab

Status: **erledigt** (2026-09-23) — gebaut, gegrillt, live am echten Mac getestet
(Status, Blink-Fix, beide Planmodi, Tasks nach Planfreigabe).
Gehört zu [`agentic-ide.md`](agentic-ide.md) §5, M7.2 — dort stehen die
Meilensteinkette und die acht Grundentscheidungen; hier steht, wie die beiden
Checkpoints konkret gebaut werden. Zuschnitt vom Owner bestätigt: **CP6 und CP6b
zusammen in einem Durchgang**, weil derselbe Kanal beides trägt.

## Kontext

Ein Agent läuft heute als PTY-Kachel: man sieht sein Terminal, aber die App weiß
nichts über ihn. Ob er gerade arbeitet, auf eine Freigabe wartet oder fertig ist,
steht nur als Pixel auf dem Bildschirm. Das ist an drei Stellen zu wenig:

- Bei mehreren Agenten nebeneinander sieht man nicht, wer auf eine Antwort wartet
  — man muss jede Pane einzeln anschauen.
- Der Plan-Tab (`AgentPane.svelte`, Owner-Wunsch E7) ist bis heute ein
  deaktivierter Knopf.
- Der Status ist laut Plan **Zustellbedingung** für das Postfach in M7.5 CP13:
  eine Nachricht erreicht einen TUI-Agenten erst, wenn er auf Eingabe wartet.
  Ohne CP6 gibt es kein A2A.

Ziel: echter Status aus einer strukturierten Quelle, nicht vom Bildschirm geraten
— und derselbe Kanal trägt gleich die Plan-Schritte, sodass der Plan-Tab bewohnt
wird statt deaktiviert zu bleiben.

## F2 ist beantwortet (Recherche 2026-09-22, beide CLIs lokal installiert)

- **Claude Code 2.1.280**: Hooks in einer Settings-Datei. Relevante Ereignisse:
  `SessionStart`, `UserPromptSubmit`, `Notification` (Matcher `permission_prompt`
  / `idle_prompt`), `Stop`, `SessionEnd`, `PostToolUse`. Jeder Hook bekommt
  sein JSON auf stdin. (Die ursprüngliche Annahme „`PostToolUse` mit Matcher
  `TodoWrite` liefert den Plan" hat Schritt 0 widerlegt — siehe dort und E14.)
  `claude --settings <datei>` lädt eine Settings-Datei **von außerhalb des
  Repos** — damit muss keine fremde Konfigurationsdatei angefasst werden.
- **Opencode 1.18.31**: Plugin-Dateien aus `.opencode/plugin/` des
  Arbeitsverzeichnisses **oder aus `$OPENCODE_CONFIG_DIR/plugin/`** (Letzteres
  nutzen wir, E12). Ein Plugin exportiert eine Funktion und bekommt
  `{ project, client, $, directory, worktree }`; der `event`-Hook liefert u.a.
  `session.created`, `session.idle`, `session.error`, `permission.asked`,
  `permission.replied`, `tool.execute.before/after` und — für den Plan-Tab —
  **`todo.updated`**.

Damit ist **F2 in `agentic-ide.md` §6 erledigt** und beim Abschluss dort
nachzutragen.

## Entscheidungen

Stand nach dem Grillen am 2026-09-22 (Runden Q1–Q7, alle vom Owner bestätigt).
E9–E13 sind die ursprünglichen, E12 und E13 dabei überarbeitet; E14–E18 kamen
aus Schritt 0 und dem Grillen hinzu.

- **E9 — Der Status kommt nicht in die Datenbank, und CP6 bringt keine
  Migration.** Abweichung vom Wortlaut in `agentic-ide.md` („eigene Migration"),
  mit Grund: Der Status ist Laufzeitzustand, der die App nicht überlebt (F7 — die
  PTY stirbt mit dem Fenster). Eine Zeile `status='working'`, die einen Neustart
  überlebt, wäre nach dem Neustart eine Lüge. Stattdessen ein Dateikanal unter
  `~/.axiomata/agent-events/<agent-id>/`. Kein `SCHEMA_SQL_V4`.
- **E10 — Der Kanal sind kleine Dateien mit dem *aktuellen* Stand, kein
  Ereignisstrom.** `state` (ein Wort plus Unix-Zeitstempel), `started` (wann der
  Agent zuletzt gestartet wurde) und — nur für Opencode — `plan.json`. Atomar
  geschrieben (temporäre Datei, dann umbenennen). Die Ereignis-Historie bekommt
  M7.5 mit seinem eigenen Ledger.
- **E11 — Die Zustandsableitung steht im Hook, nicht in Rust.** Hook bzw. Plugin
  schreiben das fertige Wort (`idle`, `working`, `waiting`, `ended`); Rust liest
  eine Zeile. **Eine benannte Ausnahme** (E14): den Plan von Claude Code liest
  Rust selbst.
- **E12 — Im Worktree wird nie etwas geschrieben, für kein Harness** (überarbeitet
  nach Schritt 0). Claude Code bekommt `--settings <kanal>/claude-settings.json`,
  Opencode `OPENCODE_CONFIG_DIR=<kanal>/opencode` mit dem Plugin in
  `plugin/axiomata-lifecycle.js`. Beides liegt in unserem Verzeichnis. Damit
  gibt es keine untracked Datei, die ein Agent mitcommitten könnte, und auch der
  **geteilte Ordner** bekommt einen Status — die frühere Ausnahme entfällt.
  Der Reset beim Start fasst `opencode/` nicht an (Opencode legt dort
  `node_modules/` ab).
- **E13 — Bei einem eigenen Befehl wird die Env trotzdem gesetzt** (überarbeitet).
  `OPENCODE_CONFIG_DIR`, `CLAUDE_CODE_TASK_LIST_ID` und `AXIOMATA_EVENTS` kommen
  immer mit; nur `--settings …` wird allein an den *generierten* Befehl
  angehängt (wie `--model` in CP4). Ein eigener Opencode-Befehl ist damit ohne
  Zutun angeschlossen; ein eigener Claude-Befehl schließt sich per
  `claude --settings "$AXIOMATA_CLAUDE_SETTINGS" …` selbst an (geht, weil der
  Befehl in eine Shell getippt wird). Kommt bei einem eigenen Befehl 10 s lang
  kein Ereignis, sagt die Statuszeile „kein Statuskanal (eigener Befehl)" statt
  eines ewigen „starting".
- **E14 — Der Plan von Claude Code kommt aus dessen eigenem Task-Verzeichnis**
  (Q1). `TodoWrite` gibt es nicht mehr, die Task-Werkzeuge melden im Hook nur
  Einzeländerungen. Wir setzen `CLAUDE_CODE_TASK_LIST_ID` pro Agent und lesen
  `<claude-home>/tasks/<liste>/*.json` tolerant (unbekannter Inhalt ⇒ Schritt
  übersprungen bzw. `None`, nie ein Fehler). Bewusst in Kauf genommen: das
  Format ist Claude-Code-intern. Der Leser ist eine Funktion, gekapselt in
  `lifecycle.rs`. Die Listen-Id trägt neben der Agent-Id eine kurze, stabile
  Prüfsumme der Kanalwurzel — sonst teilten sich Agent #1 eines Scratch-
  `AXIOMATA_HOME` und Agent #1 der echten Installation eine Liste.
- **E15 — Zuordnung Ereignis ⇒ Zustand** (Q3):

  | Zustand | Claude Code | Opencode |
  |---|---|---|
  | `starting` | Kanal leer nach Reset | Kanal leer nach Reset |
  | `idle` | `SessionStart`, `Stop` | Plugin geladen, `session.status: idle` |
  | `working` | `UserPromptSubmit`, `PostToolUse` (jedes Werkzeug) | `session.status: busy`/`retry`, `permission.replied` |
  | `waiting` | `Notification` mit `notification_type` `permission_prompt` oder `elicitation_dialog` — im Hook per `grep` gefiltert, nicht per Matcher (Live-Test: sonst blinkte ein untätiger Agent beim 60-s-`idle_prompt`) | `permission.asked`, `question.asked` |
  | `ended` | `SessionEnd` | `process.on("exit")` im Plugin (Q7) |

  `PostToolUse → working` holt einen Agenten nach erteilter Freigabe aus
  `waiting` zurück. **Nur die Hauptsitzung zählt:** das Plugin ignoriert
  Sitzungen mit `parentID` (Opencodes Unteragenten); bei Claude Code löst ein
  Unteragent nur `SubagentStop` aus, nicht `Stop`. Kein eigener Zustand
  `error`. Blinder Fleck bei `kill -9` (beide Harnesses) — der Reset beim
  nächsten Start heilt ihn.
- **E16 — Der Plan überlebt einen Neustart** (Q4). Der Reset leert `state`, nicht
  den Plan; `~/.claude/tasks/<liste>/` bleibt liegen, und Claude sieht seine
  alten Aufgaben über `TaskList`. Ein Plan, der älter ist als der letzte Start
  (`started`), heißt im Plan-Tab „aus früherer Sitzung". Beim **Löschen eines
  Agenten** verschwinden sein Kanal und genau *sein* Task-Verzeichnis, sonst
  nichts in `~/.claude`.
- **E17 — Der Plan-Tab zeigt die gemeinsame Form** (Q6):
  `PlanStep { text, state: todo|doing|done|cancelled, detail? }`. `detail`
  (Claudes `description`) nur als Tooltip; Abhängigkeiten (`blockedBy`) und
  Opencodes `priority` erst mit M7.5, wenn Schritte zu Karten werden.
  `cancelled` gibt es nur bei Opencode und wird durchgestrichen gezeigt.
- **E19 — Beide Agenten werden zum sichtbaren Planen angehalten** (Live-Test,
  Owner 2026-09-22). Ohne Anstoß legt Claude Code für eine kleine Aufgabe keine
  Task-Liste an, und der Plan-Tab blieb leer. Eine gemeinsame Anweisung
  `<kanal>/planning.md` geht an Claude Code per `--append-system-prompt-file`
  (nur beim generierten Befehl, wie `--settings`) und an Opencode über
  `instructions` in `<kanal>/opencode/opencode.json` — additiv zur
  Nutzerkonfiguration, geprüft mit `opencode debug config`. Bei Opencode gilt sie
  damit auch für einen eigenen Befehl, weil `OPENCODE_CONFIG_DIR` immer gesetzt
  wird; vom Owner so akzeptiert.
- **E20 — Der Plan aus Claudes Planmodus erscheint im Plan-Tab** (Owner-Wunsch:
  plant gern im Planmodus). Claude Code schreibt ihn als Datei nach
  `<claude-home>/plans/<name>.md`; `plansDirectory` kann nicht helfen, weil es
  innerhalb des Projekts liegen muss (E12). Deshalb merkt sich ein
  `PostToolUse`-Hook auf `Write|Edit` den Pfad, sobald er im Plan-Ordner liegt.
  Rust liest die Datei nur, wenn ihr **kanonischer** Pfad dort liegt und auf
  `.md` endet — der Zeiger kommt aus einem fremden Prozess. Der Tab zeigt oben
  die Tasks, darunter den Plan als bereinigtes Markdown, aufgeklappt solange es
  keine Tasks gibt. `ExitPlanMode` selbst wird nicht gebraucht (im `-p`-Modus ruft
  Claude es gar nicht auf). Löschen eines Agenten fasst `plans/` nicht an.
- **E18 — Das Mini-Harness bekommt noch keinen Anschluss.** Es existiert erst mit
  M7.4 und bringt dann seine eigene Abbildung mit (E11); bis dahin sagt die
  Statuszeile „kein Statuskanal".

## Schritte

### Schritt 0 — Handversuch, bevor Code entsteht (blockierend)

Unbewiesene Annahme: dass `claude --settings <datei>` die Hooks aus dieser Datei
wirklich ausführt. Vor allem anderen: eine Settings-Datei im Scratchpad, ein
`claude --settings … -p "sag hallo"` unter einem Scratch-`AXIOMATA_HOME`, und
nachsehen, ob die `state`-Datei erscheint. Dasselbe für ein minimales
Opencode-Plugin mit `session.idle`.
**Fällt der Versuch für Claude Code aus, ist Plan B** `.claude/settings.local.json`
**im Worktree** — immer noch unser Verzeichnis, aber ohne den Vorteil aus E12 für
den geteilten Ordner. Das Ergebnis wird hier festgehalten.

**Ergebnis (2026-09-22, Claude Code 2.1.267, Opencode 1.18.32):**

- **`claude --settings <datei>` trägt.** Die Hooks aus der fremden Datei feuern,
  zusätzlich zu denen des Nutzers: `SessionStart → UserPromptSubmit → Stop →
  SessionEnd` kamen in dieser Reihenfolge an, die `sh`-Zeile mit `> tmp && mv`
  schrieb den Zustand korrekt. Plan B ist nicht nötig.
- **`TodoWrite` gibt es nicht mehr.** Diese Claude-Code-Version plant mit
  `TaskCreate` / `TaskUpdate` / `TaskList`. Deren `PostToolUse`-Payload ist
  **inkrementell** (`{subject}` → `{task:{id}}`, `{taskId,status}` →
  `{statusChange:{from,to}}`), kein Schnappschuss. Den vollständigen Stand hält
  Claude Code selbst als `~/.claude/tasks/<liste>/<n>.json`
  (`{id, subject, description, status, blocks, blockedBy}`), und die Liste lässt
  sich per Env **auf eine feste Id legen**: `CLAUDE_CODE_TASK_LIST_ID=<id>` ⇒
  `~/.claude/tasks/<id>/`. Die Annahme „ein Hook liefert den Plan-Schnappschuss"
  aus Schritt 1 gilt damit nur noch für Opencode.
- **Opencode-Plugin trägt, `todo.updated` ist ein Schnappschuss**
  (`{sessionID, todos:[{content,status,priority}]}`), dazu `session.status`
  (`busy`/`idle`), `session.idle`, `session.created`. Noch nicht live gesehen:
  `permission.asked` (braucht einen echten Freigabedialog — Live-Test).
- **Opencode braucht keine Datei im Worktree:** `OPENCODE_CONFIG_DIR=<dir>` lädt
  `<dir>/plugin/*.js` **zusätzlich** zur globalen Konfiguration (MCP-Server und
  Modell blieben in `opencode debug config` unverändert). Das ist wichtig, weil
  ein Plugin in `<worktree>/.opencode/plugin/` dort eine untracked Datei wäre,
  die ein Agent mitcommitten könnte. Opencode legt in diesem Verzeichnis
  `node_modules/`, `package.json` und `.gitignore` an — der Kanal-Reset darf es
  also nicht mitleeren.
- Nicht geprüft: der `Notification`-Hook bei einem echten Freigabedialog in
  Claude Code (im `-p`-Modus gibt es keinen) — ebenfalls Live-Test.

### Schritt 1 — `crates/axiomata-ide/src/lifecycle.rs` (neu)

Der Kern, testbar ohne Agenten und ohne DB:

- `enum AgentState { Starting, Idle, Working, Waiting, Ended }` mit
  `parse`/`as_str`; `struct AgentStatus { agent_id, state, since, plan }`.
- `struct Locations { worktrees, events, claude_tasks }` — die Crate besitzt
  weiterhin keine eigenen Pfade, der Einbetter reicht sie herein
  (`claude_tasks` = `$CLAUDE_CONFIG_DIR/tasks`, sonst `~/.claude/tasks`).
- `struct Channel` — `for_agent(locations, id)`, `reset()` (leert `state`,
  schreibt `started`), `read_status()`, `read_plan(harness)`, `forget()`
  (Kanal und eigenes Task-Verzeichnis weg).
- `PlanSnapshot { steps, updated_at, from_earlier_session }`. Zwei Leser, beide
  tolerant: `plan.json` (unser Format, vom Plugin geschrieben) und das
  Claude-Task-Verzeichnis (E14; nach numerischer Id sortiert).
- `install(harness, channel) -> Hookup { args, env, connected }`: schreibt
  `claude-settings.json` bzw. das Opencode-Plugin **nur bei geändertem Inhalt**.
  Die Settings-Datei wird mit `serde_json` gebaut, Pfade in den `sh`-Zeilen
  einfach gequotet, im Plugin als JSON-Stringliteral eingebettet — keine
  Handverkettung von Anführungszeichen.

Die Hook-Kommandos sind reine `sh`-Zeilen ohne Axiomata-Binary (die CLI ist in
der gebündelten App nicht vorhanden — `tauri.conf.json` hat kein `externalBin`).
Temporäre Datei mit `$$` im Namen, weil `PostToolUse` bei parallelen
Werkzeugaufrufen gleichzeitig feuern kann.

### Schritt 2 — Anschluss in `provision.rs`

- `prepare(db, &Locations, id)` statt `prepare(db, worktree_base, id)`.
- `prepare` ruft `Channel::reset` und `lifecycle::install` — idempotent, bei
  jedem Start.
- `Provisioned` bekommt `launch_command` (generierter Befehl plus
  `Hookup::args`) und `launch_env` (`effective_env` plus `Hookup::env`, die
  Identitätszeilen zuletzt, wie CP5). Beides aus Rust, nie aus gespeichertem
  Layout (CP4-Sicherheitsregel). Die Pane startet mit diesen beiden statt mit
  `agent.effective_*`.

### Schritt 3 — Befehlsschicht

- `axiomata_core::paths::agent_events_dir()` und `claude_tasks_dir()` neben
  `worktrees_dir()`, dazu ein `ide_locations()`, das alle drei bündelt.
- Tauri: `ide_agent_states(project_id) -> Vec<AgentStatus>` — ein Aufruf für
  alle Agenten des Projekts pro Takt. `delete_ide_agent` ruft zusätzlich
  `Channel::forget`.
- CLI: `ide agents status <project>`; `ide agents delete` räumt ebenso auf.

### Schritt 4 — Frontend

- `ide/agentStatus.ts` (neu): Store plus Takt (1 s, nur solange die IDE-Ansicht
  offen ist; Intervallgeber injizierbar für den Test). Dazu die reine Funktion,
  die aus Status + Profil die Anzeige macht (inkl. der 10-s-Regel aus E13).
- `panes/AgentPane.svelte`: Punkt plus Wort in der Statuszeile; **Plan-Tab
  bewohnt** — ✓ / ▸ / ○ / durchgestrichen, „noch kein Plan", „aus früherer
  Sitzung". Der Plan-Tab **versteckt** das Terminal weiterhin nur.
- `PaneGroup.svelte`: derselbe Punkt neben dem Tab-Titel für Agent-Tabs.
- `AgentPicker.svelte`: Punkt je Agent.
- `core/backend.ts`, `core/devmock.ts` (Fixture für `ide_agent_states`).
- Farben nur über `--ax-*`-Token.

Alle drei Anzeigeorte sind vom Owner ausdrücklich gewünscht, nicht optional.

## Verifikation

- `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`
  (gebündelt am Ende, gemäß der Kadenzregel in `CLAUDE.md`).
- `cd apps/dashboard && npm run check && npx vitest run`.
- Rust-Tests: Zustands-Parser; tolerantes Plan-Lesen beider Formate (inkl. Müll ⇒
  `None`); `install` ist idempotent; `reset` leert `state`, nicht den Plan;
  `forget` entfernt nur das eigene Task-Verzeichnis. Dazu ein Test, der eine erzeugte Hook-Zeile
  **tatsächlich per `sh -c` ausführt** und prüft, dass danach der richtige
  Zustand in der Datei steht — das validiert die Shell-Zeile ohne Claude Code.
- Frontend-Test `agentStatus.test.ts`: Takt, Abbildung auf Anzeigewerte,
  Aufräumen beim Verlassen der Ansicht.
- **Live am echten Mac** (unverzichtbar, §8 des Plans): `cargo tauri dev` unter
  Scratch-`AXIOMATA_HOME`, je ein Agent mit Claude Code und mit Opencode im
  echten Axiomata-Repo. Erwartet: Frage stellen ⇒ „working"; Berechtigungsdialog
  ⇒ „waiting"; Antwort fertig ⇒ „idle"; eine Aufgabe mit mehreren Schritten ⇒
  Plan-Tab füllt sich (Claude: Task-Werkzeuge; Opencode: `todowrite`) und markiert den laufenden Schritt. Zusätzlich: Wechsel auf
  den Plan-Tab darf den Agenten **nicht** neu starten (Owner-Fund nach CP4).
- Sub-Agenten vor dem Commit, projektlokale Kadenz: `rust-test-engineer`,
  `security-auditor` (der Kanal nimmt Eingaben von fremden Prozessen entgegen und
  erzeugt Shell-Zeilen), `architecture-reviewer` (neues Modul, Änderungen über
  mehr als drei Dateien), `docs-writer`.
- Zum Schluss: `docs/plans/agentic-ide.md` (CP6/CP6b als erledigt, F2 als
  beantwortet, E9–E13 nachtragen) und `docs/architecture.md` §5 aktualisieren.

## Bewusst nicht in diesem Checkpoint

- Kein Weiterleben der Agenten über das Schließen der App hinaus (F7 bleibt
  offen).
- Kein Push des Status ins Backend-Ereignissystem — der Takt genügt, solange nur
  die Oberfläche zusieht. Der echte Aktualisierungspfad ist CP12b, und zwar
  fürs Brett.
- Keine Beförderung eines Planschritts auf das Kanban-Brett. Das ist die zweite
  Hälfte von E7 und gehört zu M7.5, wenn Karten und Agenten zusammenkommen.
