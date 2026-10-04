# Plan: Agent-zu-Agent-Kommunikation (M7.5)

Status: **Bauplan vom Owner freigegeben (2026-10-03). CP-A1 bis CP-A3 gebaut (2026-10-04); weiter mit CP-A4.** CP-A2 weicht in einem Punkt von A18 ab: `verify_card` bleibt unverändert (Fertig-Spalte); das Abzeichnen in der Review-Spalte läuft über `review_verdict` (Verschieben nach Fertig und Signatur in einer Transaktion), damit eine Signatur nie auf einer Karte liegt, die noch als „in Arbeit“ zählt.
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
- **A31 — Opencode und universeller Rückfall:** zu Beginn von CP-A5 ein **Spike** mit echtem Worktree (`opencode.json` im Worktree gegen `PUT /api/experimental/mcp/axiomata`);
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

## Offene Fragen der Runde

1. ~~Rollen und Rechte / Aufgabenverteilung~~ — beantwortet durch A2–A5.
2. ~~Bestätigung durch den Owner~~ — beantwortet durch A7.
3. ~~Zustellung / Schleifen~~ — beantwortet durch A8. 4. ~~Kostenlimits~~ — beantwortet durch A9.
6. ~~MCP-Eintrag~~ — beantwortet durch A10.
7. ~~Eigenständiges Studio~~ — beantwortet durch A11. **Die Runde ist damit durch; der Bauplan ist freigegeben.**
