# Plan: Agent-zu-Agent-Kommunikation (M7.5)

Status: **Bauplan vom Owner freigegeben (2026-10-03). Umgesetzt wird noch nichts — Beginn mit CP-A1, an einem späteren Tag.**
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

## Offene Fragen der Runde

1. ~~Rollen und Rechte / Aufgabenverteilung~~ — beantwortet durch A2–A5.
2. ~~Bestätigung durch den Owner~~ — beantwortet durch A7.
3. ~~Zustellung / Schleifen~~ — beantwortet durch A8. 4. ~~Kostenlimits~~ — beantwortet durch A9.
6. ~~MCP-Eintrag~~ — beantwortet durch A10.
7. ~~Eigenständiges Studio~~ — beantwortet durch A11. **Die Runde ist damit durch; der Bauplan ist freigegeben.**
