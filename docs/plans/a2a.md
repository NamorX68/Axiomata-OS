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

## Offen zur Entscheidung

- **A5 — Welcher Agent, welches Harness, welches Modell? (Vorschlag, noch nicht bestätigt):** ein *Profilkatalog* (Rolle, Harness, Modell, Kostenstufe,
  Rechte) und ein *Zuweiser*, der für eine Karte ein Profil vorschlägt: erst feste Regeln (Rolle „Review" → Review-Profil), dann ein Klassifikator —
  **JEV** (TypeSafe, typisierte Fragen mit kalibrierter Sicherheit: `Choice` über die Profile, `Score` für die Schwierigkeit; siehe `jev-einsatz.md`),
  mit einem lokalen Fallback (kleines Ollama-Modell mit erzwungener Auswahl; Arbeitsname KEV) und zuletzt der Frage an den Owner. Im Freigabe-Schritt
  sieht der Owner je Karte das vorgeschlagene Profil samt Sicherheit und kann es ändern; der Automatikmodus nimmt nur Vorschläge über der Schwelle.

## Offene Fragen der Runde

1. Rollen und Rechte: Wer darf mit wem reden, wer teilt Arbeit zu?
2. Aufgabenverteilung: direkt per Nachricht oder immer über das Kanban-Brett?
3. Bestätigung durch den Owner: was läuft von allein, was braucht ein Ja?
4. Zustellung an laufende TUI-Agenten: abfragen oder hineintippen; Schutz vor Endlosschleifen.
5. Grenzen und Kosten: Limits pro Aufgabe.
6. MCP-Eintrag pro Harness (F4 im Plan `agentic-ide.md`).
7. Eigenständiges Studio: Wo liegt das Brett, wenn das Studio eine eigene App wird?
