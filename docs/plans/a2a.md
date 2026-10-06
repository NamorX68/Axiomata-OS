# Plan: Agent-zu-Agent-Kommunikation (M7.5)

Status: **Bauplan vom Owner freigegeben (2026-10-03). CP-A1 bis CP-A5, CP-A6a, CP-A6b und CP-A6c gebaut (2026-10-04/05); CP-A7 (Planer + Flow-Modus) und CP-A8a–c (Plan läuft von selbst, Plan übernehmen, Kostenbuch/Limits/Eskalation/Grenzen für Vorschläge) gebaut 2026-10-05/06, CP-A9 (Flow: Reiter Agents mit Live-Kacheln und Nachrichten) und CP-A10 (Flowansicht, Graph) gebaut 2026-10-06 — der Bauplan CP-A1…A10 ist damit durch; offen sind die Mac-Tests und das Feinschleifen von UI/UX.** CP-A2 weicht in einem Punkt von A18 ab: `verify_card` bleibt unverändert (Fertig-Spalte); das Abzeichnen in der Review-Spalte läuft über `review_verdict` (Verschieben nach Fertig und Signatur in einer Transaktion), damit eine Signatur nie auf einer Karte liegt, die noch als „in Arbeit“ zählt.
Grundlage ist `agentic-ide.md` (E3, M7.5, §9); dieser Plan hält die in der Runde getroffenen Entscheidungen fest und ersetzt dort,
wo er etwas anders sagt, die älteren Aussagen.

## Entscheidungen

- **A1 — MCP als Transport, A2A als Datenmodell, A2A-Fassade später (Owner, 2026-10-03).** Claude Code und Opencode sprechen MCP, nicht A2A
  (nach heutigem Wissen); der Postfach-Kern ist deshalb unabhängig vom Transport und lehnt sich in Begriffen und Zuständen an den
  A2A-Standard (Linux Foundation, v1.0) an: Nachrichten aus Teilen (Text, Dateien, Daten), Aufgaben mit den Zuständen `submitted`,
  `working`, `input-required`, `completed`, `failed`, `canceled`, Ergebnisse als Artefakte. Später kann ein A2A-Endpunkt (Agent Card,
  JSON-RPC über HTTP, SSE) davorgesetzt werden — für Agenten, die A2A sprechen, oder außerhalb des Rechners.

- **A2 — Koordination: der Owner, über ein Planungsgespräch (Owner, 2026-10-03).** Kein fester Chef unter den Agenten. Der Owner plant mit einem
  *Planungsagenten* (grillt, schreibt den Plan) und dieser legt daraus Karten im Brett an: Titel, Beschreibung, Abnahmekriterien,
  Abhängigkeiten, Rollenhinweis. Danach arbeitet nicht der Planungsagent weiter.
- **A3 — Ablauf (Owner bestätigt):** Plan → **Freigabe durch den Owner** → Karte **starten** (ein Klick startet einen *neuen* Agenten in eigenem
  Worktree mit der Karte als Auftrag; er greift sie per `claim_task`) → Agent meldet „fertig" → **Review-Agent startet automatisch** und setzt
  „geprüft" oder gibt zurück → **Take-over durch den Owner** (Squash, nie ein Push). Die Zuweisung ist der Start-Prompt, keine Nachricht an
  einen laufenden Agenten; Nachrichten zwischen laufenden Agenten (Rückfragen, Review-Anmerkungen) laufen über das Postfach.
- **A4 — Starten von Hand oder automatisch (Owner bestätigt):** Standard von Hand; je Plan ein Schalter „automatisch bis N Agenten gleichzeitig",
  der bereite Karten (alle Abhängigkeiten erledigt) selbst startet. Der Review-Agent startet immer automatisch.

- **A5 — Drei Ebenen: Engine, Agent, Sitzung (Owner, 2026-10-03).**
  *Engine* = Harness + Modell + Provider (das, was heute im Studio „Agent" heißt: z. B. „Claude Code · Opus", „Opencode · DeepSeek",
  „Opencode · lokales Qwen"); sie trägt Kosten, Erreichbarkeit, Kontextgröße. *Agent* = eine **Rolle**: Name, Aufgabenart und Stufe (leicht, mittel,
  schwer), Arbeitsanweisung, Rechte, Standard-Engine und Ausweich-Engines (z. B. „Umsetzer leicht", „Umsetzer schwer", „Reviewer", „Planer").
  *Sitzung* = ein gestarteter Agent mit eigenem Worktree und Branch für genau eine Karte. **Karten verweisen auf einen Agenten, nie auf eine Engine**;
  ein Engine-Wechsel geschieht an einer Stelle. Der **Planer** ist ein Agent ohne feste Engine (der Owner wählt das Modell bei jedem Start).
  *Eskalation:* gibt der Review eine Karte zweimal zurück oder bricht die Sitzung ab, geht sie automatisch an den nächst stärkeren Umsetzer, mit
  dem bisherigen Verlauf als Kontext; die Stufe ist ein Feld der Karte (der Planer setzt sie). *Review-Regel:* der Reviewer läuft immer mit einer
  **anderen Engine** als der Umsetzer derselben Karte, das Studio prüft das beim Start. *Ablage:* eine Datei je Agent wie bei Skills,
  `~/.axiomata/agents/<name>/AGENT.md` (Frontmatter: Rolle, Stufe, Engine, Ausweich-Engines, Rechte, Limits; Text = Arbeitsanweisung), optional
  überschrieben durch `.axiomata/agents/` im Projekt, das wie `tasks.json` erst nach Bestätigung gilt; Engines in den Einstellungen. Die heutigen
  Studio-„Agents" werden zu Engines mit einer einfachen Rolle migriert.
- **A5a — Wer ordnet einer Karte einen Agenten zu?** Ein **Zuweiser als austauschbare Schnittstelle**. Standard ist der **Planungsagent selbst**: er kennt
  den Katalog, schlägt beim Anlegen einer Karte den Agenten (die Rolle) vor und begründet kurz; der Owner bestätigt beim Freigeben des Plans (Grund:
  ein Agent ist bei jedem Nutzer da, JEV verlangt Konto, Schlüssel, Guthaben). Der Automatikmodus (A4) startet nur Karten, deren Agent bestätigt
  ist oder die eine feste Regel bestimmt (z. B. „Review"). **Optional** ein Zuweiser mit kalibrierter Sicherheit: JEV (`jev-einsatz.md`, Version pinnen)
  oder ein lokales offenes Modell, wenn die Hardware reicht (Laya: ModernBERT-large; Nimble: Qwen3.5-9B + LoRA; Kev u. a., genannt in einem
  DataCamp-Vergleich offener JEV-Alternativen) — Einstellung, kein Muss. Die Wahl zwischen wenigen Rollen statt jeder Harness-Modell-Kombination
  macht die Entscheidung klein. *Offen:* Kev und die übrigen offenen Modelle sind nicht geprüft (Größe, Lizenz, Laufzeit).
- **A6 — Der zweite Modus des Studios heißt „Flow" (Owner, 2026-10-03):** **Editor | Flow** statt Editor | Agents. Der Rail-Eintrag „Agents" bleibt für den
  Katalog der Rollen und die Liste der Sitzungen. Im Flow steht eine **Graph-Ansicht des Plans** (in Anlehnung an Airflow): Knoten = Karten, Linien =
  „braucht zuerst", Farbe = Zustand (bereit, läuft, im Review, zurückgegeben, geprüft, übernommen), Symbol = Agent und Sitzung. Die **Abhängigkeiten sind
  azyklisch**; dass eine Karte zurückgelegt wird (Review gibt zurück, Eskalation), ist ein **Zustandswechsel am Knoten**, keine Linie. Ansicht mit
  Aktionen (Starten, Sitzung öffnen, Diff, Übernehmen, hochstufen), **kein Kabel-Editor** (E3), Anordnung automatisch. Gebaut **nach** dem Kern
  (Postfach, Karten starten, Review), vorher wäre sie leer.

- **A7 — Was läuft von allein, was braucht ein Ja (Owner, 2026-10-03: „an manchen Stellen lockerer"):**
  Feste Tore: der Owner gibt den **Plan frei** und **übernimmt** am Ende (Take-over, nie ein Push). Dazwischen:
  *Karten anlegen:* ein Agent darf Karten **bestimmter Arten** anlegen und starten lassen, ohne zu fragen — der Owner nennt als Beispiele **Testkarten**
  (ein Agent legt eine Testkarte an) und **Doku-Karten** (ein anderer legt eine an). Technisch: je Agent ein Feld `creates: [test, doc, …]` in
  `AGENT.md`; Karten dieser Arten starten im Automatikmodus (A4) von selbst, andere Arten legt der Agent an, sie landen aber in „Vorschlag" und warten
  auf ein Ja. Zum Schutz vor Lawinen: Tiefe (eine von einem Agenten angelegte Karte darf höchstens bis Tiefe 2 weitere anlegen) und Anzahl je
  Ausgangskarte begrenzt (Vorschlag: 3), außerdem gilt immer das Kostenlimit.
  *Eskalation (leicht → schwer):* automatisch, mit Meldung im Flow; nur nach dem Scheitern der letzten Stufe wartet die Karte auf den Owner.
  *Befehle der Agenten:* regeln die Rechte je Agent wie bei den Harnesses heute. *Nachrichten zwischen Agenten:* ohne Bestätigung, im Team-Panel
  einsehbar. *Kosten:* ein Limit je Plan oder Tag; ist es erreicht, hält die Automatik an und fragt. *Deutung zu prüfen:* „Testfahrten" wurde als
  „Testkarten" gelesen.

- **A8 — Zustellung an laufende Agenten und Schutz vor Schleifen (Owner, 2026-10-03):**
  MCP ist Anfrage-Antwort, der Server kann einen Terminal-Agenten nicht von sich aus ansprechen. Deshalb drei Wege, in dieser Reihenfolge gebaut:
  (1) **Selbst abfragen:** die `AGENT.md` weist den Agenten an, das Postfach (`read_inbox`) am Anfang, zwischen den Schritten und vor dem Fertigmelden
  zu lesen — überall lauffähig, kann aber vergessen werden. (2) **Anstupsen, wenn der Agent wartet:** der Status (arbeitet / wartet auf Eingabe, CP6)
  ist die Zustellbedingung; wartet der Agent, tippt das Studio eine kurze Zeile in sein Terminal („Neue Nachricht von @reviewer, bitte read_inbox"),
  aber nie, während der Owner in genau diesem Pane tippt. (3) **Hinweis während der Arbeit (später):** Claude Code Hooks, Opencode Plugins (für den
  Status schon im Einsatz) hängen bei Werkzeugaufrufen einen Hinweis an — **vor dem Bau gegen die aktuelle Dokumentation zu prüfen.**
  *Schleifenschutz:* jede Nachricht trägt einen Zähler der Weitergaben in einer Kette; nach **6** Schritten stoppt die Kette und fragt den Owner;
  höchstens **20** Nachrichten je Agent und Karte; auf eine reine Bestätigung ohne Inhalt wird nicht geantwortet. (Zahlen sind ein Anfang, einstellbar.)

- **A9 — Kosten und Limits (Owner, 2026-10-03):** drei Ebenen. **Pro Sitzung** (Kosten oder Schritte einer Karte; einstellbar im Agenten, `AGENT.md`,
  „leicht" klein, „schwer" größer; erreicht → Sitzung stoppt, Karte wird zurückgelegt und wenn möglich eskaliert). **Pro Plan** (Gesamtlimit; erreicht →
  Automatik hält an und fragt). **Pro Tag/Monat** (harter Deckel, die Reißleine). Gemessen wird aus Token mal Preis nach der vorhandenen Preistabelle
  (`config.agents.costs`, `spend.rs`); **Abonnement-Engines** (z. B. Claude Code über das Konto) werden in **Token** begrenzt, nicht in Geld. Die Werte
  stehen in den Einstellungen als Standard für alle Pläne und lassen sich je Plan überschreiben. Ist ein Limit erreicht, darf eine laufende Sitzung ihren
  **aktuellen Schritt beenden** und stoppt dann, damit nichts halb geschrieben liegen bleibt.

- **A10 — MCP-Eintrag je Harness (Owner, 2026-10-03; löst F4 aus `agentic-ide.md`):** der Eintrag wird **pro Worktree** geschrieben (Claude Code:
  `.mcp.json` bzw. `.claude/settings.local.json`; Opencode: `opencode.json` im Projekt), **nie** in `~/.config/opencode/opencode.json` oder das
  Benutzerprofil. Der Server bekommt die Identität des Agenten über Umgebungsvariablen beim Start, der Absender wird vom Server gestempelt. **Sichtbar und
  bestätigt:** beim ersten Mal je Projekt zeigt das Studio, was es einträgt; die Bestätigung gilt für diesen Inhalt (Hash), bis er sich ändert. Wird ein
  Worktree entfernt, verschwindet der Eintrag mit. Der Server ist **nur lokal** erreichbar (Pipes der gestarteten Prozesse, **kein Netzwerkport**);
  ein Planungsagent außerhalb des Studios ist damit vorerst nicht vorgesehen. *Beim Bau nachzuschlagen:* die genauen Orte und Formate der aktuellen
  Versionen von Claude Code und Opencode.

- **A11 — Eigenständiges Studio: das Brett liegt in der Datenbank der jeweiligen App (Owner, 2026-10-03).** Die Haupt-App behält ihr Brett in der Axiomata-
  Datenbank, eine eigenständige Studio-App bekäme ihre eigene kleine Datenbank in ihrem eigenen Ordner. Die Kern-Crates (`axiomata-board`,
  `axiomata-ide` mit Postfach und MCP-Server) nehmen Pfade und liefern Daten, persistiert wird außen — ein späteres Brett im Projekt (Datei) wäre dann
  ein Austausch der Speicherung. Keine Synchronisation zwischen den Apps, solange es die zweite nicht gibt.

### Runde 2026-10-04 — die Checkpoints CP-A2 bis CP-A10 durchgegrillt (Owner: „passt so", alles wie vorgeschlagen)

**Karte und Brett (CP-A2)**

- **A12 — Zustand wird abgeleitet, die Spalte trägt ihn.** Der Grundsatz des Bretts bleibt: der Status einer Karte steht nur in ihrer Spalte. Der A2A-Zustand
  (`proposed`, `ready`/`blocked`, `working`, `input_required`, `in_review`, `done`, `verified`, `taken_over`, `failed`, `canceled`) wird in Rust aus Spalte, Claim,
  Prüfsignatur und wenigen Feldern berechnet und mit der Karte mitgeschickt (wie `root_exists`). Neue Felder der Karte nur dort, wo ein Zustand nicht aus der
  Spalte folgt: `returned_count`, `input_required` (Text), `taken_over_at`, `failed_at`, `canceled_at`. „Unterbrochen" (Claim ohne lebende Sitzung) kennt nur der
  Core, nicht das Board-Crate.
- **A13 — Spaltenrollen statt neuer Status.** Eine Spalte behält ihren Status (offen/in Arbeit/fertig) und bekommt optional eine **Rolle** `stage`: `proposal`
  (Status offen) oder `review` (Status in Arbeit). Das vermeidet den Neuaufbau der eingefrorenen Tabelle (`CHECK` auf `maps_to_status`). **Standard bei allen Boards**
  (Owner): neue Boards starten mit Vorschlag · Offen · In Arbeit · **Review** · Fertig; bestehende Boards bekommen einmalig eine Spalte „Review" (zwischen letzter
  „In Arbeit"- und erster „Fertig"-Spalte; eine schon vorhandene Review-Spalte wird genommen) und eine Spalte „Vorschlag" vor der ersten. Die Oberfläche blendet
  „Vorschlag" aus, solange sie leer ist. Vorhandene Spalten und Karten bleiben unberührt.
- **A14 — Plan als Objekt.** Tabelle `plans` (Brett, Name, Zustand Entwurf/freigegeben/abgeschlossen, Auto-Schalter mit N, Limits) und `cards.plan_id` (optional;
  Karten ohne Plan bleiben normale Karten). Das Brett bleibt die Ansicht, der Plan die Einheit für Freigabe, Automatik, Limits und Graph.
- **A15 — Felder der Karte:** `agent` (Rollen-Slug, Absicht; der Planer schlägt vor, die Freigabe bestätigt), `agent_reason` (kurze Begründung, A5a), `tier`,
  `kind` (freier Slug wie bei Rollen), `acceptance` (Abnahmekriterien als Markdown, kein Schema). `assignee` und `claimed_by` bleiben: `claimed_by` ist die
  **Sitzung** (`agent:<Sitzungsname>`), `agent` die Absicht.
- **A16 — Abhängigkeiten** (`card_deps`): nur innerhalb eines Plans, azyklisch (Rust prüft in einer Transaktion beim Anlegen). **Bereit** heißt: alle Vorgänger
  **geprüft**; die Folgekarte startet auf dem **Zweig des Vorgängers**. Löschen entfernt die Kante, Archivieren einer geprüften Karte zählt als geprüft, **abgebrochene oder
  gescheiterte Vorgänger blockieren sichtbar** („wartet auf Karte #12, gescheitert"), bis die Kante gelöst oder die Karte neu gestartet wird.
- **A17 — Vorschlag und Plan-Freigabe sind ein Konzept.** Der Planer legt seine Karten in der Spalte **Vorschlag** an (Plan im Entwurf); die **Freigabe** schiebt sie
  gesammelt nach Offen (Plan „freigegeben"). Karten, die ein Agent außerhalb seiner erlaubten Arten anlegt (A7), landen ebenfalls dort, mit dem Plan der
  Ausgangskarte. Einzelne Vorschläge lassen sich einzeln freigeben oder verwerfen.
- **A18 — Review-Fluss am Brett (Owner-Beispiel: „Agent, review alles in der Review-Spalte").** `report_done` schiebt die Karte nach Review; `review_verdict` „gut" prüft
  (`verify_card` verlangt künftig eine Karte in der Review-Spalte, für Boards ohne Rolle weiter die Fertig-Spalte) und schiebt nach Fertig, „zurück" geht nach In
  Arbeit mit `returned_count` + 1. Zieht ein Akteur eine **unclaimte** Karte in die Review-Spalte, wird sie für ihn geclaimt („wer abgibt, hat gearbeitet"), damit ein
  anderer Akteur sie prüfen kann (der `CHECK verified_by <> claimed_by` bleibt unangetastet). **Ziehen ist für den Owner immer frei** *(umgesetzt als: nur ein `human:`-Akteur darf Karten frei verschieben; ein `agent:`-Akteur wird abgewiesen, siehe A39)*, auch aus Review nach Fertig ohne
  Abzeichnen; die Signatur setzt nur `review_verdict`. Agenten bewegen Karten nur über `claim_task`, `report_done`, `review_verdict`.
- **A19 — Verlauf der Karte (`card_events`).** Nur anfügende Tabelle (Karte, Zeit, Akteur, Art, Text): Review-Anmerkungen, Eskalationen, Limit-Stopps, Rückfragen,
  Zustandswechsel. Quelle für den Kontext bei Eskalation und das Team-Panel; der Markdown-Spiegel hängt die letzten Einträge an *(noch nicht umgesetzt: CP-A2 zeigt im Spiegel die Zustandsmarken der Karte, die Einträge folgen mit dem Team-Panel in CP-A9)*.

**Ablauf (CP-A6, CP-A8)**

- **A20 — Die App claimt beim Start** der Karte (atomar, `agent:<Sitzungsname>`); `claim_task` des Agenten ist danach nur eine idempotente Bestätigung. Schlägt der
  Claim fehl, startet keine Sitzung.
- **A21 — Reviewer:** die Rolle mit `kind = review` (bei mehreren die höchste passende Stufe), immer auf einer **anderen Engine** als der Umsetzer; gibt es keine,
  fragt das Studio (kein stilles Weiterlaufen mit derselben Engine). Er bekommt einen **eigenen Worktree**, detached auf dem Stand des Umsetzers, und schreibt nie
  auf dessen Zweig.
- **A22 — Aufräumen nach der Übernahme:** Sitzungen werden beendet (Panes bleiben zum Nachlesen offen), Worktree und Zweig des Umsetzers sowie der Reviewer-Worktree
  werden **erst entfernt, wenn alle aufbauenden Folgekarten übernommen oder gelöst sind**; die Karte bekommt `taken_over_at` und wird archiviert. Das Studio pusht nie.
- **A23 — Unterbrochene Karten** (Sitzung weg, z. B. App beendet): Anzeige „unterbrochen", nie automatisch neu starten. Aktionen **Fortsetzen** (neue Sitzung im
  selben Worktree/Zweig, Hinweis „du wurdest unterbrochen, prüfe den Stand") und **Freigeben** (Claim lösen, zurück nach Offen). Beim Start der App wird das einmal geprüft.
- **A24 — Mitgelieferte Rollen:** `allrounder`, `planner`, `reviewer` (gesät, wenn sie fehlen). Der Reviewer hat keine feste Engine und fragt beim ersten Gebrauch. Die
  Anweisungstexte entstehen zu CP-A6/CP-A7 und werden dem Owner vor dem Commit vorgelegt.
- **A25 — Automatik:** N gilt **je Plan**, dazu eine globale Obergrenze (Einstellung, Standard 3 gleichzeitige Sitzungen); Reihenfolge: Position in Offen, oben zuerst;
  **die Automatik hält an und fragt**, sobald eine Karte scheitert (Eskalation ausgeschöpft) oder ein Limit greift. Fortsetzen mit einem Klick.
- **A26 — Eskalation:** zuerst die **Ausweich-Engines** derselben Rolle (Engine nicht erreichbar/Limit); danach nach zwei Rückgaben oder Abbruch die Rolle mit
  gleicher `kind` und **nächsthöherer Stufe**, sonst wartet die Karte auf den Owner. Die neue Sitzung übernimmt Worktree, Zweig und Claim und bekommt die `card_events` als
  Kontext; die alte endet.
- **A27 — Limits, Startwerte** (Studio-Inspektor, Reiter Agents; je Rolle und Plan überschreibbar): je Sitzung leicht 60 Schritte / 0,50 $ / 400 k Token, mittel 120 / 2 $ /
  1,5 M, schwer 250 / 6 $ / 4 M; je Plan 15 $ oder 6 M Token; pro Tag 20 $ für Studio-Sitzungen mit Abrechnung nach Token, **getrennt** vom Tageslimit der Skills/des Chats.
  Token zählen Eingabe, Ausgabe und Cache-Aufbau, **nicht** Cache-Lesezugriffe.

**Transport und Harnesses (CP-A3 bis CP-A5)**

- **A28 — MCP-Server = `axiomata-cli mcp-serve`**, arbeitet direkt auf der Datenbank (WAL + Wartezeit; Vorbild: das CLI schreibt heute genauso in laufende Apps) und ruft die
  Core-Funktionen auf (inklusive Markdown-Spiegel). Der Pfad wird **bei jedem Start neu** geschrieben, überschreibbar per Umgebungsvariable; mit dem App-Bundle wird es
  der Sidecar darin. Zusätzliche Werkzeuge: `list_cards`, `get_card` (und die Werkzeuge aus CP-A4).
- **A29 — Postfach:** Adressen **Sitzung**, **Rolle** (aufgelöst auf die laufenden Sitzungen dieser Rolle) und **Owner**, keine Rundrufe. Persistenz in der Haupt-Datenbank im
  IDE-Crate hinter einer Schnittstelle (A11), Teile als JSON, optional `card_id`. Nachrichten an eine **beendete** Sitzung kommen als nicht zustellbar zum Absender zurück und
  erscheinen beim Owner. Aufbewahrung: bis die Karte übernommen/abgebrochen ist plus 14 Tage, Nachrichten ohne Karte 30 Tage; Aufräumen beim Start.
- **A30 — MCP-Eintrag bei Claude Code** (ändert A10): eine **app-eigene Datei** (`~/.axiomata/agent-events/<id>/mcp.json`) und pro Lauf `--mcp-config` plus `--settings` mit
  `enabledMcpjsonServers`; **nichts im Worktree** (eine Datei dort würde im Take-over landen) und nichts Globales. Beim Fortsetzen neu übergeben. Die Sichtbarkeit aus A10
  bleibt (beim ersten Mal je Projekt zeigen, Bestätigung per Hash). Ob `--mcp-config` den Erlaubnisdialog überspringt, ist zuerst live zu prüfen.
- **A31 — Opencode und universeller Rückfall** *(Spike erledigt, siehe „CP-A5 im Detail“: dynamische Registrierung statt `opencode.json`)*: zu Beginn von CP-A5 ein **Spike** mit echtem Worktree (`opencode.json` im Worktree gegen `PUT /api/experimental/mcp/axiomata`);
  unabhängig davon ein **CLI-Spiegel** der Werkzeuge (`axiomata-cli agent send|inbox|claim|report|verdict|card …`) für jedes Harness mit Shell.
- **A32 — Zustellung (präzisiert A8):** Opencode über die API (`POST /api/session/{id}/prompt`, `delivery: queue`), Claude Code während der Arbeit über einen
  `PostToolUse`-Hook (`additionalContext`), wartend nur per Tippen ins Terminal (nie, während der Owner dort schreibt). Hooks kommen damit in **CP-A5**. „Channels" von
  Claude Code (Vorschau, Entwickler-Flagge, Warndialog) werden nicht verwendet.
- **A33 — Kosten messen und durchsetzen:** Opencode über die Token der API-Nachrichten, Claude Code durch tolerantes Summieren des Sitzungs-JSONL (`--session-id` beim Start
  festgelegt; fehlende Felder zählen 0; Test mit Beispieldatei). Wird nichts gefunden: „Verbrauch unbekannt" und Rückfall auf **Schritte** (über `PostToolUse` zählbar). Durchsetzen:
  Claude Code über einen `PreToolUse`-Hook, der nach Erreichen weitere Aufrufe ablehnt (der laufende Schritt endet sauber, A9), Opencode über die API.
- **A34 — Rechte unbeaufsichtigter Karten-Sitzungen:** Claude Code `--permission-mode acceptEdits` plus `--allowedTools` aus der Rolle und die `mcp__axiomata__*`-Werkzeuge einzeln
  aufgelistet; alles andere bleibt eine Rückfrage, die als **input-required** an der Karte erscheint. Opencode die entsprechende Regel der Sitzung (nicht `--auto`).
  **`bypassPermissions` nie**, auch nicht per Rolle.
- **A35 — Start-Prompt:** ein kurzer fester Satz („Du bist Sitzung `<name>`, Rolle `<rolle>`, deine Karte ist `#<id>`. Lies sie mit `get_card` und arbeite danach; lies zuerst
  dein Postfach."); Karte und Abnahmekriterien holt der Agent per `get_card`, die Rollen-Anweisung geht bei Claude Code über `--append-system-prompt-file` (Datei im app-eigenen
  Ordner), bei Opencode als erste Nachricht über die API. Kein Kartentext in einer Shell-Zeile.

**Härtung nach Review und Security-Audit von CP-A2 (2026-10-04)**

- **A39 — Wer handeln darf, wird im Store und in der Sitzung durchgesetzt, nicht dem Aufrufer geglaubt.** Der Store verlangt für die Tore des Owners einen `human:`-Akteur
  (`approve_plan`, `approve_proposal`, `mark_taken_over`, `reopen_card`), lässt Fragen, Absagen und Scheitern nur den Halter oder den Owner an einer Karte setzen und
  weist freies Verschieben durch einen `agent:`-Akteur ab (Agenten bewegen Karten nur über `report_done` und `review_verdict`). Die CLI leitet den Akteur in einer
  Agenten-Sitzung aus der Umgebung ab (`AXIOMATA_AGENT_ID`/`_NAME`, `axiomata_core::session`) und lässt kein `--actor` zu, das davon abweicht; die Befehle des
  Owners (Brett anlegen/löschen/archivieren, Plan freigeben/abschließen/löschen/ändern, vorschlag annehmen, nach Fertig schieben, Übernahme, Wieder-Öffnen, einfaches
  Abzeichnen) sind dort gesperrt, und ein Agent legt Karten nur in die Vorschlag-Spalte. **Grenze:** das schützt vor Fehlern und vor Agenten, die ihre Anweisungen
  befolgen, nicht vor einem Prozess desselben Benutzers, der die Umgebung löscht oder die SQLite-Datei direkt schreibt; echte Trennung kommt mit dem Scoping je Sitzung
  in CP-A5 (der MCP-Server stempelt den Absender und bietet nur die Werkzeuge der Rolle). Offen bis dahin: die übrigen Befehle der CLI (`ide agents take-over`, Routinen,
  Skills) sind für eine Agenten-Sitzung nicht gesperrt.
- **A40 — Grenzen gegen Überlauf:** höchstens 2000 Karten je Brett, 100 Pläne je Brett, 50 Abhängigkeiten je Karte, höchstens 500 Verlaufszeilen je Abfrage; Titel und
  Labels sind einzeilig (ein Zeilenumbruch könnte im Markdown-Spiegel eine Überschrift fälschen).

**Oberfläche und Takt**

- **A36 — Flow-Modus:** leerer Zustand mit „Neuer Plan…" (Engine-Wahl für den Planer, Sitzung mit der Rolle `planner` in einem Terminal-Pane, Plan als Entwurf); Umschalter
  oben bei mehreren Plänen; Team-Panel rechts; Posteingang je Sitzung als Reiter im Pane. Die Graph-Ansicht (CP-A10) wird selbst gebaut (geschichtetes Layout in TypeScript,
  SVG, keine Bibliothek).
- **A37 — Tests ohne echte Agenten:** der MCP-Server bekommt einen In-Process-Testclient (JSON-RPC über Pipes) für komplette Abläufe (claim, Nachrichten, report_done, Review,
  Kettenzähler, Konkurrenz zweier Clients); echte Agenten nur als `#[ignore]`-Livetests und als Karten auf dem Brett für den Mac-Test.
- **A38 — Takt:** ein Commit je Checkpoint nach dem schlanken Review (ein kombinierter Sonnet-Agent, Tests inline), zusätzlich `security-auditor` bei CP-A2, CP-A4 und CP-A5; Zuschnitte
  werden vor dem Checkpoint kurz bestätigt, wenn etwas vom Plan abweicht; gepusht wird nur auf Ja des Owners.

## Bauplan (Checkpoints)

Jeder Checkpoint hinterlässt einen benutzbaren, getesteten Zustand; die reinen Crates sind hier auf der Linux-Box testbar, die Tauri-Schicht bleibt
dünn und wird am Mac geprüft. Vor jedem Commit läuft die schlanke Prüfung (ein kombinierter Review-Agent); für CP-A4 und CP-A5 ist der
`security-auditor` Pflicht (Eingaben von Agenten, Dateien des Owners).

**Scheibe 1 — ein Umsetzer und ein Reviewer, von Hand gestartet (der Kern):**

1. **CP-A1 Engines und Agenten.** Datenmodell Engine (Harness, Modell, Provider) und Agent (Rolle, Stufe, Engine, Ausweich-Engines, Rechte, Limits,
   `creates`); Laden der `AGENT.md`-Dateien (`~/.axiomata/agents/`, Projekt-Überschreibung mit Bestätigung per Hash); Migration der heutigen
   Studio-Agenten zu Engines mit einer einfachen Rolle; Einstellungen für die Engines.
2. **CP-A2 Karten erweitern.** In `axiomata-board`: Art (`kind`), Stufe, zugewiesener Agent, Abhängigkeiten (azyklisch, geprüft), Spalte „Vorschlag",
   Zustände nach dem A2A-Modell (`submitted`, `working`, `input-required`, `completed`, `failed`, `canceled`) plus „fertig/geprüft"; Brett-Oberfläche
   zeigt die neuen Felder. Migration, Markdown-Spiegel bleibt einseitig.
3. **CP-A3 Postfach-Kern** in `axiomata-ide`: Adressen, Nachrichten aus Teilen, Aufgaben, Zustellung an Zugwechseln (Status aus CP6), Kettenzähler
   (6) und Grenzen (20 je Agent und Karte), Persistenz hinter einer Schnittstelle (A11).
4. **CP-A4 MCP-Server (stdio).** `list_agents`, `send_message`, `read_inbox`, `claim_task` (Compare-and-Swap), `create_card` (nur erlaubte Arten, A7),
   `report_done`, `review_verdict`; Identität über Umgebungsvariablen, Absender wird gestempelt; Tests gegen die Crates. *security-auditor.*
5. **CP-A5 Einbindung je Harness.** Eintrag pro Worktree (Orte und Formate zuerst nachschlagen, A10), sichtbar und bestätigt, Aufräumen mit dem
   Worktree; Sitzung starten mit der Karte als Auftrag (Start-Prompt); Anstupsen eines wartenden Agenten (A8, Weg 2). *security-auditor.*
6. **CP-A6 Ablauf von Hand.** „Karte starten" (neue Sitzung, Worktree, `claim_task`), automatisches Review mit anderer Engine (A5), Rückgabe,
   Take-over als zweites Tor, Limits pro Sitzung (A9).

**Scheibe 2 — Planung und Automatik:**

7. **CP-A7 Planer.** Planungsagent mit dem Agentenkatalog als Kontext; legt Karten mit Vorschlag für den Agenten an; Freigabe-Ansicht (Karte, vorgeschlagener
   Agent, ändern); „Vorschlag"-Spalte für Karten, die ein Agent anlegt (A7).
8. **CP-A8 Automatik und Eskalation.** Modus „automatisch bis N Agenten", Start bereiter Karten (Abhängigkeiten erledigt), Eskalation leicht → schwer nach zwei
   Rückgaben (A5), Limits pro Plan und Tag/Monat (A9), Tiefe und Anzahl selbst angelegter Karten (A7).

**Scheibe 3 — Oberfläche:**

9. **CP-A9 Flow.** Umbenennung Editor | Agents → **Editor | Flow** (A6), Team-Panel (wer arbeitet woran, Nachrichten einsehbar), Inbox-Tab je Sitzung,
   Rail-„Agents" als Katalog und Sitzungsliste.
10. **CP-A10 Graph-Ansicht** des Plans (A6): Knoten, „braucht zuerst"-Linien, Zustandsfarben, Aktionen am Knoten, automatische Anordnung.

**Später, bei Bedarf:** Hinweise während der Arbeit über Hooks/Plugins (A8, Weg 3, vorher gegen die Dokumentation prüfen); optionale Zuweiser mit
kalibrierter Sicherheit (JEV, lokale offene Modelle — vorher prüfen, A5a); **A2A-Fassade** (Agent Card, JSON-RPC über HTTP, SSE; A1).

## CP-A1 im Detail (Entwurf, Owner-Antworten vom 2026-10-04)

Drei Festlegungen des Owners: **(1)** der Engine-Katalog liegt **global in der Config** (wie `agents.providers`), nicht je Projekt;
**(2)** die Migration **leitet Engines aus den vorhandenen Studio-Agenten ab** und gibt jeder Zeile die Rolle `allrounder`;
**(3)** die Oberfläche bekommt **Engines in den Einstellungen und ein Rollen-Formular** (Anlegen/Bearbeiten von `AGENT.md`).

**Befund, der den Zuschnitt prägt:** `ide_agents` ist heute faktisch schon die **Sitzung** (Worktree, Branch, Port, Basisbranch, Opencode-Session) und
trägt zugleich die Engine-Felder (Harness, Command, Modell, Env). CP-A1 trennt das: die Zeile bleibt die Sitzung und verweist auf Engine und Rolle.

- **Neues Crate `axiomata-roster`** (rein, ohne Tauri und ohne `axiomata-core`, wie `axiomata-tasks`/`axiomata-board`; A11): Typen `Engine`, `Role`
  (Stufe, Ausweich-Engines, Rechte, Limits, `creates`), Laden und Schreiben der `AGENT.md` aus einem übergebenen Verzeichnis, Prüfungen (Name als Verzeichnis,
  Engine-Verweise, Größenlimit wie bei Skills), Projekt-Überschreibung mit **Hash-Bestätigung** (Muster aus `axiomata-tasks/src/trust.rs`). Frontmatter über
  `gray_matter`, wie die Skills.
- **Engines** liegen in `axiomata-core::config` (`agents.engines`, eine Tabelle je Engine-Id) mit den Typen aus dem Roster-Crate; `validate_for_save` prüft sie.
  Migration beim Laden: fehlt die Tabelle, wird sie aus nichts erzeugt, die Ableitung aus den Studio-Agenten läuft einmal in der DB-Migration (siehe unten).
- **Datenbank, Migration 14 (`SCHEMA_SQL_V6`):** `ide_agents` bekommt `engine_id TEXT` und `agent_role TEXT` (nullable, `agent_role` Standard `allrounder`).
  Die Ableitung der Engines braucht die Config, nicht die DB; deshalb schreibt eine **einmalige Anwendungs-Migration beim Start** (Core, nach `Config::load`)
  je eindeutiger Kombination Harness/Command/Modell/Env eine Engine in die Config und setzt `engine_id`. Die alten Spalten `harness`/`command`/`model`/`env`
  bleiben bis CP-A6 als Rückfall bestehen (kein Löschen in einer Migration, die Daten verlieren könnte).
- **Rollen-Ablage:** `~/.axiomata/agents/<name>/AGENT.md`; die Rolle `allrounder` wird wie die Bundled Skills **seed-if-absent** angelegt.
- **Schnittstellen:** CLI `ide engines list|add|edit|delete`, `ide roles list|show`; Tauri-Commands für Engines und Rollen; die Oberfläche (Engines-Liste, Rollen-Formular) liegt im **Inspektor des Studios** als Reiter „Agents" (nicht in den allgemeinen App-Einstellungen: das Studio wird ein eigenes Programm, Owner 2026-10-04);
  Rolle löschen nur, wenn keine Sitzung sie trägt. Katalog-Ansicht im Rail und Start-Logik bleiben CP-A6/CP-A9.
- **Tests (inline):** AGENT.md-Parser (Frontmatter, Fehler, Größe, Symlink), Engine-Validierung, Hash-Bestätigung der Überschreibung, Migration auf einer
  Datenbank mit Alt-Zeilen (Ableitung eindeutig, idempotent), Config-Round-Trip; Frontend: Vitest für die Formular-Logik.
- **Prüfung vor dem Commit:** ein kombinierter Review-Agent (schlank); `security-auditor` entfällt hier laut Plan, **bis auf** die Projekt-Überschreibung
  (Dateien aus dem Projekt wirken auf Prozess-Start) — die wird im Review ausdrücklich mitgeprüft.

*Vom Owner bestätigt (2026-10-04):* Crate-Zuschnitt `axiomata-roster` (statt Modul in Core) und die alten Spalten bleiben bis CP-A6.

## CP-A3 im Detail (gebaut 2026-10-04)

Zuschnitt in `axiomata-ide::mailbox` (Migration 16, `SCHEMA_SQL_V7`); weicht nirgends von A8/A29 ab, präzisiert aber:

- **„Aufgaben" sind die Karten.** Eine eigene Aufgaben-Tabelle gibt es nicht: die Aufgabe im A2A-Sinn ist die Karte (Zustand abgeleitet, A12), die Nachricht
  nennt sie über `card_id` (ohne Fremdschlüssel, A11). Die Artefakte folgen mit CP-A4 als Datei-Teile.
- **Nachrichtenarten:** `message`, `ack` (reine Bestätigung) und `notice` (vom Studio, z. B. Rückläufer). Auf `ack` und `notice` wird nicht geantwortet (A8).
- **Antwortrecht:** `in_reply_to` nur auf eine Nachricht, die der Absender selbst **empfangen** hat; sonst ließe sich eine Kette fälschen.
- **Kettenzähler:** frische Nachricht = 1, Antwort = Zähler des Elternteils + 1, Schreiben des Owners setzt zurück. Über 6 wird die Nachricht **angehalten**
  (`held`): nur das Postfach des Owners bekommt sie, der Absender ein Notice; `release_held` lässt sie weiterlaufen (Zähler wieder 1). Nachrichten an den Owner
  werden nie angehalten (er beendet die Kette).
- **Grenze 20:** je Absender-Sitzung und Karte; Nachrichten ohne Karte zählen als ein eigener Eimer; der Owner ist nicht begrenzt. Eine abgelehnte Nachricht wird nicht gespeichert.
- **Zustellung:** Rolle → die lebenden Sitzungen dieser Rolle (die Liste der lebenden Sitzungen reicht der Aufrufer herein, der Absender fällt heraus);
  nicht zustellbar → gespeichert, Notice an Absender (falls Sitzung) und Owner. `nudges` nennt nur `Idle`-Sitzungen (nie `Waiting`: das würde eine
  Rückfrage beantworten), höchstens 3-mal je Zustellung; die Zeile wird aus gefilterten Namen gebaut, das Tippen und die Prüfung „tippt der Owner gerade?" sind Sache der Oberfläche.
- **Projektgrenze (aus dem Review):** eine Sitzung erreicht nur Sitzungen **ihres Projekts**; eine Rolle wie `reviewer` gibt es in jedem Projekt, und eine Nachricht trägt Text und
  Dateipfade eines Repositorys. Eine Sitzung eines anderen Projekts wird mit denselben Worten abgewiesen wie eine, die es nicht gibt. Der Owner ist nicht begrenzt.
  Sitzungen dürfen kein `notice` senden (das ist die Stimme des Studios).
- **Pflichten für CP-A4 (aus dem Review, hier festgeschrieben):** (1) Absender, `Inbox` und `card_id` kommen **aus der Sitzung** (`AXIOMATA_AGENT_ID`, geclaimte Karte), nie aus
  einem Argument des Agenten — sonst setzt ein wechselndes `card_id` die Grenze 20 zurück, und eine frische Nachricht statt einer Antwort umgeht Kettenzähler und Ack-Regel.
  (2) Der Server setzt `in_reply_to` selbst, wenn der Agent auf eine ungelesene Nachricht derselben Gegenseite und Karte antwortet (oder verlangt es). (3) Die Owner-Operationen
  (`release_held`, `Inbox::Owner` lesen, `Sender::Owner`) sind in einer Agenten-Sitzung nicht erreichbar, im Test festzuschreiben. (4) **Offen für den Owner:** zusätzlich eine
  Gesamtgrenze je Sitzung unabhängig von der Karte (der Plan nennt nur „je Agent und Karte")?
- **Aufbewahrung gelöschter Karten:** das Brett kann von einer gelöschten Karte nicht „abgeschlossen" sagen; der Aufrufer von `purge` listet sie deshalb als geschlossen (CP-A4-Glue).
- **Einstellbar** sind die Zahlen über `Limits` (6 / 20 / 3); die Einstellungsoberfläche folgt, wenn es sie braucht.
- **Noch nicht:** MCP-Werkzeuge (CP-A4), CLI-Spiegel `agent send|inbox` (A31), Tauri-Glue, Oberfläche (Team-Panel CP-A9), der Aufruf von `purge` beim Start (braucht die Liste abgeschlossener Karten aus dem Brett — der Glue in `axiomata-core` kommt mit CP-A4).

## CP-A4 im Detail (gebaut 2026-10-04)

`axiomata-cli mcp-serve` (A28) = `axiomata-core::agent_mcp`; Details in `architecture.md` §3. Festlegungen beim Bau, die der Plan offen ließ:

- **Handgeschriebenes JSON-RPC** statt einer Protokoll-Bibliothek (vier Methoden; „selber bauen“). Zeilenweise über stdio, Zeilen höchstens 1 MiB, keine Batches.
- **Identität nur aus der Umgebung:** `AXIOMATA_AGENT_ID` (Pflicht), `AXIOMATA_CARD_ID` (die Karte, für die die Sitzung gestartet wurde; setzt CP-A6) und `AXIOMATA_PLAN_ID`
  (Planer, CP-A7). Ohne `…_CARD_ID` ist die Karte der Sitzung die, die sie hält. Der Reviewer hält die Karte nicht (er prüft die des Umsetzers) und braucht deshalb `…_CARD_ID`.
- **Rollenrechte aus der `kind` der Rolle:** `review` → `review_verdict`; `plan` → `create_card` für jede Art; alle anderen → `claim_task`/`report_done`, `create_card` nur mit `creates:`
  und nur diese Arten. Fehlende Rollendatei = nur lesen und Post. Vorgeschlagene Karten landen immer in „Vorschlag“, mit dem Plan der Ausgangskarte (A17); die Auto-Start-Regel für `creates`-Arten
  (A7) sowie Tiefe und Anzahl selbst angelegter Karten kommen mit CP-A8.
- **Anwesenheit als Dateisperre** (`axiomata-ide::presence`, `<events>/<id>/mcp.lock`): Ein laufender Server hält sie; wer sie nehmen kann, weiß, dass die Sitzung fort ist. Das Betriebssystem gibt sie
  beim Prozessende frei, es gibt keinen Herzschlag und nichts aufzuräumen. Für Opencode ist der Server nach A31 vermutlich je Verzeichnis, nicht je Sitzung — Sache des CP-A5-Spikes.
- **`claim_task` = `flow::start_card`:** Claim und Verschieben nach „In Arbeit“ und `started`-Ereignis in einer Transaktion; Karte muss bereit sein (offene Spalte, Vorgänger geprüft, nicht gehalten);
  für den Halter idempotent. Eine Sitzung hält höchstens eine Karte gleichzeitig.
- **Antwort-Ableitung** (Pflicht (2) aus dem CP-A3-Review) liegt im Postfach-Kern, nicht im Server: eine Nachricht an jemanden, der der Sitzung zur selben Karte geschrieben hat und noch unbeantwortet ist, gilt als Antwort.
- **Die CLI loggt auf stderr** (vorher stdout): ein Logzeile auf stdout wäre eine Nachricht, die der Client nicht versteht.
- **Aus Review und Security-Audit von CP-A4 eingearbeitet:** (1) `in_reply_to` ist kein Argument des Werkzeugs mehr; die Verknüpfung macht allein der Postfach-Kern, und eine nach dem Kettenlimit
  *angehaltene* Antwort zählt nicht als beantwortet (sonst startete die nächste Nachricht eine frische Kette). (2) **Reichweite:** `get_card`, `list_cards`, `claim_task`, `review_verdict` nur auf dem
  Brett, auf dem die Sitzung arbeitet (Brett der Karte bzw. des Plans); ohne Karte nur Karten, die der eigenen Rolle zugewiesen sind (`card.agent`); `claim_task` verlangt außerdem, dass `card.agent` leer
  oder die eigene Rolle ist. Das Brett enthält auch Karten des Owners (ToDo). (3) „Eine Karte gleichzeitig“ prüft `flow::start_card` nach dem Sperren der Transaktion (nur für `agent:`-Akteure);
  eine unsignierte, vom Owner nach „Fertig“ gezogene Karte zählt nicht mehr als offene Arbeit. (4) `report_done` schreibt die Zusammenfassung in derselben Transaktion (zu lang → nichts bewegt);
  `create_card` ist `flow::propose_card` (Karte, Abhängigkeiten, Herkunftszeile atomar) und **höchstens 20 Vorschläge je Sitzung** (`MAX_PROPOSALS_PER_ACTOR`). (5) Anwesenheit: ein beschädigter
  Lock-Pfad zählt als „nicht lebendig“ statt alles lahmzulegen; `hold` wartet bis 2 s, und ein Server, der den Lock nicht bekam, übernimmt ihn später auf einem eigenen Thread, wenn der alte endet.
  (6) Dateiteile nennen nur relative Pfade im Projekt (kein `/`, `~`, `..`, Schema). (7) Rollen der Sitzung = die für ihr Projekt geltenden (`roster::project_roles(..).effective`, also bestätigte
  Projektrollen), nicht nur die des Owners. (8) Protokoll: Notifications lösen nichts aus (auch `tools/call` ohne id nicht), Antworten des Clients werden nicht beantwortet, unbekannte Version → neueste.
- **Bekannte Grenze (A39, jetzt konkret):** die Umgebung (`AXIOMATA_AGENT_ID`, …) erbt jeder Kindprozess der Sitzung. Ein Agent kann `AXIOMATA_AGENT_ID=<Reviewer> axiomata-cli mcp-serve` selbst
  starten (die IDs liefert `list_agents`) und die eigene Karte abzeichnen — das Zwei-Parteien-Prinzip vergleicht nur Akteurs-Strings. **Geschlossen mit CP-A5** (Geheimnis je Start, siehe unten); übrig bleibt die Grenze „derselbe Benutzer kann Dateien lesen“.
- **Entschieden (Owner, 2026-10-04):** 20 Vorschläge je Sitzung reichen als Start. Für die **Gesamtgrenze je Sitzung** hatte der Owner kein Gefühl; gesetzt sind **60 Nachrichten** (`Limits::max_per_sender_total`, das Dreifache der Grenze je Karte), einstellbar wie die anderen Zahlen.
- **Noch nicht:** Eintrag je Harness und Start-Umgebung (CP-A5/CP-A6), der CLI-Spiegel `agent send|inbox` (A31), Kosten-/Schrittgrenzen, `purge` beim Start.

## CP-A5 im Detail (gebaut 2026-10-04)

**Owner-Entscheidungen vor dem Bau:** (1) der Eintrag wird **angezeigt, nicht bestätigt** (A10/A30 verlangten eine Bestätigung per Hash; sie entfällt, weil nichts aus dem
Projekt einfließt — der Inhalt kommt allein von Axiomata); (2) **jede** Sitzung auf dem erzeugten Befehl bekommt den Server, auch von Hand gestartete; Sitzungen mit eigenem Befehl
bleiben unberührt (E13); (3) die **CLI-Befehle `board claim|report|verdict|add` sind in einer Agenten-Sitzung gesperrt** (`mcp_only`), damit der MCP-Server die einzige Tür ist.
`board input|fail|cancel|note` bleiben offen (sie verlangen Halter oder Owner; ein MCP-Gegenstück gibt es noch nicht).

**Spike Opencode 2.0.22 (A31), gegen den laufenden Dienst:** eine `opencode.json` im Worktree wird je Verzeichnis gelesen (`environment` wird übernommen), aber nach dem ersten Laden **zwischengespeichert** —
eine Änderung wirkt auch nach `connect` nicht, ein neues Geheimnis je Start ginge so nicht, und die Datei läge im Take-over. Stattdessen `PUT /api/experimental/mcp/axiomata`
(Body `{"config": {"type":"local","command":[…],"environment":{…}}}`, Header `x-opencode-directory` = Worktree): kein Eintrag im Worktree, ein zweites PUT ersetzt Konfiguration und Prozess,
`DELETE` räumt auf (beim Verwerfen des Worktrees und Löschen des Agenten, ohne den Dienst dafür zu starten). Die Registrierung lebt im Speicher des Dienstes; ein neu gestarteter Dienst erfährt sie beim
nächsten Start des Agenten. **Claude Code 2.1.288:** `--mcp-config <datei>` bindet den Server ein (`connected`, Quelle `dynamic`), die Werkzeuge folgen der Rolle; live mit Haiku bestätigt
(`read_inbox`, `list_agents`, Anwesenheit `running: true`). *Am Mac geprüft (Owner, 2026-10-04, von Hand gestartete Sitzung „Claude Sonnet“):* kein Dialog beim Start (A30 bestätigt), und `read_inbox` lief ohne Erlaubnis-Rückfrage; der Inbox-Reiter zeigt „Team tools connected“. Ob das an `--mcp-config` oder an den eigenen Claude-Einstellungen des Owners liegt (`mcp__*` erlaubt?), ist offen; `send_message` ist noch nicht ausprobiert. Für die Karten-Sitzungen (CP-A6) bleibt `--allowedTools` Pflicht (A34).

**Gebaut:** `axiomata-ide::session_token` (Geheimnis je Start: 256 Bit, nur der SHA-256 im Kanalordner, jeder Start ersetzt ihn; `Context::from_vars` verlangt es, sonst startet der Server nicht);
`Channel::write_private` (`0600`) und `append_instructions`; `axiomata-core::agent_entry` (CLI-Pfad `$AXIOMATA_CLI` oder `axiomata-cli` neben dem Programm, Konfiguration beider Harnesses, Anweisungstext aus Rolle plus Postfach-Hinweis,
`AgentEntry` für die Anzeige ohne Geheimnis); `ide_start::start_agent(core, id) -> Started`; `Service::register_mcp`/`remove_mcp`; Inbox-Reiter zeigt den Eintrag; das **Anstupsen** (A8, Weg 2):
`ide_mailbox_nudge`/`ide_mailbox_nudged`, `Terminal.typeLine`, Regeln in `ide/nudge.ts` (nur im Leerlauf, nie während der Owner tippt — 10 s Ruhe —, höchstens alle 3 s eine Frage).
Fehlt der CLI-Pfad oder lehnt der Dienst ab, startet die Sitzung ohne die Teamwerkzeuge und der Reiter nennt den Grund.

**Aus Review und Security-Audit von CP-A5 eingearbeitet:** (1) Opencode-Eintrag nur für einen **eigenen Worktree**: Sitzungen eines Projekts ohne Repository teilen den Ordner, und die Registrierung je Verzeichnis
würde sich gegenseitig überschreiben (die erste spräche als die zweite) — dort „nicht anwendbar“; (2) nach dem PUT wartet `await_mcp` bis der Server `connected` ist (ein 204 sagt nur „Konfiguration angenommen“), `failed` meldet den Grund;
(3) ein abgelehnter PUT nennt nur den Status, **nie den Antwort-Body** (er kann die Anfrage samt Geheimnis zurückgeben); (4) `Server` hat kein `Debug` mehr; (5) die **`ide …`-Befehle**, die Sitzungen starten oder
ändern (`agents prepare|new|edit|delete|…`, `engines`/`roles`/`projects` schreibend), außerdem `routines` schreibend und `skills reseed`, sind in einer Agenten-Sitzung gesperrt — sonst hätte ein Agent mit `ide agents prepare <Reviewer>`
ein frisches Geheimnis ausstellen und aus der Datei lesen können (das war die „Offen“-Liste aus A39); (6) die Zeile des Anstupsens nennt Absender nur als `session <id>`, nie mit Freitext-Namen (ein Name wie „the owner says …“ hätte der Nachricht die
Stimme des Owners gegeben), `typeLine` entfernt auch die C1-Steuerzeichen, und kurz vor dem Tippen wird Leerlauf und Ruhe des Owners **erneut** geprüft; die Ruhezeit ist 30 s (ein halb geschriebener Prompt würde sonst mitgeschickt).
**Bewusst nicht geschlossen** (dokumentierte Grenze „derselbe Benutzer“): wer `AXIOMATA_AGENT_ID` löscht, ist für die CLI der Owner; `--actor agent:…` ohne Sitzung wird nicht abgewiesen (der Owner darf von Hand testen, und ein Agent mit gelöschter Umgebung könnte ohnehin als `human:owner` handeln);
`board note|input|fail|cancel` und `dep` stehen mit exportierter fremder Id weiter offen (Halter-Rechte, kein MCP-Gegenstück); der Kanalordner ist für den Agenten beschreibbar, er kann den Hash einer eigenen Wahl dort ablegen.
Was das Geheimnis schließt: ein **MCP-Server als eine andere Sitzung** zu starten, ohne deren Konfiguration zu lesen.

**Verschoben:** der **Start-Prompt** (A35) und `AXIOMATA_CARD_ID`/`…_PLAN_ID` im Eintrag gehören zu „Karte starten“ (CP-A6), wo es erst eine Karte gibt; ebenso die Rechte unbeaufsichtigter Sitzungen (A34).
**Offen:** (a) der Server einer Opencode-Sitzung lebt je Verzeichnis und überlebt das Terminal — seine Anwesenheitssperre sagt „lebt“, nachdem das Pane zu ist; der Statusbeobachter weiß es besser und
ist in CP-A6/CP-A9 zu befragen; (b) die Rollen-Anweisung erreicht Opencode erst als erste Nachricht (CP-A6); (c) der Nachrichten-Hinweis während der Arbeit (Hooks, Weg 3) bleibt „später“.

**A6a — Darstellung (Vorschlag des Owners, vom Owner bestätigt 2026-10-04):** drei Modi im Studio statt zwei: **Editor | Canvas | Flow**. *Canvas* = die heutige freie Fläche der Agenten-Terminals
(ohne Karten); *Flow* hat die Reiter **Planung** (Planer-Terminal, Vorschläge, Freigabe, CP-A7), **Agents** (Rollenkatalog, Sitzungen, Team-Panel, Nachrichten, CP-A9) und **Flowansicht** (Graph, CP-A10).
Ersetzt A6 (dort ging die freie Fläche im Flow auf) und A36, wo sie abweichen.

## CP-A6a im Detail (gebaut 2026-10-04)

**Zuschnitt (Owner, 2026-10-04):** CP-A6 in drei Stufen — **6a Karte starten** (hier), **6b** Reviewer, Rückgabe und Take-over (Stücke 2–4), **6c** Limits je Sitzung (A9/A33). **Projekt beim Start gewählt** (Dialog, CLI `--project`); Folgekarten und der
Reviewer übernehmen das Projekt der Vorgängersitzung, der Planer bekommt seines mit CP-A7 am Plan. Der Startknopf liegt am **Kanban-Brett** (Detailansicht einer bereiten Karte), solange es den Flow-Modus noch nicht gibt.

**Ablauf:** `card_session::start_card_session` macht eine **Sitzung** (Agentenzeile, Name `<rolle>-<karte>`, bei Wiederholung `-2`, …) der Rolle der Karte (`card.agent`, sonst `allrounder`) auf einer Engine — die gewählte, sonst die der Rolle, sonst die erste vorhandene
Ausweich-Engine; nennt nichts eine Engine, wird der Owner beim Start gefragt — und **claimt die Karte atomar für sie** (A20, `flow::start_card`, Akteur = die Sitzung). Scheitert der Claim (Vorschlag, gehalten, nicht bereit, abgesagt), wird die Zeile wieder gelöscht. Verlangt:
ein Repository (unbeaufsichtigte Sitzungen teilen nie den Ordner des Owners), `axiomata-cli` für die Teamwerkzeuge, eine Rolle, die Karten **bearbeitet** (nicht prüft oder plant), eine Engine **ohne eigenen Befehl** (E13: den fasst Axiomata nicht an). Karten mit Vorgängern werden abgelehnt
(der Start auf dem Zweig des Vorgängers, A16, kommt mit 6b). Das Gegenstück `flow::release_started` gibt eine gestartete Karte zurück (Halter oder Owner, Karte zurück nach Offen, Ereignis `released`); CLI `board start|release` sind Owner-Schritte.

Die **Harness startet, wenn der Pane der Sitzung öffnet** (`prepare_ide_agent` → `start_agent`): `card_of` erkennt, dass die Sitzung eine Karte hält, und startet sie dann unbeaufsichtigt — `AXIOMATA_CARD_ID` in der Umgebung des MCP-Servers (kein Argument, das ein Modell ändern könnte),
Claude Code mit `--permission-mode acceptEdits --allowedTools <je Werkzeug der Rolle, einzeln, kein Wildcard> + role.permissions` und dem **Start-Prompt hinter `--`** (A35: nur Namen und Ids, nie der Kartentext, den holt der Agent mit `get_card`), Opencode mit Regeln `axiomata_<werkzeug>: allow` je Werkzeug
(Opencode nennt MCP-Werkzeuge `<server>_<werkzeug>`) plus `git push` verboten und der ersten Nachricht (Rolle + Start-Prompt) über die API — **nur bei einer neu angelegten Sitzung**, eine fortgesetzte hat sie schon. `role.permissions` sind Regeln im Stil von Claude Code (`Bash(git status *)`) und
gelten nur dort. Fehlen die Teamwerkzeuge (kein CLI, Dienst lehnt ab, eigener Befehl, Mini-Harness), wird eine Karten-Sitzung **nicht gestartet**: sie könnte nicht berichten. Alles andere bleibt eine Rückfrage im Pane (A34); `bypassPermissions` nie.
Eine Sitzung, die eine Karte **selbst** genommen hat (`claim_task`) und neu gestartet wird, zählt genauso als Karten-Sitzung — das braucht eine unterbrochene Sitzung.

**Oberfläche:** „Starten …“ in der Detailansicht einer Karte im Zustand `ready` öffnet ein kleines Formular (Projekt, vorbelegt mit dem zuletzt geöffneten; Engine, vorbelegt „die der Rolle“); danach öffnet das Studio den Pane der Sitzung im Agents-Modus (`shell:agent` →
`ide/agentRequest.ts` → `IdeView.showRequestedAgent`). **Live bestätigt (Scratch-Home, 2026-10-04):** Claude Code (Haiku) las `get_card`, schrieb die Datei im Worktree und rief `report_done` — Karte in Review mit Zusammenfassung; Opencode: Sitzung mit den Einzelregeln, Server verbunden, erste Nachricht zugestellt.

**Aus Review und Security-Audit von CP-A6a eingearbeitet:** (1) die Opencode-Sitzung wird erst **nach** Verdrahtung und erster Nachricht gemerkt — scheitert der Start dazwischen, gäbe der nächste Start sonst keine erste Nachricht mehr; (2) `card_of` zählt nur
eine Karte **in Arbeit** (nicht in Review: ein neu gestarteter Pane soll nicht noch einmal arbeiten) und eine Sitzung mit eigenem Befehl oder der Mini-Harness wird bei gehaltener Karte normal gestartet statt abgelehnt; (3) `role.permissions` gehen nur noch durch, wenn sie eng sind (`Tool(spec)`
mit einer Einschränkung; nackte Werkzeugnamen, `Bash(*)`, Kommas fallen weg), Claude bekommt zusätzlich `--disallowedTools` für **`git push` in allen Schreibweisen und `Edit(.claude/**)`, `Edit(.mcp.json)`** (`acceptEdits` nimmt Änderungen ohne Rückfrage, die Sitzung soll ihre eigenen Regeln nicht
erweitern), Opencode verbietet `git * push` ebenfalls; (4) `flow::release_started` lässt eine gescheiterte oder abgesagte Karte nicht nach Offen zurück; (5) **Freigeben** (`release_card_session`, Tauri `release_card`, CLI `board release`, Kanban-Knopf bei einer Karte in Arbeit) nimmt außerdem das **Geheimnis der Sitzung zurück**,
und `board start` gibt die Karte selbst zurück, wenn die Sitzung nicht startet; (6) Anfragen an das Studio stehen in einer Warteschlange, ein Fehler beim Öffnen wird gemeldet; (7) die Bestätigung der Projekt-Rollen sagt jetzt, dass deren Rechte in Karten-Sitzungen **ohne Rückfrage** gelten.
**Aus dem ersten Mac-Test (Owner, 2026-10-04):** der Startbefehl einer Karten-Sitzung war 1036 Zeichen lang und kam im Pane bei 1024 abgeschnitten an (Zeilenlimit eines Terminals im Zeilenmodus, macOS `MAX_CANON`; mitten im Prompt, danach lief nichts). Befehle über 700 Zeichen
stehen deshalb in `<events>/<id>/launch.sh` (0600, bei jedem Start neu, `exec <befehl>`), und der Pane tippt nur `sh '<pfad>'`. Der Start-Prompt sagt außerdem, dass die Sitzung **nicht committet und nicht pusht** — das Take-over nimmt nur Committetes mit, und das Committen übernimmt der Owner (6b: das
Take-over der Karte committet vorher die Änderungen im Worktree); sonst fragte ein Agent nach einer Erlaubnis für `git commit`, die er nicht bekommt. Danach lief der Ablauf am Mac: Claude las den Prompt, rief `axiomata` ohne Rückfrage auf, und ein Shell-Befehl wurde — wie geplant (A34) — zur Rückfrage im Pane
(Status „waiting“). Einfache lesende Befehle (`ls`, `grep`, `git status`) laufen von selbst; ein zusammengesetzter Befehl mit `cd … && …; … | head` fragt immer (Claudes eigene Prüfung, mit und ohne unsere Sperren geprüft).
**Bewusst offen / zu wissen:** (a) jede erlaubte Bau- oder Testbefehlszeile (`Bash(cargo test)`) plus `acceptEdits` ist Codeausführung — der Agent ändert `build.rs` oder Tests und führt sie aus; die Rechte einer Rolle sind damit Vertrauen in die Rolle, auch die eigenen des Owners;
(b) Opencodes Vorgabe ohne passende Regel ist laut Dokumentation `ask`, dann bleiben Shell und Edits Rückfragen — am Mac bei der ersten echten Karte zu bestätigen; (c) Freigeben stoppt den laufenden Pane nicht (der Owner schließt ihn), die Sitzung könnte `claim_task` erneut aufrufen, bis ihr Pane zu ist;
(d) die Rechte der Rolle zeigt das Startformular noch nicht; (e) ein einzelnes `card_of`-Flag an der Sitzung statt der Ableitung aus dem Claim wäre robuster (LOW).

**Noch nicht (6b/6c und später):** Reviewer (andere Engine, eigener Worktree), Rückgabe, Take-over und Aufräumen (A22), Start auf dem Zweig eines Vorgängers, Fortsetzen/Freigeben unterbrochener Karten in der Oberfläche (A23; `board release` gibt es als CLI), die Rückfrage einer wartenden Sitzung als `input_required` an der Karte, Kosten- und Schrittgrenzen.

## CP-A6b im Detail (gebaut 2026-10-05)

**Zuschnitt:** Reviewer, Rückgabe, Take-over und Aufräumen. **Verschoben nach CP-A8:** der Start einer Folgekarte auf dem Zweig des Vorgängers (A16) — Pläne mit Abhängigkeiten gibt es erst mit CP-A7/A8, und gestapelte Zweige haben ein eigenes Problem (ist der Vorgänger schon
als Squash im Hauptzweig, kollidiert der Folgezweig mit sich selbst); `start_card_session` lehnt Karten mit Vorgängern weiter ab.

**Datenmodell (Migration 17):** `ide_agents.card_id`, `card_review`, `start_ref`. Die Sitzung **merkt sich ihre Karte**, statt dass „Karten-Sitzung“ aus dem Claim abgeleitet wird (damit ist auch der LOW-Befund aus 6a erledigt: eine von Hand gemachte Sitzung, die selbst eine Karte nimmt, ist keine
Karten-Sitzung). `ide_start::card_launch` liest es und prüft, ob die Karte die Sitzung noch will: ein Arbeiter braucht die Karte **in Arbeit**, ein Reviewer **in Review**.

**Reviewer starten** (`card_session::start_review_session`, automatisch durch den Beobachter der App, von Hand `board review <karte> [--engine]` oder „Review starten …“ im Kanban): Voraussetzung ist eine Karte in Review, die eine Sitzung **des Studios** bearbeitet hat (eine vom Owner von Hand in die Spalte
gezogene Karte hat keinen Arbeiter und ist die des Owners). Dann: (1) **Schnappschuss** — was der Arbeiter nicht committet hat (er soll es nicht), wird auf seinem Zweig committet (`AgentRepo::snapshot`); (2) **Rolle**: die mit `kind: review`, bei mehreren die stärkste Stufe;
**Engine**: die gewählte, sonst die der Rolle, sonst die erste Ausweich-Engine, die existiert, keinen eigenen Befehl hat und **nicht die des Arbeiters** ist — gibt es keine, wird der Owner gefragt (nie still dieselbe Engine, A21); (3) eine Sitzung `reviewer-<karte>` ohne Claim, deren Worktree ein **losgelöster Checkout
des Schnappschusses** ist (`worktree::add_detached`, nur volle Commit-Ids, kein Zweig zum Schreiben). **Ein Review je Meldung:** ein Reviewer, der nach der letzten `reported`-Zeile entstand, ist der zuständige; nach einer Rückgabe und neuer Meldung entsteht ein neuer.
Der Reviewer bekommt keine automatischen Änderungen: Claude ohne `acceptEdits` und mit `--disallowedTools Edit`, Opencode mit `edit: deny`; sein Start-Prompt nennt Karte, Arbeiter und Basiszweig (`git diff <basis>...HEAD`). Die Rolle `reviewer` wird wie `allrounder` **gesät, wenn sie fehlt** (auch nach dem Löschen beim nächsten Start); ihr Text steht in `axiomata_roster::reviewer_role`.

**Rückgabe:** sagt der Reviewer „zurück“, schreibt das Studio dem Arbeiter eine **Notice** (`mailbox::studio_notice`, die Stimme des Studios, keine Antwort einer Sitzung) mit der Anmerkung; der wartende Arbeiter wird wie jede ungelesene Post angestupst. Bei „gut“ gibt es keine Notice.

**Take-over der Karte** (`card_session::take_over_card`, `board take-over <karte> [--message]`, „Übernehmen …“ im Kanban bei einer abgezeichneten Karte): übernommen wird **genau der geprüfte Stand** — steht der Zweig des Arbeiters nicht auf dem Commit, den der letzte Reviewer gesehen hat, oder liegt Ungeprüftes im Worktree,
passiert nichts und die Karte sagt es (eine Signatur auf anderer Arbeit als der, die ausgeliefert wird, ist keine). Eine vom Owner ohne Reviewer abgezeichnete Karte hat nichts zu vergleichen, ihre Änderungen werden so committet. Dann der bestehende Squash (G7–G12) mit `#<karte> <titel>` oder dem Text des Owners;
ein **Konflikt wird zurückgenommen** und mit den Dateinamen gemeldet. Bei Erfolg: `mark_taken_over` (archiviert), Aufräumen nach A22 — Worktrees von Arbeiter und Reviewern entfernt, Zweig des Arbeiters gelöscht (`worktree::delete_branch`, nur `axiomata/…`), Sitzungen samt Geheimnis, Kanal und Opencode-Registrierung vergessen. Was sich nicht entfernen ließ, kommt als Liste mit zurück; die Übernahme gilt trotzdem.
Es wird **nie gepusht**. Der Pane eines aufgeräumten Agenten bleibt zum Nachlesen offen und meldet nur, dass das Profil fehlt.

**Beobachter** (`apps/axiomata/src-tauri/src/card_watch.rs`): alle 3 s, in der App; startet den Reviewer jeder Karte, die noch keinen hat, und meldet es dem Frontend (`card:review-started` — das Studio öffnet den Pane **im Hintergrund**, ohne den Owner aus seiner Ansicht zu ziehen, denn der Pane startet die Harness — und `card:review-blocked` als Hinweis, wenn der Owner eine Engine wählen muss).
Er läuft nur, solange die App läuft; ohne App bleibt eine gemeldete Karte in Review, bis der Owner `board review` aufruft oder die App wieder läuft.

**Live bestätigt (Scratch-Home, 2026-10-05):** Arbeiter (Haiku) erledigt die Karte, `board review` auf der Worker-Engine wird abgelehnt, ohne Engine mit „pick one“ abgelehnt, auf der zweiten Engine startet der Reviewer im losgelösten Checkout, liest `get_card` und `git`, zeichnet ab; `board take-over`
bringt einen Commit auf `main`, Branch, Worktrees und Sitzungen sind weg, die Karte ist archiviert.

**Aus Review und Security-Audit von CP-A6b eingearbeitet:** (1) ein Reviewer **früherer Meldung** kann nicht mehr urteilen: `card_launch` und das MCP-Werkzeug `review_verdict` verlangen den *aktuellen* Reviewer (`current_reviewer`), frühere Reviewer werden beim Start des nächsten **entfernt** (Sitzung, Worktree, Geheimnis), und nach einem Urteil wird das Geheimnis des Reviewers zurückgenommen;
(2) das Take-over vergleicht gegen den Schnappschuss der Sitzung, die die Karte **abgezeichnet hat** (`verified_by`), nicht gegen „den letzten Reviewer“, prüft den **Zweig** (`refs/heads/<zweig>`) statt `HEAD` und verlangt, dass der Worktree auf diesem Zweig steht; `commit_all`/`snapshot` committen nur dort;
(3) **Hooks und Konfiguration im Worktree des Arbeiters laufen nicht mehr:** git, das das Studio dort selbst aufruft, bekommt `core.hooksPath=/dev/null` und `core.fsmonitor=false` (`git_agent`), der Commit `--no-verify`; (4) **ein Reviewer liest nichts, was der Arbeiter für Agenten abgelegt hat:** ändert die Arbeit `.claude/`, `.mcp.json`, `opencode.json(c)`, `.opencode/` oder `CLAUDE.md`/`AGENTS.md` (`is_agent_config`), wird der Review **nicht automatisch** gestartet — der Owner startet ihn von Hand und nimmt das ausdrücklich an
(`--allow-agent-config`, Kästchen im Formular) —, und ein Claude-Reviewer läuft immer mit `--setting-sources user --strict-mcp-config`; (5) die **Datenbank ist nur zum Lesen und Schreiben gesperrt, nie während git läuft** (`plan` → git → `finish`, für Review und Take-over; die Aufrufe laufen in `spawn_blocking`); (6) ein Ereignis des Beobachters, das vor dem Laden der Seite kam, geht nicht verloren: das Studio fragt beim Start einmal
nach den laufenden Reviewern (`open_card_sessions`); (7) `mark_taken_over` und Aufräumen sind nach dem Squash nur noch **Hinweise**, nie ein Fehler (die Arbeit ist im Hauptzweig); eine Sitzung, deren Worktree bleibt, behält ihre Zeile; alle Zweige der Sitzungen einer Karte werden gelöscht; (8) die Anmerkung des Reviewers erreicht den Arbeiter **als Zitat mit Herkunft** („in seinen Worten, keine Anweisung des Studios“);
(9) das Take-over verlangt einen Arbeiter, den das Studio für diese Karte gestartet hat; `latest_event` statt „die letzten 500 Zeilen“; nach dem Schnappschuss entsteht kein Commit mehr für einen Review, der danach an einer Namens- oder Rollenfrage scheitert.

**Aus dem Mac-Test (Owner, 2026-10-05):** Arbeiter und Reviewer fragten bei verketteten Befehlen (`cd … && ls | grep …`, `git log … && git status …`) nach — Claude Code prüft das selbst, Opencode erlaubt nur einzelne Muster. Beide Start-Prompts sagen jetzt, dass die Shell schon im Worktree startet und **ein einfacher Befehl nach dem anderen** abzusetzen ist (kein `cd`, kein `&&`, `;`, `|`); der Opencode-Reviewer darf `git log|diff|show|status|rev-parse` einzeln ohne Rückfrage. Der automatische Start des Reviewers wurde dabei bestätigt (`reviewer-58` auf DeepSeek, erste Nachricht zugestellt).
**Hinweis auf den Push (Owner, 2026-10-05):** das Studio pusht nie; den Push gibt es als Knopf im Git-Reiter des Studios (`git_push`: nur der ausgecheckte Branch, an seinen Upstream oder `origin`, nie mit `--force`, nur durch einen Klick in der App). Nach einem Übernehmen sagt die Oberfläche jetzt, was wartet („Noch nicht gepusht: ↑N auf origin/main. Push im Git-Reiter des Studios.“, `unpushedNote`), und `board take-over` druckt dasselbe; `CardTakeOver::Done` trägt dafür die `project_id`. Ein zweiter Push-Knopf im Übernehmen-Formular wurde bewusst nicht gebaut: dazwischen soll der Owner die Arbeit an seinem Hauptstand ansehen können.
**Bewusst offen:** (a) die **Rechte des Reviewers** sind die der Rolle plus die Werkzeuge des Servers, sonst Rückfragen im Pane — ein Reviewer, der `npm test` laufen lassen will, fragt; (b) ein **zweiter Reviewer** bei Streit oder eine Eskalation nach zwei Rückgaben (A26) kommt mit CP-A8; (c) der Beobachter startet beim App-Start auch Reviews für Karten, die schon in Review
lagen, als die App aus war; (d) ein im Studio geöffneter, aber versteckter Pane startet mit Größe null, bis das Studio gezeigt wird; (e) die Rolle des Reviewers ist nur ein Text — was er prüft, entscheidet das Modell; (f) der **Take-over-Commit im Projekt des Owners** läuft mit den Hooks des Projekts, wie jedes Take-over (Owner-Repository, Owner-Klick); ein vom Arbeiter geänderter Hook-Skript (`.husky/`) kommt mit der Arbeit dorthin — der Review soll es sehen, die Erkennung (`is_agent_config`) kennt Hook-Verzeichnisse nicht; (g) die Zeitgrenze für git fehlt (ein hängender Hook in dessen Verzeichnis hielte nur noch die Aufgabe, nicht die Datenbank, an);
(h) die Logik des Beobachters (`card_watch::tick`) hat keinen eigenen Test, weil sie einen `AppHandle` braucht; ihre Bausteine (`cards_awaiting_review`, `start_review_session`) sind getestet.

## CP-A6c im Detail (gebaut 2026-10-05)

**Zuschnitt (Owner, 2026-10-05):** Limits je Sitzung (A9, A27, A33), nichts darüber hinaus — Limit je Plan und je Tag sowie die Eskalation kommen mit CP-A8, das Team-Panel mit CP-A9. **Nach einem Limit** stoppt die Sitzung nach dem laufenden Schritt, **die Karte bleibt bei ihr beansprucht** (Arbeit im Worktree bleibt); der Owner hebt das Limit der Rolle an und lässt sie weitermachen oder gibt die Karte frei. (Nicht: automatisch zurücklegen, wie A9 wörtlich sagt — ein Worktree mit halber Arbeit an einer Karte, die niemand hält, wäre der Preis.) **Der Wächter läuft nur in der App** (wie `card_watch`, alle 5 s).

**Werte:** `Limits::resolve(tier)` (Roster): was die Rolle in `limits:` setzt, sonst der Standard der Stufe nach A27 (leicht 60 Schritte / 0,50 $ / 400 k Token, mittel 120 / 2 $ / 1,5 M, schwer 250 / 6 $ / 4 M; `Tier::default_limits`). Das Geld zählt nur für Engines mit Abrechnung je Token (`Billing::Metered`) und nur, wenn für das Modell ein Preis in `config.agents.costs` steht (`spend::metered_cost_usd`); ein fehlender Preis ist nicht „kostenlos“, sondern „nicht gemessen“. Abo-Engines kennen Schritte und Token, nie Dollar.

**Messen** (`axiomata_ide::usage`, rein, ohne Preis und ohne Durchsetzung): *Claude Code* — beim Start `--session-id <uuid>` (`usage::new_claude_session_id`; der Kanal merkt sich alle Ids der Starts in `claude-sessions`, eine Karte zählt die Summe), das Transkript `~/.claude/projects/<Ordner>/<uuid>.jsonl` wird gefunden, indem jeder Ordner nach `<uuid>.jsonl` abgesucht wird (der Ordnername ist Claudes Sache). **Das Transkript schreibt eine Zeile je Inhaltsblock einer Antwort, jede mit demselben `usage`** — gezählt wird deshalb je `message.id` einmal (die größeren Zahlen gewinnen), Schritte als verschiedene `tool_use`-Ids. `ClaudeTally` liest inkrementell ab dem letzten Zeilenende (Offset; eine unfertige letzte Zeile bleibt liegen; eine geschrumpfte Datei wird neu gelesen). *Opencode* — `GET /api/session/<id>/message` (`Service::all_messages`), je Assistent-Nachricht `tokens.input + cache.write` als Eingabe, `output + reasoning` als Ausgabe, Schritte als Inhaltsteile vom Typ `tool`. Token zählen Eingabe, Ausgabe und Cache-Aufbau, **nicht** Cache-Lesezugriffe (A27). Gegen die echte Umgebung geprüft: dieselben Summen wie eine unabhängige Zählung des Claude-Transkripts bzw. der Opencode-Datenbank.

**Durchsetzen** (`axiomata_core::session_limits`): `Meter::check` sieht alle Sitzungen, die das Studio für eine Karte gestartet hat **und deren Karte sie noch will** (`card_launch`), misst, und je Sitzung entscheidet `decide`: über dem Limit und noch nicht gestoppt → **Stopp** (Marker `limit-reached` im Kanal, Zeile `limit_stop` im Verlauf der Karte, Ereignis `card:limit-reached` → Hinweis im Frontend); gestoppt und noch drüber → der Stopp bleibt; gestoppt und gemessen darunter (Limit angehoben) → der Stopp wird aufgehoben; **nicht gemessen** → nie gestoppt und nie aufgehoben (kein Messwert ist kein Befund). *Claude Code:* ein `PreToolUse`-Hook im erzeugten `--settings` lehnt mit Exit 2 jeden weiteren Werkzeugaufruf ab, solange der Marker existiert, und zeigt dem Modell dessen Text („keine Werkzeuge mehr, sag dem Owner in einem Satz, wo du stehst“) — der laufende Schritt endet sauber. *Opencode:* kein Hook, deshalb `POST /api/session/<id>/interrupt` beim Stopp und bei jedem weiteren Blick, solange der Stopp steht (der Owner könnte ins Pane tippen; weitermachen heißt Limit anheben, nicht daran vorbeireden). Ein neuer Start der Sitzung lässt den Marker liegen (ein neu gestartetes Pane über dem Limit wird vom ersten Aufruf an abgelehnt); aufgehoben wird er nur von einem Blick, der die Sitzung wieder innerhalb ihrer Limits misst.

**Anzeige:** Kanban-Detail „Verbrauch“ (`CardUsage.svelte`, je Sitzung Schritte/Token/Kosten mit Balken und „Gestoppt: …“; „Verbrauch unbekannt“, wenn die Aufzeichnung nicht lesbar war), CLI `board usage <karte>`, Tauri `card_usage`. Das Limit anheben: Studio → Engines & roles → Rolle → Limits (oder `AGENT.md`).

**Beim Prüfen am echten System gefunden:** Opencode lehnt `cursor` **zusammen mit** `order` ab (HTTP 400 `InvalidCursorError`, 2.0.23). `messages_after` hatte denselben Fehler, er zeigte sich nur nicht, weil ein Zug nie mehr als eine Seite hat; beide Wege bauen die Seitenadresse jetzt über `page_path` (erste Seite mit `order`, spätere nur mit dem Cursor).

**Aus dem Security-Audit von CP-A6c eingearbeitet:** das Transkript liegt dort, wo die Werkzeuge der Sitzung hinkommen — (1) eine Lektüre ist **begrenzt** (8 MiB je Blick, eine Zeile über 1 MiB wird übersprungen statt erwartet, höchstens 200 000 gemerkte Antworten/Werkzeugaufrufe), läuft in `spawn_blocking` und folgt keinem Link; (2) die **Session-Ids im Kanal** behalten die *ältesten* 100 (angehängte Zeilen verdrängen die echten nicht mehr), ein Link an Stelle der Datei wird entfernt; (3) ein **Höchststand** je Transkript bzw. Opencode-Sitzung: eine gekürzte Datei oder ein aus dem 2000er-Fenster gewanderter Verlauf zählt nie weniger als zuvor (`Usage::at_least`).

**Aus dem Review von CP-A6c eingearbeitet:** (1) der Marker überlebt einen Neustart des Panes (sonst ein Fenster bis zum nächsten Blick und ein zweiter `limit_stop`); (2) der Opencode-Dienst wird **einmal je Blick** gesucht (das ist ein Prozessstart) und jede Anfrage hat 10 s Zeit, damit ein hängender Dienst den Stopp der Claude-Sitzungen dahinter nicht aufhält; (3) eine Engine, deren Befehl schon `--resume`/`--continue`/`--session-id` nennt, bekommt keine eigene `--session-id` (sie ginge nicht zusammen) — ihr Verbrauch ist dann „unbekannt“; (4) die Anzeige leert sich beim Wechsel der Karte.

**Bekannte Lücken (bewusst):** (0c) `/clear` oder `/resume` im Pane erzeugt eine Sitzungs-Id, die nicht im Kanal steht und nicht gezählt wird; „Neue Sitzung“ bei Opencode tauscht `opencode_session` und lässt den alten Verbrauch fallen; (0d) `gather` lädt die Rollen unter der Datenbanksperre (ohne `.await`, aber mit Dateizugriff); `find_transcript` durchsucht je Blick alle Ordner und merkt sich keine Treffer; `Meter::check` hat noch keinen Test mit einem echten `AxiomataCore` (die Bausteine `decide`, `stop`, `read` sind einzeln getestet); (0) eine Sitzung kann **ihre eigene Messung** angreifen — `claude-sessions` oder das Transkript löschen, den Marker löschen (der nächste Blick schreibt ihn neu), im Hintergrund laufende Prozesse (der Hook sperrt nur *neue* Werkzeugaufrufe) —, das Studio ist kein Sandkasten (a2a.md A39); die Ids bräuchten eine Spalte in der Datenbank, wenn das je zählt; (0b) wer ein Limit erreicht hat, dessen Karte **nicht mehr in Arbeit** ist (gemeldet), wird nicht mehr überwacht (`wanted`); (a) die Transkripte von **Unter-Agenten** (`Task`-Werkzeug; eigene Dateien unter `<uuid>/subagents/`) sind nicht eingerechnet — deren Werkzeugaufrufe laufen aber durch denselben `PreToolUse`-Hook; (b) eine Sitzung, deren Aufzeichnung nicht lesbar ist, hat kein wirksames Limit und sagt es nur in der Anzeige; (c) ohne laufende App wird bei Opencode nichts durchgesetzt, und bei Claude Code wirkt nur ein schon geschriebener Marker; (d) Kosten sind Token × Preistabelle ohne Cache-Preise, also eine Näherung; (e) `all_messages` liest höchstens 2000 Nachrichten — eine längere Sitzung wird zu klein gezählt (die Schritt- und Token-Grenze greift dann später, nicht gar nicht); (f) ein Review mit `reviewer`-Rolle und eigenem Limit zählt für sich, nicht zusammen mit dem Arbeiter.

## CP-A7 im Detail (7a gebaut 2026-10-05)

**Zuschnitt (Owner, 2026-10-05):** CP-A7 in zwei Stufen — **7a Backend** (hier), **7b Oberfläche: der Flow-Modus im Studio** (dritter Modus *Editor | Canvas | Flow*, Reiter „Planung“, A6a). Entschieden: der Planer arbeitet in einem **losgelösten Lese-Checkout** des Projekts (wie der Reviewer: kein Zweig, nichts erreicht je ein Take-over); „Neuer Plan …“ und die Freigabe-Ansicht leben **im Flow-Modus**, nicht im Kanban. Nicht in CP-A7: Auto-Start, Eskalation, Plan-/Tageslimit (CP-A8), Reiter „Agents“ und Graph (CP-A9, CP-A10).

**Gebaut (7a):**
- **Ziel des Plans:** `plans.goal` (Migration 19, `PlanFields.goal`, höchstens 16 KiB; CLI `board plan new --goal`, `plan edit --goal`). `PlanFields` ist ein Full Replace; das CLI (`plan edit`) und das Tauri-Kommando `update_board_plan` (`PlanUpdate`) behalten das Ziel, wenn es nicht genannt wird. Das Ziel schreiben darf im CLI nur der Owner (`plan new --goal`, `plan edit`): ein Agent, der es schreiben könnte, schriebe den Auftrag des Planers.
- **Sitzung für einen Plan:** `ide_agents.plan_id` (Migration 18) neben `card_id`; ein Planer hat `start_ref` = den Commit, auf dem `HEAD` beim Start stand (`worktree::head_commit`), und `provision::prepare` schneidet für ihn denselben losgelösten Worktree wie für den Reviewer.
- **Rolle `planner`** (gesät wie `allrounder`/`reviewer`, `kind: plan`, Stufe schwer, **ohne feste Engine**: der Owner wählt das Modell bei jedem Start). Anweisungstext: `axiomata_roster::planner_role` (A24: dem Owner vor dem Commit vorgelegt).
- **MCP-Werkzeug `get_plan`** (nur Rollen der Art `plan`): Name, Ziel, Status des Plans, der **Rollenkatalog** (`RoleEntry`: Name, Beschreibung, Art, Stufe — die Rollen, die für das Projekt gelten) und die Karten des Plans bisher. Der Plan kommt nur aus `AXIOMATA_PLAN_ID` im MCP-Eintrag (nie aus einem Argument), `create_card` hängt seine Vorschläge schon seit CP-A4 an diesen Plan.
- **`Launch`** (`agent_entry`): wofür das Studio eine Sitzung gestartet hat — `Card(CardLaunch)` oder `Plan(PlanLaunch)`; `read_only()` gilt für Reviewer und Planer (Claude: kein `acceptEdits`, `Edit` verboten, `--strict-mcp-config`; Opencode: `edit: deny`). Der Start-Prompt des Planers nennt nur Namen und Plan-Nummer, das Ziel kommt über `get_plan` (nie in eine Shell-Zeile). `ide_start::launch_of` liest es aus der Zeile der Sitzung; ein Planer ist **gewollt, solange sein Plan ein Entwurf ist** — ein neu gestartetes Pane eines freigegebenen Plans schlägt keine Karten mehr in ihn.
- **Planer starten** (`plan_session::start_plan_session`, CLI `board plan start <plan> --project <id> --engine <id>`, Tauri `start_plan_session`): dreiphasig wie der Reviewer (Fragen unter der Sperre, `git rev-parse HEAD` ohne, Anlegen unter der Sperre mit erneuter Prüfung). Verweigert: kein Entwurf, ein Projekt ohne Repository oder ohne Commit, kein `axiomata-cli`, keine Rolle der Art `plan`, keine/ungültige Engine (auch eine mit eigenem Befehl), ein Plan, der schon einen Planer hat.
- **Aufräumen:** Freigeben, Schließen und Löschen eines Plans beenden seine Planer-Sitzungen (`forget_plan_sessions`: Worktree, Geheimnis, Kanal, Opencode-Eintrag); Tauri-Kommandos dafür sind jetzt `async`.
- **Limits:** die CP-A6c-Limits gelten auch für Planer; der Wächter kennt Sitzungen eines Plans (`Breach.plan_id`); ein Stopp eines Planers hat keine Karte, an die er schreibt — der Owner erfährt es über den Hinweis.

**Aus Review und Security-Audit von 7a eingearbeitet:** (1) `get_plan` und `create_card` verlangen, dass der Plan der **eigenen Zeile** der Sitzung entspricht (nicht nur der Umgebung) und noch ein **Entwurf** ist — ein Planer, dessen Harness nach der Freigabe weiterläuft, schlägt nichts mehr in den Plan vor; (2) **ein Planer je Plan über alle Projekte** (nicht nur je Projekt); (3) das Ziel des Plans ist im CLI **owner-only**; (4) Opencode: `git * --no-index*` und `git * --output*` sind für Reviewer und Planer verboten (nach den Erlaubnissen; die Reihenfolge-Annahme „die spätere Regel gilt“ ist am echten Dienst noch nicht geprüft); (5) Tests für Worktree des Planers (losgelöst, auf dem Start-Commit auch nach neuen Commits), Claude-Optionen und `AXIOMATA_PLAN_ID`, Opencode-Rechte, `head_commit`, die beiden Prüfungen in `finish`, den Stopp eines Planers ohne Karte, das Ziel beim Plan-Update.
**Bekannte Lücken (7a, bewusst):** (a) `forget_plan_sessions` beendet **keinen laufenden Harness** — das Pane des Planers ist Sache des Studios (7b schließt es); der Server lehnt nach der Freigabe aber alles ab (1), und der Wächter kennt die gelöschte Zeile nicht mehr; (b) ein Pane-Start kann mit `forget_plan_sessions` um den Worktree laufen (dasselbe Fenster wie beim Reviewer, `start_lock` gilt nur für Starts); (c) Opencode liest die eigene Projektkonfiguration (`opencode.json`, `.opencode/`, `AGENTS.md`) im Worktree — beim Planer ist das der `HEAD` des Projekts; (d) ein Claude-Planer bekommt kein `Bash` ohne Rückfrage und hält bei `git log` an; (e) `AXIOMATA_AGENT_ID` aus der Umgebung entscheidet über „owner only“ im CLI (A39).

**Gebaut (7b, 2026-10-05) — der Flow-Modus im Studio:**
- **Drei Modi** *Editor | Canvas | Flow* (`ide/modes.ts`): `Mode = "editor" | "agents" | "flow"` (die Kennung `agents` bleibt gespeichert, die Kopfzeile nennt sie **Canvas**, `MODE_LABEL`). Ein Projekt hat **ein Layout je Modus** in `layout_json` (`layouts.flow` ist neu; eine Zeile aus der Zwei-Modi-Zeit bekommt das Start-Layout des Flow), `switchMode(ws, next)` parkt das gezeigte Layout, die nicht gezeigten bleiben gemountet (ein laufender Planer stirbt nicht, wenn der Owner in den Editor schaut). `projectSession.parked` ist jetzt `Partial<Record<Mode, Layout>>`; `IdeView` hält die Panes aller geparkten Layouts (`parkedLayouts`).
- **Pane-Typ `plan`** (`PLAN_PANE`, `PlanPane.svelte`, `ide/planning.ts` mit Tests): das Start-Layout des Flow ist ein einzelner Reiter „Planung“. Links die **Pläne des Bretts** (Brettwahl je Projekt gemerkt) und „Neuer Plan …“ (Name, Ziel); rechts der offene Plan: das **Ziel** (bearbeitbar im Entwurf, gespeichert beim Verlassen des Feldes), der **Planer** (Engine wählen → „Planer starten“, danach „Pane zeigen“; ohne Ziel kein Start), die **Vorschläge** mit Rolle und Stufe (änderbar, `update_card` mit allen Feldern), „braucht #…“, Begründung, Abnahmekriterien, einzeln **Annehmen** / **Verwerfen** (zweistufig), und **Plan freigeben** / **Schließen**. Solange ein Planer an einem Entwurf arbeitet und der Reiter sichtbar ist, wird das Brett alle 3 s neu gelesen (der Planer schreibt aus einem anderen Prozess).
- **Der Planer-Pane** öffnet im Flow neben der Planung (`showRequestedAgents`: eine Sitzung mit `plan_id` öffnet im Flow, alle anderen im Canvas; `IdeAgent.plan_id` ist neu). Freigeben oder Schließen eines Plans **schließt** seinen Planer-Pane (Ereignis `studio:close-agent-panes` → `IdeView`; das Backend hat die Sitzung dann schon vergessen).
- Browser-Mock (`core/devmock.ts`): `start_plan_session`, Freigabe beendet den Planer; im Browser gegen `vite` geprüft (Moduswechsel, Plan anlegen, Planer starten und sein Pane, Schließen per Ereignis).

**Aus dem Review von 7b eingearbeitet:** (1) das Flow-Layout bekommt sein **Planungs-Tab zurück**, wenn es keines hat — beim Laden und bei jedem Zeigen des Flow (`withPlanPane`; ein ganz leeres gespeichertes Layout liest `parseLayout` ohnehin als beschädigt und nimmt das Start-Layout); (2) **ein Pane je Planer**: `openAgent` sucht die Sitzung in allen Layouts und schaltet in den Modus, in dem sie schon offen ist, statt einen zweiten Harness zu starten; (3) `studio:close-agent-panes` schließt auch in den **geparkten** Layouts (`closeParkedAgentTabs`); (4) „Rolle/Stufe ändern“ liest die Karte vorher frisch (ein Planer kann ihren Text inzwischen umgeschrieben haben) und setzt das Auswahlfeld zurück, wenn die Änderung nicht ging; „Verwerfen“ prüft vorher, dass die Karte noch ein Vorschlag ist; (5) das **Ziel** wird beim Verlassen des Feldes, beim Wechsel des Plans, beim Schließen des Panes und vor dem Planer-Start geschrieben (nicht an `busy` gebunden), ein von außen geändertes Ziel überschreibt eine Eingabe nicht; (6) offener Plan und Engine sind abgeleitet statt in `$effect` geschrieben; Ladefehler werden gezeigt; „Plan freigeben“ geht auch, wenn alle Vorschläge einzeln angenommen wurden.

**Nicht in 7b (bewusst):** der Reiter „Agents“ und die Flowansicht (CP-A9, CP-A10); ein Ring-Eintrag, der das Studio **im Flow** öffnet (`shell:ide` öffnet weiter im Canvas); Ziel und Planer-Start auch im Kanban (dort legt „Neuer Plan“ weiter ohne Ziel an, das Ziel schreibt der Owner im Flow); Komponententests für `PlanPane` (der Projektstand testet reine TS-Logik, die Oberfläche am Browser und am Mac).

## CP-A8 im Detail (8a gebaut 2026-10-05)

**Zuschnitt und Entscheidungen (Owner, 2026-10-05):** CP-A8 in drei Stufen — **8a** der Plan läuft von selbst (hier), **8b** „Plan übernehmen“ mit einem Klick, **8c** Eskalation nach zwei Rückgaben sowie Limit je Plan und je Tag. Vom Owner: das Übernehmen ist **ein Klick je Plan** (nicht je Karte, nicht vollautomatisch); parallel läuft **so viel, wie die Abhängigkeiten zulassen** (nicht „bis N“) — nur eine Obergrenze für die Maschine bleibt (`agents.max_parallel_sessions`, Standard 4, A25: bisher 3, auf Wunsch größer; Worker und Reviewer zählen je eine Sitzung, über alle Pläne).

**Abweichung von A16 (gestapelte Zweige) — die Integrationslinie:** jeder Plan, der von selbst läuft, hat einen **eigenen Zweig `axiomata/line/<id>` mit eigenem Worktree** (`<worktrees>/<projekt>/.lines/<id>`), geschnitten aus dem Zweig, auf dem das Projekt beim ersten Start stand (`plans.base_branch`). Der Review-Ja einer Karte **integriert** sie dort hinein (Squash, ein Commit je Karte, `#<karte> <titel>`); Karten, die auf anderen aufbauen, starten **von der Linie** (ihr Zweig wird daraus geschnitten, `base_branch` der Sitzung = die Linie, also ist auch die Review-Basis die Linie). Warum nicht gestapelte Agent-Zweige: eine Karte, die zwei andere braucht (Plan #1: #61 braucht #59 und #60), müsste von einem Merge zweier Agent-Zweige starten; ein Squash des ersten nach main kollidiert mit der Historie des zweiten (A16 sah das selbst); und zwei Karten, die dieselbe Datei ändern, fielen erst beim Übernehmen des Owners auf. Hier zeigt es sich beim Integrieren der zweiten, dem Studio. Alles, was git im Worktree eines Agenten oder in der Linie selbst tut, läuft **ohne Hooks** (`git_agent`; die Linie enthält Agenten-Inhalt); nur das Übernehmen in den eigenen Zweig des Owners (8b) läuft mit den Hooks seines Projekts, wie jedes Take-over.

**Datenmodell (Migration 20, dazu 21 für `plans.line_tip`):** `plans.project_id` (das Projekt, in dem der Plan läuft; der Flow setzt es beim Anlegen, ein Plan ohne Projekt läuft nicht von selbst), `plans.base_branch`, `cards.integrated_at`; Zustand `TaskState::Integrated` („im Plan“), Ereignis `integrated`. Ein Plan **läuft von selbst** (`card_session::runs_by_itself`), wenn er freigegeben ist, `auto_start_max` gesetzt hat und ein Projekt (der Flow setzt beim Freigeben 64 = kein eigenes Limit; die Spalte bleibt als Plan-Obergrenze, `board plan edit --auto N`).

**Der Ablauf (`axiomata_core::plan_run`, alle 5 s in `plan_watch.rs` der App):** 1. eine vom Reviewer abgezeichnete, noch nicht integrierte Karte wird **integriert** (`card_session::integrate_card`); 2. eine **bereite** Karte (offene Spalte, alles, worauf sie aufbaut, ist auf der Linie) wird **gestartet**, solange die Obergrenze der Maschine und die des Plans reichen, in Reihenfolge des Bretts; 3. ein Plan, dessen Karten alle auf der Linie sind, wird einmal als „bereit zum Übernehmen“ gemeldet. Ereignis `plan:run` an das Studio: eine gestartete Karte öffnet ihr Pane **im Hintergrund** (das Öffnen startet den Harness; `AgentRequest.background`, `addTabParked`: der Owner bleibt im Modus, in dem er ist), die Panes integrierter Karten schließen, Meldungen. Der Reviewer braucht keinen Schritt (`card_watch`). **Ein Plan läuft, solange die App und ihr Studio offen sind.** Eine Karte, die nicht starten oder integriert werden kann (keine Engine, geänderte Arbeit), wird einmal gemeldet und 5 Minuten in Ruhe gelassen.

**Integrieren** (`integrate_blocking`): wie das Take-over — das Gate „**genau das Geprüfte**“ (`gate_reviewed`, aus `squash` herausgezogen), dazu: ist der Worker noch im Zug (Arbeitend/Wartend), passiert nichts bis zum nächsten Blick. Danach `plan_line::integrate` (`merge --squash`, `commit --no-verify`, ohne Hooks): **Fertig** → Karte `integrated`, ihre Sitzungen (Worker, Reviewer) samt Worktrees und Zweigen aufgeräumt; **leer** (die Arbeit ist schon auf der Linie) wie fertig ohne Commit; **Konflikt** (zwei parallele Karten änderten dieselbe Stelle) → die Linie bleibt, wie sie war, die Karte wird **zurück nach Offen** gesetzt (`flow::reset_for_rework`: Anspruch und Signatur weg, Ereignis `released` mit „conflict: …“) und startet von selbst neu **auf der Linie, wie sie jetzt ist**; beim **zweiten** Konflikt derselben Karte gibt das Studio auf: die Karte bleibt geprüft und nicht integriert, die Begründung steht im Verlauf, ihre Sitzung bleibt zum Ansehen — das entscheidet der Owner (CLI `board integrate <karte>` versucht es erneut). Eine abgezeichnete Karte kann nicht „scheitern“ (Board-Regel), deshalb kein `failed`.

**Sonst:** das Übernehmen einer **einzelnen** Karte eines solchen Plans ist verweigert (die Linie wird als Ganzes übernommen, 8b); Start einer Karte eines solchen Plans von Hand (Kanban „Starten …“) läuft über dieselbe Linie und verlangt das Projekt des Plans; CLI `board integrate` (nur Owner), `board plan new|edit --project` (nur Owner); Freigeben und Einstellung in einem Schritt (`approve_board_plan(id, run_by_itself, project_id)`); `open_card_sessions` beim Seitenstart holt Panes nach, deren `started`-Ereignis verloren ging; im Flow „Automatisch abarbeiten“ (Standard an) beim Freigeben und „läuft automatisch“ am Plan.

**Grenzen der Kosten (M3):** `max_cost_usd`/`max_tokens` eines Plans werden von der Selbstlauf-Schleife **nicht** durchgesetzt (nur die Limits je Sitzung aus 6c und die Tagesgrenze der Provider); das Plan- und Tageslimit gehört zu 8c.

**Nachgebessert nach dem Review (2026-10-05):** das Gate liefert den **geprüften Commit** und genau der wird gemergt (kein zweites Lesen des Zweigs); `git worktree add/prune` laufen mit den Agent-Optionen (keine Hooks); `unsafe_config` prüft vor dem Gate (im Agent-Worktree, bevor dort `add -A` läuft) und liest bei `extensions.worktreeConfig` auch die Worktree-Config, dazu `core.worktree` und `submodule.*.update`; ein Linienstand, der der Aufzeichnung nur um **eigene Studio-Commits** (`#<karte> <titel>`) voraus ist (Absturz zwischen Commit und Aufzeichnung), wird wieder aufgenommen, eine Linie ohne Aufzeichnung nur, wenn nichts Eigenes auf ihr liegt; „gave up“-/„conflict“-Zählung nur für Zeilen des Akteurs `agent:studio`; ein laufender Plan meldet einmal je Karte, dass sie auf eine gescheiterte oder abgebrochene Karte baut, und gilt auch mit abgebrochenen Karten als fertig; ein Pane einer Karte in einem anderen als dem offenen Projekt erscheint, sobald dieses Projekt aufgeht (`open_card_sessions`).

**Bekannte Grenzen (bewusst, gehören zu 8c oder bleiben):** eine Karte, bei der das Studio aufgegeben hat, hält ihre beiden Sitzungen und blockiert den Plan bis zur Entscheidung des Owners (bisher nur `board integrate` im CLI, keine Oberfläche); ein hängengebliebener „Working“-Eintrag im Kanal eines abgestürzten Harness hält eine Karte für immer auf „Busy“ (kein Zeitlimit); ein Absturz zwischen `merge --squash` und `commit` lässt Änderungen auf der Linie stehen, die das Studio nicht anrührt (Owner: `git reset --merge` im Linien-Worktree); die Owner-Sperren des CLI hängen an `AXIOMATA_AGENT_ID` (kein Sandkasten, A39); das Take-over einer einzelnen Karte (nicht der Linie) squasht weiter nach Zweigname.

**CP-A8b gebaut (2026-10-05): „Plan übernehmen“.** Im Flow erscheint der Knopf, sobald jede Karte des Plans auf der Linie liegt (abgebrochene zählen nicht; `readyToTakeOver`). Ein Klick: `card_session::take_over_plan` prüft dasselbe noch einmal, die Linie steht, wo das Studio sie ließ (`check_line_tip`), und `plan_line::take_over` bringt sie in den Zweig des Projekts — **die Commits der Karten bleiben einzeln** (Fast-Forward, `TakeOverMode::Linear`), hat sich der Zweig inzwischen bewegt, gibt es **einen Merge-Commit** „Plan #<id>: <name>“. Ein Konflikt mit dem Stand des Zweigs wird zurückgenommen und mit den Dateien gemeldet (keine automatische Auflösung); abgelehnt wird bei einer Config mit Merge-Programm (`unsafe_config`), einer anderen ausgecheckten Branch im Projektordner, Gestagtem dort, einer bewegten oder schmutzigen Linie. Danach: Karten als übernommen markiert und archiviert, Plan geschlossen, Linien-Worktree und -Zweig entfernt (nur `axiomata/line/<ziffern>`); was sich nicht entfernen lässt, ist ein Hinweis, nie ein Fehler. Gepusht wird nie; der Hinweis „noch nicht gepusht“ erscheint wie beim Übernehmen einer Karte. CLI: `board take-over-plan <id>` (nur Owner). Tauri-Befehl `take_over_plan`.

**CP-A8c-1 gebaut (2026-10-05): Kostenbuch, Limit je Plan und je Tag.** Zuschnitt vom Owner bestätigt (Startwerte bleiben, A27); 8c in drei Commits — **8c-1** Kostenbuch + Plan-/Tageslimit (hier), **8c-2** aufgegebene Karte lösen + Eskalation (genau ein Versuch auf die nächststärkere Rolle, sonst entscheidet der Owner), **8c-3** Tiefe/Anzahl selbst angelegter Karten.

- **Das Buch** (`axiomata_core::studio_spend`, Migration 22 `session_spend`): die Sitzungs-Zahlen leben nur, solange die Sitzung lebt (Transkript, Opencode-DB) und verschwinden beim Integrieren; ein Limit über einen Plan oder Tag braucht also eine Summe, die bleibt. `Meter::check` (alle 5 s, `limit_watch.rs`) schreibt je Blick **die Zunahme** seit dem Blick davor (`record_look`, Zeile je Zunahme, nur anhängen, nie zurücknehmen; ein gefallener Messwert schreibt nichts). Eine Sitzung, die zwischen zwei Blicken verschwindet, verliert höchstens ihre letzten ≤5 s. Gebucht wird auf den **Plan der Karte** (bzw. des Planers); Reviewer zählen mit.
- **Zwei Zahlen, bewusst getrennt:** **Token** zählen für jede Engine; **Dollar** nur für Engines mit Abrechnung je Token **und** Preis in `config.agents.costs`. Ein fehlender Preis ist „nicht gemessen“, nie „kostenlos“ — das Dollar-Limit eines Plans oder Tages hält also nur, was sich bepreisen ließ; das Token-Limit des Plans hält alles.
- **Plan-Limit** (A9/A27): `plans.max_cost_usd`/`max_tokens` gewinnen, sonst `agents.plan_max_cost_usd` (15 $) und `agents.plan_max_tokens` (6 M). Erreicht → `plan_work` startet für diesen Plan **nichts mehr** (Integrieren und „fertig“ laufen weiter, das kostet nichts; laufende Sitzungen enden ihre Karte, begrenzt von den Limits je Sitzung), einmal `RunEvent::Paused`. **„Weiter“** (`studio_spend::resume_plan`, CLI `board plan resume`, Tauri `resume_plan`, Knopf im Flow) setzt beide Limits des Plans auf **bisher Verbrauchtes + Standard**; der Plan startet beim nächsten Blick wieder, eine zweite Pause wird wieder gemeldet.
- **Tageslimit** (A9, „die Reißleine“): `agents.studio_daily_usd_cap` (20 $, `None` = aus), über die Zunahmen des lokalen Tages, **getrennt** vom `daily_usd_cap` der Skills/des Chats. Erreicht → kein Plan startet etwas (`RunEvent::DayCapReached`, einmal) **und laufende Sitzungen mit Abrechnung je Token werden wie an einem eigenen Limit gestoppt** (derselbe Marker/Interrupt wie 6c, Karte bleibt beansprucht, Hinweis im Verlauf „Tageslimit anheben oder bis morgen warten“). Abo-Sitzungen werden nicht gestoppt: sie verbrauchen keine Dollar. Mit dem nächsten Tag (oder angehobenem Cap) hebt sich der Stopp von selbst.
- **Anzeige:** Im Flow zeigt ein Plan, der von selbst läuft, „Verbrauch: x von y Token · $a von $b“ (Dollar nur, wenn etwas bepreist wurde), bei Pause den Grund mit „Weiter“, bei erreichtem Tageslimit den Hinweis auf `studio_daily_usd_cap`; Toasts für beide Ereignisse. CLI `board plan spend <id>` zeigt dasselbe.
- **Nach dem Review nachgebessert:** Reviewer starten bei erreichtem Tageslimit nicht (`card_watch`; der Bericht gilt nicht als versucht, der Review startet nach dem Anheben), bei einem Plan-Limit **schon** (die Arbeit ist getan, der Review lässt sie integrieren); das Tageslimit stoppt nur Sitzungen mit **bepreisten** Dollars (`stopped_by_day`: Abo, unbepreist, unlesbar ⇒ nichts zu sparen); „Weiter“ senkt nie ein Limit (`max(bisheriges, Verbrauch + Standard)`) und wird verweigert, wo kein Limit erreicht ist; ein Plan ohne offene Karte meldet keine Pause.
- **Grenzen:** das Buch zählt nur, solange die App (und ihr `limit_watch`) läuft — wie die Limits je Sitzung; eine Sitzung, die nicht lesbar ist (`measured: false`), wird nicht gebucht; ein Messwert, der bei null neu anfängt (neue Claude-Session-Id statt Fortsetzung, neu gestarteter Meter nach verschobenem Opencode-Fenster), verbirgt Verbrauch, bis er die alte Summe überholt (nur Unterzählung, nie Überzählung); ein Plan, der Karten von Hand startet, wird im Buch geführt, aber nicht pausiert (nur Pläne, die von selbst laufen, prüft `PlanRun`).

**CP-A8c-2 gebaut (2026-10-05): Eskalation und die aufgegebene Karte.**

- **Eskalation nach der zweiten Rückgabe (A26), genau ein Versuch.** `PlanRun::tick` sieht eine Karte eines von selbst laufenden Plans in Arbeit mit `returned_count == 2` und ruft `card_session::escalate_card`: Es sucht die **schwächste stärkere Rolle gleicher `kind`** (höhere `tier`, arbeitet Karten, hat eine Engine ohne eigenen Befehl; Gleichstand nach Name). **Die Sitzung wechselt Rolle und Engine an Ort und Stelle** (`roster::update_agent_on_engine`, Name und Zeile bleiben): Worktree, Zweig, Claim und Akteur bleiben, es gibt keine Übergabe von Claim und Worktree zwischen zwei Zeilen — das war die einfachere und weniger fehleranfällige Auslegung von „die neue Sitzung übernimmt“. Die Opencode-Sitzung wird vergessen (die nächste ist neu) und, falls der Dienst noch im Zug ist, unterbrochen. Verlauf: `escalated` mit „von … auf …“. Das Studio schließt das Pane und öffnet es neu (`RunEvent::Escalated`, `endedSessions` + `requestAgent` in `App.svelte`): das Öffnen startet die neue Engine auf der vorhandenen Arbeit; der Start-Prompt sagt jetzt, dass der Verlauf der Karte (`get_card`) enthält, was der Reviewer falsch fand. **Der Versuch ist einmalig**: eine `escalated`-Zeile des Studios im Verlauf (auch die „keine stärkere Rolle“) verhindert jeden weiteren, über Neustarts hinweg.
- **Keine stärkere Rolle (oder keine mit lauffähiger Engine):** die Karte bleibt, wie sie ist, einmal `Blocked` mit „entscheide du“. **Wird sie ein drittes Mal zurückgegeben**, meldet `escalation_work` einmal, dass auch die stärkere Rolle es nicht geschafft hat (weiter / übernehmen / abbrechen — der Owner entscheidet).
- **Die Limits der neuen Rolle** gelten für die **ganze Karte**: die Summe der Claude-Sitzungs-Ids einer Karte zählt (6c), die früheren Versuche der schwächeren Rolle also mit. Opencode zählt je Sitzung neu.
- **Aufgegebene Karte lösen** (zweimal Konflikt mit der Linie): im Flow, in „Karten des Plans“, zeigt eine solche Karte „Passt zweimal nicht … das Studio hat aufgegeben“ mit **„Erneut integrieren“** (`integrate_card` mit `owner = true`, wie `board integrate`) und **„Neu machen“** (`card_session::redo_card`, CLI `board redo`): Anspruch und Signatur weg, Karte zurück nach Offen, ihre Sitzungen samt Worktrees aufgeräumt, der Plan startet sie auf der Linie, wie sie jetzt ist. Das Aufgeben ist jetzt `gave_up_on` = Zahl der „gave up“-Notizen des Studios **größer als** die der „redo requested by the owner“-Notizen: nach „Neu machen“ behandelt das Studio die Karte wieder wie jede andere, ein weiterer Konflikt legt sie sofort wieder in die Hände des Owners. `PlanRun` vergisst das Aufgeben einer Karte, sobald sie nicht mehr „abgezeichnet, nicht integriert“ ist. Tauri `integrate_card`, `redo_card`, `plan_cards_left_for_owner`.
- **Nach dem Review nachgebessert:** „Neu machen“ setzt die Rückgabe-Zählung der Karte auf null (`flow::clear_returned_count`; der Verlauf behält die Zeilen) und die Meldung „mehrfach zurückgegeben“ behauptet nicht mehr, eine stärkere Rolle habe es versucht; `cards_left_for_owner` blendet übernommene, gescheiterte und abgebrochene Karten aus.
- **Grenzen:** die Eskalation gilt nur für Pläne, die von selbst laufen (Karten von Hand entscheidet der Owner ohnehin); ein Worker, der mitten in der Zeile ist, wird mitten im Zug ersetzt (Pane zu, Arbeit im Worktree bleibt, der neue Start prüft `git status`); Claude-Code-Harnesses der alten Sitzung enden mit dem Schließen des Panes, ein Opencode-Dienst per Unterbrechung.

**CP-A8c-3 gebaut (2026-10-06): Tiefe und Anzahl selbst angelegter Karten (A7).** Zusätzlich zu den 20 Vorschlägen je Sitzung (CP-A4) gelten zwei Grenzen, beide in der Board-Transaktion `flow::propose_card_from`, also für jeden Aufrufer und nicht erst im Studio: **Tiefe 2** — eine Karte, die eine arbeitende Sitzung für ihre Karte vorschlägt, liegt eine Ebene unter dieser (Tabelle `card_proposal_depth`, Migration 23; ohne Zeile = Tiefe 0: Planer- und Owner-Karten); die Sitzung an einer Tiefe-2-Karte schlägt nichts mehr vor; und **30 Karten je Plan** von arbeitenden Sitzungen zusammen (die Karten des Planers zählen nicht, dafür wurde der Plan freigegeben). Die Ablehnung nennt die Grenze und was stattdessen zu tun ist (Verlauf der Karte oder Nachricht an den Owner); `create_card` (MCP) reicht dafür die eigene Karte der Sitzung als `parent` durch. Konstanten `MAX_PROPOSAL_DEPTH`, `MAX_SELF_PROPOSED_PER_PLAN` im Board (nicht einstellbar; die Startwerte bleiben, solange der Owner nichts anderes will).

**CP-A8 ist damit vollständig (8a–8c).** Bekannte Grenzen: ein Opencode-Worker meldet seinen Zustand nicht in den Kanal, die Prüfung „noch im Zug“ greift dort nicht; die Konfliktzahl wird aus dem Verlauf gezählt (`released`-Zeilen mit „conflict:“).

## CP-A9 im Detail (gebaut 2026-10-06)

**Zuschnitt (Owner, 2026-10-06):** der Reiter „Agents“ im Flow soll wie das Canvas zeigen, **was ein Agent gerade tut** — kein Katalog, keine bloße Liste; die Ausgestaltung überließ der Owner dem Bau („überrasch mich“), UI/UX wird nach dem Bau in Runden verfeinert. Die dritte Flow-Sicht ist weiter die **Flowansicht** (Graph, CP-A10). Kein Rollenkatalog im Pane (Rollen und Engines pflegt der Studio-Inspektor).

- **Leitstand statt Liste:** `TeamPane` (Pane-Typ `team`, Tab „Agents“, `ide/panes/TeamPane.svelte`, reine Daten in `ide/team.ts`) steht im Flow **rechts neben „Planung“** (`withFlowPanes`: beim Laden und beim Umschalten wiederhergestellt, wie das Planungs-Pane; wer es in die Planungsgruppe zieht, behält es dort). Eine **Kachel je Studio-Sitzung** (Arbeiter, Reviewer, Planer; von Hand gemachte Sitzungen bleiben dem Canvas), gruppiert nach dem Plan ihrer Karte: Status-Punkt, Name, Rolle, Engine, ✉-Zähler, „arbeitet an / prüft #n Titel“ + Zustand, eine **Live-Zeile** mit dem neuesten Schritt (`Bash cargo test -p core · vor 5 s`) und, aufgeklappt, die letzten acht Schritte, der Verbrauch gegen die Limits (`CardUsage`) und die Nachrichten. „Terminal“ holt das Pane der Sitzung nach vorn. Rückfragen (`input_required`) rahmen die Kachel orange und zeigen die Frage; Reihenfolge: Rückfrage, in Arbeit, im Review, …
- **Was ein Agent tut — gelesen, nicht gefragt:** keines der Harnesses hat eine „was machst du“-Schnittstelle, beide schreiben aber jeden Schritt mit. `axiomata_ide::activity` (rein) zerlegt das in `Activity { Tool|Say, name, detail, at }`: *Claude Code* — die **letzten 256 KiB** des Transkripts (`tail_of`: ab Zeilenanfang, kein Symlink; `find_transcript` über die UUID-geprüfte Sitzungs-Id, die neueste der Sitzung), `tool_use` mit Ziel (`command`, `file_path`, `path`, `pattern`, …, eine Zeile, 160 Zeichen) und die erste Zeile des Gesagten; *Opencode* — die neuesten 20 Nachrichten (`recent_messages`, neueste zuerst, `name`/`tool` und `input`/`state.input` je nach Dienstversion). `axiomata_core::session_activity::read` (Tauri `session_activity`, alle 3 s solange das Pane gezeigt wird, höchstens 32 Sitzungen, Opencode mit 4-s-Zeitlimit) liefert je Sitzung `readable` — „nichts aufzeichnet“ ist nicht „untätig“. Der Text stammt von Modell und Werkzeugen: er wird dem Owner nur angezeigt, nie in eine Befehlszeile gesetzt, nie als HTML.
- **Nachrichten einsehbar, Owner schreibt:** `session_mail::conversation` (Tauri `ide_mailbox_messages`) = was die Sitzung erreichte (`read_inbox` mit `mark_read = false`) und was sie schrieb (`mailbox::sent_by`), älteste zuerst; **Ansehen markiert nichts als gelesen** (das tut der Agent mit `read_inbox`). `ide_mailbox_unread` zählt Ungelesene je Sitzung (✉ an der Kachel). `session_mail::send_as_owner` (Tauri `ide_mailbox_send`) schreibt als Owner an die Sitzung, zur Karte, an der sie arbeitet; das Anstupsen übernimmt der vorhandene `ide_mailbox_nudge`, sobald die Sitzung wartet; eine beendete Sitzung speichert die Nachricht und sagt es („niemand liest es“). Dieselbe Komponente (`SessionMail.svelte`) steht im **Inbox-Reiter** jeder Sitzung unter „Team tools“.
- **Nach dem Review nachgebessert:** Opencode-Zeiten (Epoch-Millisekunden) werden RFC 3339; über 32 Sitzungen hinaus bekommt jede weitere einen Eintrag „nicht lesbar“ statt keinen; ein Opencode-Dienst, der einmal nicht rechtzeitig antwortet, wird im selben Blick nicht noch einmal gefragt, und das Frontend lässt nie zwei Blicke gleichzeitig laufen; die Polls hängen am Schlüssel der Sitzungs-Ids (nicht an einem neuen Array je Neuberechnung); die Board-Abos enden mit dem Effekt; das Transkript-Ende ist 1 MiB groß (eine große Werkzeug-Antwort ist eine Zeile für sich); gleiche Nachrichten-Ids in Ein- und Ausgang kollidieren nicht mehr.
- **Bekannte Grenzen:** `claude_sessions()` behält höchstens 100 Ids (die ältesten): eine Sitzung mit mehr als 100 Neustarts liest ein altes Transkript; die Feldnamen der Opencode-Werkzeugteile (`name`/`tool`, `input`/`state.input`) sind nach Version geraten und gegen einen echten Dienstaufruf noch nicht geprüft (nur die Form `type: "tool"` ist belegt); der Aufruf der Live-Zeile liest je Blick einmal pro Sitzung.
- **Nicht gebaut (bewusst):** die Flowansicht (CP-A10); Nachrichten zwischen zwei Sitzungen als eigener Strom (sie stehen in beider Unterhaltung); ein Livebild des Terminals in der Kachel (das Canvas zeigt es); Verbrauch aller Kacheln auf einmal (er wird je Kachel beim Aufklappen gelesen, ein Lesen hat die Kosten eines Transkript-Durchgangs); Komponententests der Svelte-Dateien (reine Logik ist in `team.ts`/`activity.rs` getestet, die Oberfläche am Browser-Mock mit `agent-browser`; am Mac mit echten Agenten steht aus).

## CP-A10 im Detail (gebaut 2026-10-06)

**Die Flowansicht** (Pane-Typ `graph`, Tab „Flowansicht“, `ide/panes/GraphPane.svelte`; Geometrie in `ide/flowGraph.ts`) — die dritte Flow-Sicht aus A6/A6a, als Tab **in der Gruppe der Planung** (`withFlowPanes`: Planung + Flowansicht links als Reiter, Agents rechts; jedes wird beim Laden und Umschalten wiederhergestellt). Gebaut nach A6, nichts darüber hinaus:

- **Knoten = Karten des gewählten Plans** (auch archivierte/übernommene: ein fertiger Plan zeigt, was er tat; Vorschläge gestrichelt), **Linien = „braucht zuerst“** (Kurven von rechts nach links mit Pfeil), **Farbe = Zustand** in sechs Familien aus den Theme-Tokens (bereit/wartet, in Arbeit, Rückfrage, im Review, geprüft/im Plan, gescheitert), ein grüner Punkt = die Karte hat eine lebende Sitzung, „n× zurück“ = so oft zurückgegeben. **ein Startknoten** links, aus dem jede Karte ohne Vorgänger wächst (`START` in `flowGraph.ts`), und der Baum **hängt in der Höhe um die Mittellinie** (jede Spalte mittig zur höchsten, der Graph mittig im Pane; `margin: auto`, damit ein größerer von oben links scrollt); **Anordnung automatisch** (`layersOf`: Spalte = Länge der längsten Kette davor; Reihenfolge je Spalte nach dem Schwerpunkt der Zeilen dessen, was die Karte braucht, damit sich zwei Ketten nicht kreuzen), SVG, keine Bibliothek, kein Kabel-Editor (E3): Abhängigkeiten legen der Planer und das Brett an.
- **Plan-Auswahl:** die **Planliste links in der Planung** bestimmt für alle drei Bereiche, was zu sehen ist (`ide/flowSelection.ts`: die Planung schreibt Brett und Plan, Agents und Flowansicht folgen und haben keinen eigenen Auswähler); das Brett wird bei sichtbarem Pane alle 5 s neu gelesen (Karten bewegen sich aus anderen Prozessen).
- **Klick auf einen Knoten** (auch per Tastatur): hebt seine Linien hervor und öffnet unten die Karte — Zustand, Rolle/Stufe, „braucht zuerst“, die Rückfrage, die **Sitzungen der Karte mit „Sitzung öffnen“**, der **Verlauf** (letzte acht Zeilen) und, für eine Karte, bei der das Studio aufgegeben hat, **„Erneut integrieren“ / „Neu machen“** (dieselben Befehle wie in der Planung).
- **Nicht gebaut (bewusst):** „Starten“, „Diff“, „Übernehmen“, „hochstufen“ als Knoten-Aktionen (A6 nennt sie; Starten und Übernehmen sind in der Planung und im Kanban, das Diff im Git-Reiter, und der Plan läuft von selbst — sie hier zu verdoppeln würde dieselbe Entscheidung an zwei Orten anbieten); Zoom und Verschieben per Maus (das Pane scrollt); eine eigene Darstellung für Karten, die ein Vorgänger-Plan braucht. **Mac-Test steht aus** (Plan mit echten Abhängigkeiten).

**Feste Anordnung (Owner, 2026-10-06, nach dem ersten Blick):** Planung links (mit der Planliste), **Agents rechts oben, Flowansicht rechts unten** (`withFlowPanes`: ein fehlendes Pane wird an dieser Stelle ergänzt; ein vom Owner verschobenes bleibt, wo es liegt). Die drei Panes sind **angeheftet** (`PaneTab.pinned`): kein Schließen-Kreuz, `closeTab` lässt sie stehen; Größe und Platz bleiben änderbar. **„Anordnung zurücksetzen“** (unten in der Planliste) legt sie an den Anfangsplatz zurück (`resetFlowPanes`; die Sitzungs-Panes bleiben). Verworfen: eine Kopfleiste über alles (aus einem Missverständnis des Owners Wort „über die gesamte Ansicht“) und eine ganz neue Ansicht mit festen Regionen — die Terminals der Planer und Karten-Sitzungen brauchen ein Dock als Zuhause. **Pläne löschen:** Knopf „Löschen“ in der Planung (mit „Wirklich löschen“), erlaubt für einen Entwurf und einen geschlossenen Plan sowie einen freigegebenen, solange keine Karte von einer Sitzung gehalten wird und keine Linie besteht (`canDeletePlan`; dieselbe Regel verweigert `board::flow::delete_plan`, auch für das CLI); die Karten bleiben ohne Plan auf dem Brett.

## Plan bearbeiten und grillen (Owner, 2026-10-06)

**Anlass:** „Wie ändert der User einen Plan, der nicht passt, bevor er ihn freigibt?“ und „wie bekommen wir einen Plan gegrillt?“. Entschieden (Grilling-Runde, Owner: „finde die Vorschläge gut so“): **Q1** Bearbeiten im Planungs-Pane; **Q2** der Planer darf seine eigenen Vorschläge ändern und zurückziehen; **Q3** eine gelieferte Rolle `grill` und der Knopf „Plan grillen“ (interaktive Terminal-Sitzung; ein Skill als einmaliger Lauf passt für ein Interview nicht); **Q4** das Ergebnis ist ein **Vorschlag** für ein geschärftes Ziel (`propose_goal`), den der Owner übernimmt — das Ziel bleibt seine Formulierung (A7). Offen für eine zweite Runde: Änderungen an einem schon freigegebenen, laufenden Plan; ein Hinweis „nicht gegrillt“ bei der Freigabe; Grillen einzelner Karten.

- **Q1 gebaut:** an jedem Vorschlag „Bearbeiten“ (`ide/panes/ProposalEditor.svelte`): Titel, Beschreibung, Abnahmekriterien, Rolle, Stufe und **„Braucht zuerst“** (Auswahl der anderen Karten des Plans; eine Karte, die schon — auch über Umwege — auf diese wartet, wird nicht angeboten, `needsCandidates`); Speichern schreibt die Karte (`update_card`, frisch gelesen, falls der Planer sie inzwischen umgeschrieben hat) und die Kanten (`needsDiff`: nur was sich ändert); eine Ablehnung des Bretts (Zyklus, anderer Plan) steht im Formular. Am Entwurf „Karte hinzufügen“: die Karte geht in die Vorschlags-Spalte des Plans, wie die des Planers. Ein Plan, dessen Vorschlag der Owner bearbeitet, bleibt ein Entwurf.

## Aus dem Mac-Test von CP-A6c/A7 (2026-10-05, nachgebessert)

- **Abnahmekriterien müssen verhältnismäßig sein:** der Planer schrieb „`cargo build --workspace` läuft noch fehlerfrei“ in eine README-Karte, Arbeiter und Reviewer führten es aus (Minuten, mehrere Rückfragen). Jetzt sagen es `get_plan` (wirkt sofort), der Anweisungstext der Rolle `planner` (nur für frische Installationen, gesäte Rollen werden nie überschrieben) und beide Start-Prompts: ein Bau oder Tests nur, wenn die Änderung sie berühren kann, bei Dokumentation ist das Lesen des Diffs die Prüfung.
- **Rechte einer Rolle gelten jetzt auch für Opencode** (`agent_entry::opencode_shell_patterns`): `Bash(cargo build:*)` im Feld „Rights“ der Rolle wird bei Claude Code zu `--allowedTools`, bei Opencode zu einer Shell-Regel `cargo build *`; `push`, `--output` und `--no-index` fallen immer heraus, ein Fang-alles (`Bash(*)`, `Bash`) wird nie weitergegeben. Davor kamen die Rechte nie bei einem Opencode-Reviewer an. **Die Standards der Rollen stehen weiter ohne Rechte** — was ein Agent ohne Rückfrage darf, ist die Entscheidung des Owners je Rolle.
- Die Start-Texte sagen, dass die Team-Werkzeuge keine Shell-Befehle sind (ein Claude-Haiku-Arbeiter rief `mcp call axiomata read_inbox` per Bash auf und wartete auf eine Freigabe).
- **Beim Übernehmen schließen die Panes** der Sitzungen der Karte (`studio:close-agent-panes`, wie beim Planer); die Karten-Nummer steht auf Kachel und Detail im Kanban.
- **Terminal-Start abseits des Hauptthreads** (`terminal_spawn` async, 30 s Zeitlimit): ein macOS-Hang-Report zeigte den Hauptthread 145 s lang in `Command::spawn` eines Terminals.

## Offene Fragen der Runde

1. ~~Rollen und Rechte / Aufgabenverteilung~~ — beantwortet durch A2–A5.
2. ~~Bestätigung durch den Owner~~ — beantwortet durch A7.
3. ~~Zustellung / Schleifen~~ — beantwortet durch A8. 4. ~~Kostenlimits~~ — beantwortet durch A9.
6. ~~MCP-Eintrag~~ — beantwortet durch A10.
7. ~~Eigenständiges Studio~~ — beantwortet durch A11. **Die Runde ist damit durch; der Bauplan ist freigegeben.**
