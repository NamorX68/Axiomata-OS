# Skills und das Skills Deck

Was ein Skill ist, wo er liegt, wie man ihn startet und wie das Skills Deck die mitgelieferten Skills aktuell hält.

## Was es ist

Ein **Skill** ist eine Arbeitsanweisung für einen Agenten: eine Datei `~/.axiomata/skills/<name>/SKILL.md` mit
Kopfzeilen (Frontmatter) und dem Text, mit dem der Agent arbeitet. Es gibt nur diesen einen Ort — keine Skills im
Vault. Jeder Lauf ist ein echter Agenten-Zug und kann Geld kosten (siehe [Kosten](kosten.md)).

Die Kopfzeilen: `name` und `description` (Pflicht); wahlweise `backend:` (`claude_code`, `axh` = Glut, `ollama` =
ohne Werkzeuge; fehlt es, gilt der Harness für Skills aus den Einstellungen), `model:` (schlägt das Skills-Modell
des Providers), `allowed_tools` (die **ganzen** Rechte des Laufs), `prepend_files:` (Vault-Dateien, die als Kontext
vorne angehängt werden), `timeout_secs`, `output: json` (für Kacheln, die die Antwort auslesen), `access: read`.

**Mitgelieferte Skills:** `calendar-digest`, `mail-digest`, `reminders-digest` (füttern die Kacheln, siehe
[Verbindungen](verbindungen.md)), `cleanup` (räumt alte Mail-Notizen, erledigte ToDos und alte Cleanup-Berichte
nach 3 Tagen weg), `inbox-sort` (sortiert die Inbox in die Bereiche, siehe [Notizen](notizen.md)), `todo-to-kanban`
(macht aus offenen ToDos Kanban-Karten), `news-rss`, `news-web`, `model-news` (KI-Nachrichten und neue Modelle in
den Ordner `News/`) und `vault-tagging` (der Vault-Lauf, siehe [Tags und Links](tags-links.md)).

## Wo die Knöpfe sind

- **Vorderseite der Kachel „Skills Deck“:** je Skill eine Karte mit `/name`, dem Backend und ▶ („Ausführen“). Daneben
  der Status des letzten Laufs; ein unterbrochener Lauf zeigt „fortsetzen“ und „verwerfen“. Ein kaputter Skill erscheint
  als „⚠ übersprungen …“. Gibt es neue Fassungen mitgelieferter Skills, steht oben „Für … mitgelieferte Skills gibt es
  Updates“ mit „Ansehen“.
- **Rückseite (Zahnrad):** „Backend“ (Filter), „Reihenfolge“ („nach Name“ / „zuletzt ausgeführte zuerst“) und der
  Abschnitt **„Mitgelieferte Skills“**: je Skill der Stand („aktuell“, „veraltet“, „fehlt“, „von dir geändert“, „weicht
  ab — vielleicht deine“) mit „Aktualisieren“, oben „Alle veralteten aktualisieren“.
- Anderswo: ⌘K findet Skills (Enter startet), `/skills` bzw. `/skills run <name>` im Chat.

## Häufige Handgriffe

1. **Skill starten:** im Skills Deck ▶ — oder ⌘K, Namen tippen, Enter.
2. **Mitgelieferte Skills aktualisieren:** „Ansehen“ (oder Rückseite) → „Aktualisieren“ beim Skill bzw. „Alle veralteten
   aktualisieren“. Bei „von dir geändert“ fragt die App: „Ersetzen“ oder „Meine behalten“.
3. **Eigenen Skill anlegen:** den Chat bitten, ihn zu schreiben, oder selbst einen Ordner
   `~/.axiomata/skills/<name>/` mit `SKILL.md` anlegen; das Deck liest die Skills alle paar Sekunden neu.

## Fallen

- Das Deck zeigt **keine Ausgabe** eines Laufs, nur Status und Zeit; das Ergebnis steht dort, wohin der Skill
  schreibt (Kachel, Vault-Notiz).
- Mitgelieferte Skills werden nur **einmal** angelegt; eine neuere Fassung im Build überschreibt deine Kopie nicht.
  „Alle veralteten aktualisieren“ fasst eine von dir geänderte Kopie nie an; im Terminal überschreibt
  `axiomata-cli skills reseed --force` alle mitgelieferten (deine eigenen Skills bleiben).
- `allowed_tools` sind die ganzen Rechte: fehlt ein Werkzeug dort, darf der Lauf es nicht.
- Ein Skill mit `backend: axh` (Glut) braucht einen Skill-Provider mit Endpoint (OpenRouter oder Ollama), sonst
  scheitert er mit klarer Meldung.

## Siehe auch

[Routinen](routinen.md) · [Verbindungen](verbindungen.md) · [Provider](provider.md) · [Chat](chat.md)
