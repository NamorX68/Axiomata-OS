# Plan: Opencode 2 sauber anbinden (OC1–OC4)

Status: **gegrillt 2026-09-27** (Q1–Q10, alle vom Owner bestätigt). **OC1–OC4 erledigt** (2026-09-27/28) — der Umbau ist abgeschlossen.
Kommt vor ED6 (LSP) aus [`editor.md`](editor.md); betrifft die Skill-Läufe, den Assistenten-Chat
und die IDE-Agenten aus [`agentic-ide.md`](agentic-ide.md) / [`agent-lifecycle.md`](agent-lifecycle.md).

Maßgebliche Doku: **https://opencode.ai/v2/docs** (nicht `opencode.ai/docs` — das ist noch v1).

## Kontext

Opencode 2 (installiert: 2.0.18) ist keine Sammlung eigenständiger Prozesse mehr, sondern ein
**gemeinsamer Hintergrunddienst** pro Benutzer, an den sich jede Oberfläche (TUI, `opencode run`,
`opencode api`) als Client hängt. Axiomata ist heute noch wie gegen v1 gebaut; gemessen am
2026-09-27 bricht das an fünf Stellen:

1. **Skill-Läufe** parsen den JSON-Stream von `opencode run`. Das Format hat sich zwischen 2.0.17
   (keine Tokens) und 2.0.18 (Tokens für jeden Schritt außer dem letzten) still geändert; jeder
   Lauf galt als fehlgeschlagen (Notfix `1b6dbfb`: der Session-Export gewinnt).
2. **IDE-Start mit Modell**: die Vollbild-TUI kennt kein `--model` mehr → Agenten mit Modell starten nicht.
3. **CP6 (Status + Plan-Tab) ist still tot**: das Plugin ist im v1-Format (v2 will
   `export default { id, setup(ctx) }`), der Config-Schlüssel heißt `plugins`, `OPENCODE_CONFIG_DIR`
   ignoriert der Dienst (mit `--standalone` ersetzt es die globale Config), und alle Event-Namen sind neu.
4. **Kein Todo-Werkzeug mehr** in v2 — der Plan-Tab hatte seine Schritte aus Opencodes Todos.
5. **Nur noch `AGENTS.md`**: v2 liest `CLAUDE.md` nicht mehr als Ersatz.

Nebenbefunde: `--standalone` neben dem Dienst hängt beim Start (gemeinsame Datenbank; mit eigenem
`OPENCODE_DB` nicht); jeder private Server startet eigene MCP-Server (apple-mail öffnete Mail.app
bei jedem Test). v2 betreibt keine LSP-Server mehr (für ED6 ohne Belang — wir bauen einen eigenen Client).

## Entscheidungen

- **Q1 — Hosting:** alle Agenten und Läufe auf dem **einen gemeinsamen Dienst**, kein `--standalone`.
- **Q2 — Kanal:** Axiomata ist **v2-API-Client**. Es legt Sessions selbst an (`POST /api/session`:
  `location.directory`, `model`, `agent`, `permissions`, `title`), hängt die Planungs-Anweisung als
  Session-Anweisung an und liest den Status aus `GET /api/event`. Nach jedem (Wieder-)Verbinden wird
  der Zustand über die Session/Nachrichten neu gelesen (der Stream ist laut Doku flüchtig). Die
  IDE-Kachel öffnet `opencode --session <id>` im Worktree. Plugin und `OPENCODE_CONFIG_DIR` entfallen.
- **Q3 — Plan-Tab:** für Opencode vorerst **Status + Antwort des Plan-Agenten**; die Schrittliste
  kommt später über Axiomatas eigenen MCP-Server (M7, A2A) — für beide Harnesses gleich.
- **Q4 — Projekt-Anweisungen:** **keine Brücke**. Opencode liest `AGENTS.md`, Claude Code `CLAUDE.md`;
  in diesem Repo wird `AGENTS.md` die Quelle und `CLAUDE.md` zieht sie per `@AGENTS.md` herein.
- **Q5 — Skills und Chat** laufen ebenfalls über die API, **als erster Checkpoint**. Stream-Parsing
  und `session export` fallen weg.
- **Q6 — Dienst läuft nicht:** Axiomata startet ihn mit `opencode service start` (dokumentiert);
  danach nicht erreichbar → klare Meldung.
- **Q7 — Berechtigungen:** IDE-Agenten: Opencodes Standard plus **`git push *` immer verboten**.
  Skill-Läufe mit `auto_approve_tools`: „alles erlauben, was nicht ausdrücklich verboten ist“
  (`*`/`*`/`allow`, das Gegenstück zu `--auto`); ohne: jede Berechtigungsfrage wird abgelehnt und der
  Lauf scheitert (wie heute ohne `--auto`) — ein unbeaufsichtigter Lauf wartet nie auf eine Antwort.
- **Q8 — Versionsprüfung** beim Verbinden (`GET /api/info`): Hauptversion ≠ 2 → Abbruch mit Meldung;
  neueres 2.x → Hinweis im Log. Nach jedem Opencode-Update einen echten Skill-Lauf prüfen.
- **Q9 — Wiederöffnen** einer Agentenkachel setzt die **letzte Session** fort (Session-ID als Spalte
  des Agenten, sie ist Stammdatum, kein flüchtiger Status); Schaltfläche „Neue Session“.
- **Q10 — Verbindung:** **direktes HTTP**. Anmeldung wie Opencodes eigene Clients: HTTP-Basic mit
  Benutzer `opencode` und dem Passwort aus der Discovery-Datei `<state>/service.json` (`<state>` über
  das dokumentierte `opencode debug paths state`). Das ist die **einzige undokumentierte Stelle** —
  gekapselt an einer Stelle, `401` bricht laut mit eigener Meldung ab. Issue an Opencode, die
  Anmeldung zu dokumentieren: https://github.com/anomalyco/opencode/issues/51724.
  `opencode api` ist kein Ersatz: es reicht den Event-Stream nicht durch.

## Gemessene API-Fakten (2.0.18)

- `GET /api/info` → `{version, pid, urls, paths}`.
- `POST /api/session` → `{data: Session.Info}`; `model: {providerID, id, variant?}`,
  `permissions: [{action, resource, effect: allow|deny|ask}]` (letzte passende Regel gewinnt,
  ohne Treffer `ask`; `*` passt auf alles, auch in `action`).
- `POST /api/session/{id}/prompt` `{text}` → nimmt die Eingabe dauerhaft an und startet die Schleife.
- `GET /api/event` (SSE, `data: {id, type, created, location, data}`, `: heartbeat`). Für einen Lauf
  relevant: `session.execution.started|succeeded|failed` (`data.sessionID`), `permission.asked`
  (`data: {id, sessionID, action, resources, save}`), `session.tool.*`, `session.step.*`.
  Die Event-Daten stehen **nicht** in der OpenAPI-Beschreibung (dort nur „JSON-String“) — gemessen.
- `POST /api/session/{id}/permission/{requestID}/reply` `{decision: once|always|reject}`.
- `GET /api/session/{id}/message` (`order`, `limit`, `cursor`) → Assistenten-Nachrichten mit
  `content` (Teile `text`/`reasoning`/`tool`), `finish` (`stop|length|tool-calls|content-filter|error|unknown`),
  `cost`, `tokens {input, output, reasoning, cache{read,write}}`; ein Lauf endet mit einem
  `idle`-Eintrag `outcome: succeeded|failed|interrupted`.
- `POST /api/session/{id}/interrupt` bricht eine laufende Ausführung ab.
- Die Plugin-API existiert (`ctx.event.subscribe`, `ctx.tool.transform`, …), wird aber nach Q2 nicht gebraucht.

## Checkpoints

### OC1 — API-Client, Skills und Chat darauf (erledigt)

Neues Crate **`axiomata-opencode`** (ohne Abhängigkeit zu `axiomata-core`, wie `axiomata-terminal`/
`axiomata-files`), weil Kern (Skills, Chat) und `axiomata-ide` (OC2/OC3) es beide brauchen:

- `service`: Discovery (`opencode debug paths state` → `service.json`), Anmeldung, `GET /api/info`,
  Versionsprüfung (Q8), Dienst starten (Q6), kleine JSON-Helfer über `reqwest` (ist über `ollama-rs`
  schon im Baum; ohne TLS, ohne neue Features).
- `events`: SSE-Leser für `/api/event`, gefiltert auf eine Session.
- `session`: anlegen, Prompt, Nachrichten lesen (mit Paging), Berechtigung beantworten, abbrechen,
  Modell wechseln.
- `turn`: ein Lauf = Stream öffnen → Prompt → auf `session.execution.succeeded|failed` warten,
  Berechtigungsfragen nach Q7 beantworten, Zeitlimit → `interrupt` → Nachrichten seit dem Prompt
  lesen: Antworttext, Tokens, Kosten, Turns, `finish`, `idle.outcome`.

`axiomata-core::agents::opencode` benutzt das Crate statt `opencode run`: Skill-Lauf = neue Session
in `cwd` mit Titel; Chat = neue oder fortgesetzte Session (bei geändertem Chat-Modell vorher
`switchModel`). `AgentRunResult`/`ChatReply`, Kosten-Cap und `runs`-Tabelle bleiben unverändert.
Die Nebenläufigkeitsgrenze (`agent_slots`) bleibt. Entfällt: Stream-Parser, `session export`,
Kind-Umgebung für `opencode run`.

Prüfung: Unit-Tests (SSE-Parser, Nachrichten-Auswertung, Berechtigungs-Politik, Modell-ID-Zerlegung),
Integrationstest gegen einen Mock-HTTP-Server; live: `mail-digest`, `reminders-digest`,
`calendar-digest`, ein Chat-Turn mit Fortsetzung, ein Lauf ohne `auto_approve_tools`.
Abhängigkeitsprüfung (`Cargo.toml` ändert sich), Sicherheitsprüfung (Anmeldedaten, lokaler HTTP-Client).

Umgesetzt und geprüft (2026-09-27):
- Crate `axiomata-opencode` (`service`, `events`, `session`, `turn`); `agents/opencode.rs` darauf umgestellt,
  `AgentRequest.env`/`ChatRequest.env`, `strip_ansi`, `into_string_lossy` entfernt.
- Gemessen beim Bau: eine abgelehnte Berechtigung **unterbricht** die Ausführung
  (`session.execution.interrupted`, danach kein `idle`-Eintrag); `question` wird für unbeaufsichtigte
  Läufe immer verboten.
- Ein ungültiger Arbeitsordner scheitert vor dem Dienst (gleiche `ENOENT`/`ENOTDIR` wie früher der
  Prozessstart); unter `cfg(test)` verbindet das Backend nie — ein erster Testlauf hatte sonst Sessions im
  echten Dienst angelegt (kostenlos, aufgeräumt).
- Sicherheitsprüfung: HIGH „ungebremstes Puffern“ behoben (Antworten ≤ 32 MiB, Event-Zeilen/-Nutzlast
  ≤ 16 MiB); LOW: `service.json` muss dem Benutzer gehören und darf für andere nicht schreibbar sein;
  Doku stellt klar, dass „nichts vorab erlaubt“ nur für die Session-Regeln gilt, nicht für die globale
  Opencode-Config. Abhängigkeitsprüfung ohne Befund (keine neuen Crates, kein TLS-Stack).
- Tests: Unit-Tests, `tests/mock.rs` (handgeschriebener Loopback-Server + Fake-`opencode`: Discovery,
  Anmeldung, 401, falsche Hauptversion, Dienst starten, nicht erreichbar, Lauf, abgelehnte Berechtigung),
  `tests/live.rs` (`#[ignore]`, echter Dienst mit lokalem Modell). Live: Chat mit Fortsetzung,
  `reminders-digest`, `calendar-digest`, `mail-digest`.

### OC2 — IDE-Sessions über die API (erledigt)

Agentenstart für Opencode: Session per API im Worktree (Modell, Agent, Q7-Regeln, Planungs-Anweisung
als Session-Anweisung), dann `opencode --session <id>` in der PTY-Kachel. Session-ID als Spalte am
Agenten (Migration), Fortsetzen beim Wiederöffnen, „Neue Session“ (Q9). `--model` am TUI-Befehl entfällt.

Umgesetzt (2026-09-27): Migration 13 (`ide_agents.opencode_session`, `SCHEMA_SQL_V5`);
`Agent::resolve_command` hängt für Opencode kein `--model` mehr an; `axiomata_core::ide_start::start_agent`
ist der eine Startweg für Dashboard und CLI (`provision::prepare`, dann `agents::opencode::ide_session`:
fortsetzen, wenn der Dienst die Session im selben Ordner noch hat — Modell ggf. umschalten —, sonst neu
anlegen; Regeln `shell: git push`/`git push *` → `deny`). „New session“ in der Agentenkachel
(`ide_agent_new_session`, CLI `ide agents new-session`). **Abweichung vom Plan:** keine
Planungs-Anweisung für Opencode — der Text verlangt ein Todo-Werkzeug, das v2 nicht hat, und der
Endpunkt für Session-Anweisungen ist `experimental`; die Anweisung kommt mit der Schrittliste über den
eigenen MCP-Server (Q3). Live geprüft per CLI: Anlegen, Fortsetzen, „New session“, Ersatz einer
verschwundenen Session (404), TUI öffnet `--session` mit dem Modell der Session. Offen: Live-Test in
`cargo tauri dev` durch den Owner.

### OC3 — Status aus dem Event-Stream (erledigt)

Ein Beobachter pro App über `/api/event` → Statuswort je Agent (`working` bei
`session.execution.started`, `waiting` bei `permission.asked`/Formular, `idle` bei
`succeeded|failed`), Neu-Lesen nach Wiederverbinden. Plugin-Vorlage, `OPENCODE_CONFIG_DIR` und der
Opencode-Teil des Dateikanals unter `~/.axiomata/agent-events/` fliegen raus; Claude Code bleibt beim
Dateikanal. Plan-Tab für Opencode: Status + Antwort des Plan-Agenten (Q3).

Umgesetzt (2026-09-28): `axiomata-opencode::status` (`Tracker`, ein reiner Zustandsautomat;
`Service::active_sessions`/`session_snapshot`/`find`), `axiomata_core::ide_status` (ein Beobachter pro
Prozess, gestartet vom ersten `ide_agent_states`, startet den Dienst nie selbst; Stream zuerst öffnen,
dann alle Agenten-Sessions neu einlesen; Wiederverbinden nach 2 s bis 30 s; `overlay` für Opencode-Agenten
mit generiertem Befehl, `overlay_once` für die CLI). Gemessen beim Bau: eine Rückfrage des Agenten ist
`form.created` (Session unter `data.form.sessionID`), ihr Abbruch `form.cancelled`. Plan-Tab: die
**letzte** Antwort des Plan-Agenten eines Turns (ein Live-Test zeigte zwei Plan-Schritte in einem Turn —
das Neu-Einlesen nimmt ebenfalls die letzte). Plugin, `OPENCODE_CONFIG_DIR`, `plan.json`/`plan-mode.md`
und der Opencode-Teil der Planungs-Anweisung sind entfernt; `reset` löscht den alten Plugin-Ordner. Ein
Opencode-Agent meldet nie `ended` (der Dienst weiß nicht, wann eine TUI endet). Live geprüft:
`ide agents status` über den Dienst; `tests/live.rs` folgt einem Plan-Agenten-Turn (working → idle) und
vergleicht den Plan mit dem Neu-Einlesen.

### OC4 — Anweisungen und Doku (erledigt)

`AGENTS.md` als Quelle dieses Repos, `CLAUDE.md` mit `@AGENTS.md`; `docs/architecture.md`,
`CLAUDE.md`-Zusammenfassung, Memory-Notizen (u. a. die vorgemerkte „CLAUDE.md → AGENTS.md“-Notiz).

Umgesetzt (2026-09-28): `AGENTS.md` trägt jetzt, was jeder Coding-Agent braucht (Stand, Befehle, Fallen —
der bisherige Inhalt der `CLAUDE.md`, harness-neutral eingeleitet; die alte, auf M3 stehengebliebene
`AGENTS.md` ist ersetzt). `CLAUDE.md` importiert sie per `@AGENTS.md` und behält nur, was Claude Code allein
betrifft (die Sub-Agenten-Regeln). Die Vault-Seite (Memory-Router, `CLAUDE.md` im Workspace) bleibt ein
eigenes Vorhaben (Owner-Notiz „CLAUDE.md → AGENTS.md“).
