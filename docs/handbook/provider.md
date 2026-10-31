# KI-Provider und Harness je Rolle

Welcher Harness (Claude Code oder Glut) und welcher Provider (Anthropic, OpenRouter, Ollama) den Chat und die Skills bedienen, wie man Modelle wählt, Claude Code anmeldet und Gluts Websuche einrichtet.

## Was es ist

- Der **Harness** führt den Agenten — seine Werkzeuge, Rechte und Schleife: **Claude Code** (nur Anthropic, mit
  einer eigenen Anmeldung der App, getrennt von deiner in `~/.claude`) oder **Glut**, Axiomatas eigener Harness
  (OpenRouter oder Ollama).
- Der **Provider** liefert das Modell: **Anthropic** (über das Abo, ohne Schlüssel), **OpenRouter** (mit API-Key)
  oder **Ollama** (lokal auf diesem Mac).
- Es gibt zwei **Rollen**: den Chat und „Skills & Routines“. Jede Rolle wählt Harness und Provider für sich — etwa
  Claude Code für den Chat und Glut mit Ollama für die Digests.
- Ein Skill kann mit `backend:` und `model:` im eigenen Kopf davon abweichen. Die Agenten des Studios wählen ihr
  Modell dagegen über ihre **Stufe** und deren Engine (siehe [Modelle und Rollen](modelle-rollen.md)).

## Wo die Knöpfe sind

**Einstellungen → KI-Anbieter:**

- **„Anbieter bearbeiten“:** die drei Provider (Abzeichen „Chat“ / „Skills“ zeigen, wer gerade dient). Ein Klick öffnet
  das Formular: „Basis-URL“, „API-Schlüssel“ (gespeichert als „•••••• gespeichert — leer lassen, um ihn zu behalten“),
  „Chat-Modell“, „Modell für Skills/Routines“ (Auswahl aus dem Katalog, ↻ frischt ihn auf) → „Anbieter speichern“.
- **„Harness & Anbieter zuordnen“:** je Rolle „Harness für …“, „Anbieter für …“ und bei Glut „Effort für …“
  (leer = „Einstellung von Glut“).
- **„Claude-Code-Anmeldung“** (wenn eine Rolle Claude Code nutzt): „Mit Anthropic anmelden“, ggf. „Code (nur, wenn die
  Browserseite einen zeigt)“ → „Senden“; angemeldet: „Abmelden“.
- **„Websuche (Glut)“:** Schlüssel für Exa, TinyFish und Tavily, „Suchen pro Tag (alle Anbieter)“, „Tavily-Suchen pro
  Monat“.
- Die „Ausgaben“-Zeilen und „Tagesgrenze (USD, bezahlte Anbieter)“ (siehe [Kosten](kosten.md)), dann „Zuordnung
  speichern“.

## Häufige Handgriffe

1. **Chat auf ein anderes Modell:** „Anbieter bearbeiten“ → Provider des Chats → „Chat-Modell“ → „Anbieter speichern“.
   Nur für ein Gespräch: `/model <id>` im Chat.
2. **Digests lokal laufen lassen:** „Harness für Skills & Routines“ = Glut, „Anbieter für Skills & Routines“ =
   Ollama → „Zuordnung speichern“; bei Ollama das Skills-Modell setzen.
3. **Claude Code anmelden:** „Mit Anthropic anmelden“ → im Browser bestätigen → ggf. den Code eintragen → „Senden“.

## Fallen

- Claude Code kann nur Anthropic, Glut nur OpenRouter und Ollama — die Auswahl der Provider folgt dem Harness.
- Ohne Anmeldung scheitern Chat und Skills auf Claude Code; beim Start erscheint dann ein Hinweis.
- Ein API-Key wird nach dem Speichern nie wieder angezeigt. Ein geleertes Schlüsselfeld löscht den Schlüssel beim
  Speichern („Der Schlüssel wird beim Speichern gelöscht.“).
- Ein `model:` im Kopf eines Skills schlägt das Skills-Modell des Providers.
- „Suchen pro Tag“ oder „Tavily-Suchen pro Monat“ auf 0 schaltet die Suche bzw. Tavily ab; Glut fragt Exa, dann
  TinyFish, dann Tavily und überspringt einen Anbieter ohne Schlüssel.
- „Anbieter speichern“ und „Zuordnung speichern“ speichern beide **alles** auf diesem Reiter; es gilt ab dem nächsten
  Agenten-Aufruf. Ungespeichertes gilt nicht.

## Siehe auch

[Kosten](kosten.md) · [Glut](glut.md) · [Chat](chat.md) · [Skills](skills.md) · [Einstellungen](einstellungen.md)
