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

- **A5 — Welcher Agent, welches Harness, welches Modell (Owner, 2026-10-03).** Ein **Profilkatalog** (Rolle, Harness, Modell, Kostenstufe, Rechte)
  und ein **Zuweiser als austauschbare Schnittstelle**. **Standard ist der Planungsagent selbst:** er kennt den Katalog, schlägt beim Anlegen einer
  Karte ein Profil vor und begründet kurz; der Owner bestätigt beim Freigeben des Plans. Gründe (Owner): ein Agent ist bei jedem Nutzer ohnehin
  da, JEV dagegen verlangt Konto, Schlüssel und Guthaben. Der **Automatikmodus (A4)** startet nur Karten, deren Profil der Owner bestätigt hat
  oder die eine feste Regel bestimmt (z. B. Rolle „Review"). **Optional** kommt ein Zuweiser mit kalibrierter Sicherheit dazu — JEV (TypeSafe,
  `jev-einsatz.md`, Version pinnen) oder ein lokales offenes Modell, sofern die Hardware reicht (Laya: ModernBERT-large; Nimble: Qwen3.5-9B + LoRA;
  Kev u. a. — alle in einem DataCamp-Vergleich offener JEV-Alternativen genannt) — als Einstellung, kein Muss. Das Review-Modell ist ein anderes
  als das der Umsetzung.
  *Offen:* Kev und die übrigen offenen Modelle sind noch nicht geprüft (Größe, Lizenz, Laufzeit); Katalog-Ort (App-Einstellungen, pro Projekt, oder
  beides mit Überschreiben) und die Anfangsprofile.

## Offene Fragen der Runde

1. Rollen und Rechte: Wer darf mit wem reden, wer teilt Arbeit zu?
2. Aufgabenverteilung: direkt per Nachricht oder immer über das Kanban-Brett?
3. Bestätigung durch den Owner: was läuft von allein, was braucht ein Ja?
4. Zustellung an laufende TUI-Agenten: abfragen oder hineintippen; Schutz vor Endlosschleifen.
5. Grenzen und Kosten: Limits pro Aufgabe.
6. MCP-Eintrag pro Harness (F4 im Plan `agentic-ide.md`).
7. Eigenständiges Studio: Wo liegt das Brett, wenn das Studio eine eigene App wird?
