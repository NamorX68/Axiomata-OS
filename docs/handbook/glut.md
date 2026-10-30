# Glut in Axiomata

Was Glut ist und wo es in Axiomata arbeitet: als Harness für Chat und Skills, als Karten-Lauf im Studio-Pane, als Glut-Master im Projekt — und wie man seine Fragen beantwortet.

## Was es ist

**Glut** ist Axiomatas eigener Harness: das Programm, das einen Agenten mit Werkzeugen, Rechten und Schleife führt —
an Modellen von OpenRouter oder Ollama. Es ist eine eigene Anwendung (`glut`) mit eigener Ablage `~/.glut`
(Einstellungen, MCP-Server in `~/.glut/mcp.json`, Sitzungen); Axiomata gibt ihm nur Werte obendrauf mit. In
Axiomata arbeitet Glut an drei Stellen:

- **Chat und Skills:** wenn unter Settings → AI providers „Glut“ als Harness einer Rolle gewählt ist. Dann gibt es
  den „Effort“-Schalter und Gluts Websuche. Die Digests erreichen Mail, Kalender und Erinnerungen über Gluts
  MCP-Server.
- **Karten im Studio:** eine Engine der Art Glut führt eine Karte in ihrem Pane mit Gluts eigener Oberfläche (TUI).
  Das Modell steht fest (kein `/model` im Pane), die Rechte kommen aus der Rolle, das Restbudget der Sitzung geht
  als Grenze des Laufs mit. Ein vom Reviewer zurückgegebener Auftrag läuft im selben Gespräch weiter.
- **Glut-Master:** die erste Master-Kachel im Agents-Bereich des Studios — dein eigenes Glut im echten Projektordner,
  mit den Team-Werkzeugen, ohne Karte und ohne Worktree.

## Wo die Knöpfe sind

- **Harness wählen:** Settings → AI providers → „Harness for chat“ / „Harness for skills & routines“ → „Glut“.
- **Engine für Karten:** jedes eingeschaltete OpenRouter- oder Ollama-Modell ist von selbst eine Glut-Engine (Studio →
  Agents → „Models & roles…“ → „Models“, [Modelle und Rollen](modelle-rollen.md)). Eine eigene: dort unter „Own
  engines“ → „Add engine…“ → Harness Glut; im Feld „Model“ wählt `openrouter/…` oder `ollama/…` den Provider.
- **Master öffnen:** Studio → Agents → oben die Master-Kachel „Glut“.
- **Zuschauen:** auf einer Live-Kachel im Flow „Watch“ — folgt der Sitzung nur lesend in einem eigenen Pane.
- **Fragen beantworten:** im Dialog im Pane **oder** in der App („Allow“ / „Deny“ / „In terminal“); die erste
  Antwort gilt.
- **Platz:** Settings → Storage zeigt Gluts Bedarf; Glut räumt `~/.glut` selbst.

## Häufige Handgriffe

1. **Digests auf Glut + Ollama:** Settings → AI providers → Skills: Harness „Glut“, Provider „Ollama“ → „Save
   assignment“.
2. **Eine Glut-Frage beantworten:** dem Abzeichen „… is waiting for you“ folgen → „Allow“ oder „Deny“ — oder im
   Pane antworten.
3. **Einen abgebrochenen Lauf fortsetzen:** im Skills Deck „resume“ (oder „drop“); im Chat „Resume“; ein
   Karten-Pane bietet beim Wiederöffnen die Fortsetzung an.

## Fallen

- Glut bedient **kein Anthropic**; ein Skill mit `backend: axh` braucht einen Skill-Provider mit Endpoint
  (OpenRouter oder Ollama), sonst scheitert er mit klarer Meldung statt stillem Ausweichen.
- Gluts Sandbox verbirgt `~/.axiomata` vor jedem Befehl; nur einige freigegebene `axiomata-cli`-Befehle laufen an ihr
  vorbei. Ein Skill bekommt nur die Rechte aus seinem `allowed_tools`; `deny`/`ask` aus `~/.glut` gelten trotzdem.
- Ein neuer Build der App nimmt Glut aus seinem eigenen Repository mit; eine Glut-Einstellung gehört nach
  `~/.glut/settings.json` bzw. in Gluts eigene Befehle, nicht in Axiomata.
- Wartet eine Glut-Karte auf eine Antwort und niemand sitzt am Pane, wartet sie, bis du in der App antwortest.

## Siehe auch

[Provider](provider.md) · [Studio](studio.md) · [Flow](flow.md) · [Skills](skills.md) · [Storage](storage.md)
