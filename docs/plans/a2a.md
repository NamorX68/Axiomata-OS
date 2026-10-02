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

## Offene Fragen der Runde

1. Rollen und Rechte: Wer darf mit wem reden, wer teilt Arbeit zu?
2. Aufgabenverteilung: direkt per Nachricht oder immer über das Kanban-Brett?
3. Bestätigung durch den Owner: was läuft von allein, was braucht ein Ja?
4. Zustellung an laufende TUI-Agenten: abfragen oder hineintippen; Schutz vor Endlosschleifen.
5. Grenzen und Kosten: Limits pro Aufgabe.
6. MCP-Eintrag pro Harness (F4 im Plan `agentic-ide.md`).
7. Eigenständiges Studio: Wo liegt das Brett, wenn das Studio eine eigene App wird?
