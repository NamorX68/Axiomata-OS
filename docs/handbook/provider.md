# KI-Provider und Harness je Rolle

Welcher Harness (Claude Code oder Glut) und welcher Provider (Anthropic, OpenRouter, Ollama) den Chat und die Skills bedienen, wie man Modelle wählt, Claude Code anmeldet und Gluts Websuche einrichtet.

## Was es ist

- Der **Harness** führt den Agenten — seine Werkzeuge, Rechte und Schleife: **Claude Code** (nur Anthropic, mit
  einer eigenen Anmeldung der App, getrennt von deiner in `~/.claude`) oder **Glut**, Axiomatas eigener Harness
  (OpenRouter oder Ollama).
- Der **Provider** liefert das Modell: **Anthropic** (über das Abo, ohne Schlüssel), **OpenRouter** (mit API-Key)
  oder **Ollama** (lokal auf diesem Mac).
- Es gibt zwei **Rollen**: den Chat und „skills & routines“. Jede Rolle wählt Harness und Provider für sich — etwa
  Claude Code für den Chat und Glut mit Ollama für die Digests.
- Ein Skill kann mit `backend:` und `model:` im eigenen Kopf davon abweichen. Die Agenten des Studios wählen ihr
  Modell dagegen über **Engines** (siehe [Studio](studio.md)).

## Wo die Knöpfe sind

**Settings → AI providers:**

- **„Edit providers“:** die drei Provider (Abzeichen „Chat“ / „Skills“ zeigen, wer gerade dient). Ein Klick öffnet
  das Formular: „Base URL“, „API key“ (gespeichert als „•••••• saved — leave empty to keep“), „Chat model“,
  „Skills/routines model“ (Auswahl aus dem Katalog, ↻ frischt ihn auf) → „Save provider“.
- **„Harness & provider assignment“:** je Rolle „Harness for …“, „Provider for …“ und bei Glut „Effort for …“
  (leer = „Glut's own setting“).
- **„Claude Code sign-in“** (wenn eine Rolle Claude Code nutzt): „Sign in with Anthropic“, ggf. „Code (only if the
  browser page shows one)“ → „Send“; angemeldet: „Sign out“.
- **„Web search (Glut)“:** Schlüssel für Exa, TinyFish und Tavily, „Searches per day (all providers)“, „Tavily
  searches per month“.
- Die Spend-Zeilen und „Daily limit (USD, paid providers)“ (siehe [Kosten](kosten.md)), dann „Save assignment“.

## Häufige Handgriffe

1. **Chat auf ein anderes Modell:** „Edit providers“ → Provider des Chats → „Chat model“ → „Save provider“. Nur für
   ein Gespräch: `/model <id>` im Chat.
2. **Digests lokal laufen lassen:** „Harness for skills & routines“ = Glut, „Provider for skills & routines“ =
   Ollama → „Save assignment“; bei Ollama das Skills-Modell setzen.
3. **Claude Code anmelden:** „Sign in with Anthropic“ → im Browser bestätigen → ggf. den Code eintragen → „Send“.

## Fallen

- Claude Code kann nur Anthropic, Glut nur OpenRouter und Ollama — die Auswahl der Provider folgt dem Harness.
- Ohne Anmeldung scheitern Chat und Skills auf Claude Code; beim Start erscheint dann ein Hinweis.
- Ein API-Key wird nach dem Speichern nie wieder angezeigt. Ein geleertes Schlüsselfeld löscht den Schlüssel beim
  Speichern („The key is deleted on save.“).
- Ein `model:` im Kopf eines Skills schlägt das Skills-Modell des Providers.
- „Searches per day“ oder „Tavily searches per month“ auf 0 schaltet die Suche bzw. Tavily ab; Glut fragt Exa, dann
  TinyFish, dann Tavily und überspringt einen Anbieter ohne Schlüssel.
- „Save provider“ und „Save assignment“ speichern beide **alles** auf diesem Reiter; es gilt ab dem nächsten
  Agenten-Aufruf. Ungespeichertes gilt nicht.

## Siehe auch

[Kosten](kosten.md) · [Glut](glut.md) · [Chat](chat.md) · [Skills](skills.md) · [Einstellungen](einstellungen.md)
