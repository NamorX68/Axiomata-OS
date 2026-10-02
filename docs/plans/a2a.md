# Plan: Agent-zu-Agent-Kommunikation (M7.5)

Status: **in Planung** (Fragenrunde mit dem Owner, begonnen 2026-10-03). Es entsteht kein Code, bevor die Runde bestätigt ist.
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

## Offene Fragen der Runde

1. ~~Rollen und Rechte / Aufgabenverteilung~~ — beantwortet durch A2–A5.
2. ~~Bestätigung durch den Owner~~ — beantwortet durch A7.
4. Zustellung an laufende TUI-Agenten: abfragen oder hineintippen; Schutz vor Endlosschleifen.
5. Grenzen und Kosten: Limits pro Aufgabe.
6. MCP-Eintrag pro Harness (F4 im Plan `agentic-ide.md`).
7. Eigenständiges Studio: Wo liegt das Brett, wenn das Studio eine eigene App wird?
