# Axiomata im Überblick

Was Axiomata ist, wie seine Bereiche zusammenhängen und welche Handbuchseite welche Frage beantwortet.

## Was es ist

Axiomata-OS ist die Kommandozentrale und das zweite Gehirn des Owners: eine Mac-App, die den eigenen Vault aus
Markdown-Notizen, die Agenten, die darin und in Code-Projekten arbeiten, und die täglichen Übersichten an einem Ort
zusammenbringt. Die Bereiche:

- **Orbit** — der Startbildschirm: der Vault als Wolke im Hintergrund, der App-Ring, die Kacheln und das Dock.
- **Second Brain** — der Vault als Bild (Bundle, Map, Hex), mit Suche, Tags und Links; Notizen legt beim Speichern
  ein Agent an den passenden Platz.
- **Studio** — die Werkbank für Code: Editor, Agenten-Panes mit eigenem Worktree, Git, Run, Debug und der Flow, in
  dem ein Planer ein Ziel in Karten zerlegt, die ein Team von Agenten abarbeitet.
- **Kanban** — das Board für alle Aufgaben, eigene wie die der Agenten.
- **Skills und Routinen** — Arbeitsanweisungen für Agenten, von Hand oder nach Zeitplan gestartet; sie füttern auch
  die Kacheln für Kalender, Mail und Erinnerungen.
- **Glut** — Axiomatas eigener Harness, neben Claude Code; er führt Chat, Skills und Karten auf Modellen von
  OpenRouter oder Ollama.
- **Der Chat** unten in der Leiste und **⌘K** erreichen alles davon.

## Wie die Seiten zusammenhängen

Die App-Daten liegen in `~/.axiomata` (Einstellungen, Datenbank, Skills, Protokolle), getrennt vom Vault, den du
unter Settings → Workspace wählst. Jeder Agenten-Lauf — Chat, Skill, Routine, Kachel-Refresh, Karte — läuft auf
einem **Harness** und einem **Provider** ([Provider](provider.md)) und zählt gegen die **Grenzen**
([Kosten](kosten.md)). Eine Aufgabe wandert typisch so: Karte im [Kanban](kanban.md) → „Start …“ → Plan im
[Flow](flow.md) → Agenten im [Studio](studio.md) → „Take over plan“. Welche Rolle auf welchem Modell arbeitet, steht
unter [Modelle und Rollen](modelle-rollen.md). Was du siehst, stellst du unter
[Einstellungen](einstellungen.md) ein; aufgeräumt wird unter [Storage](storage.md).

Der Chat antwortet aus diesen Seiten. Steht eine Antwort hier nicht, sagt er das — dann gehört sie ins Handbuch.

Selbst lesen: `?` im Dock öffnet das Handbuch als Fenster (links die Seiten, rechts die Leseansicht), ⌘K findet
Seiten in der Gruppe „Handbook“, und `/help <thema>` im Chat öffnet die passende Seite.

## Alle Seiten

- [Orbit, App-Ring, Kacheln und Dock](orbit.md) — Wie der Startbildschirm aufgebaut ist: der Orbit im Hintergrund, der App-Ring um ihn herum, die Kacheln auf der Fläche und das Dock mit seinen Knöpfen.
- [Second Brain](second-brain.md) — Wie man den Vault als Bild ansieht: die Ansichten Bundle, Map und Hex, das Filterfeld, „How your vault grew“, die Zeitreise und das Detailfeld einer Notiz.
- [Notizen anlegen, ablegen und die Inbox](notizen.md) — Wie man eine neue Notiz schreibt, wie der Agent sie beim Speichern in den passenden Bereich des Vaults ablegt und was mit der Inbox geschieht.
- [Datei-App und Editor](editor.md) — Wie man Dateien öffnet, liest und bearbeitet: das Dateifenster, der Editor im Studio, Leseansicht, Vi-Modus, Suche, Sprachserver (LSP) und Sprungverlauf.
- [Tags und Wikilinks](tags-links.md) — Wie man Notizen mit `#tags` und `[[Wikilinks]]` verknüpft, wie die Tag-Liste in TAGS.md gepflegt wird und wie der einmalige Vault-Lauf Tags und Links für alle Notizen vorschlägt.
- [Studio](studio.md) — Wie die Werkbank für Code-Projekte aufgebaut ist: Projekte, die Modi Editor, Canvas und Flow, Panes, Agenten mit eigenem Worktree, Git, Run/Tasks und Debug.
- [Flow: Pläne, Agententeam und Grenzen](flow.md) — Wie man im Studio-Modus „Flow“ einen Plan anlegt, vom Planer in Karten zerlegen lässt, freigibt, selbst laufen lässt und am Ende übernimmt — mit Grenzen, Kostenstand, Fragen der Agenten und Eskalation.
- [Modelle, Stufen und Rollen](modelle-rollen.md) — Wie das Studio Modelle einschaltet und in die Stufen Light, Medium und Heavy einordnet, welche Engine eine Stufe startet, was eine Rolle ist und was die einmalige Umstellung auf Rollen mit Stufen gemacht hat.
- [Kanban](kanban.md) — Wie das Kanban-Board funktioniert: Boards, Spalten mit den festen Spalten Proposal und Review, Kartenfelder, die Knöpfe einer Karte und was Agenten auf dem Board dürfen.
- [Der Chat in der Leiste](chat.md) — Wie man mit dem Agenten unten in der Leiste spricht, welche Slash-Befehle es gibt, was Plan und Build bedeuten und was der Chat in der App tun darf.
- [Spotlight (⌘K)](spotlight.md) — Wie man mit ⌘K alles in Axiomata findet und startet: Befehle, Kacheln, Skills, Routinen, Karten, Pläne, Projekte, Tags und Dateien — und wie man von dort den Chat fragt.
- [Skills und das Skills Deck](skills.md) — Was ein Skill ist, wo er liegt, wie man ihn startet und wie das Skills Deck die mitgelieferten Skills aktuell hält.
- [Routinen und ihr Zeitplan](routinen.md) — Wie man einen Skill oder einen Prompt nach Zeitplan laufen lässt, wie das Intervall eingestellt wird und was passiert, wenn die App geschlossen ist.
- [Kalender, Mail und Erinnerungen](verbindungen.md) — Wie die Kacheln „Calendar“, „Mail“ und „Reminders“ an Apples Programme angebunden sind, wann sie sich auffrischen und was das kostet.
- [Terminal](terminal.md) — Wie man die Terminal-Kachel öffnet und einstellt (Shell, Schrift, Farben) und welche Tasten dort anders sind.
- [Glut in Axiomata](glut.md) — Was Glut ist und wo es in Axiomata arbeitet: als Harness für Chat und Skills, als Karten-Lauf im Studio-Pane, als Glut-Master im Projekt — und wie man seine Fragen beantwortet.
- [Kosten, Preise und Tagesgrenzen](kosten.md) — Was Agenten-Läufe kosten, woher die Preise kommen und welche Grenzen Chat, Skills und das Studio stoppen.
- [Einstellungen und Darstellung](einstellungen.md) — Was in den Einstellungen steht und wie man das Aussehen anpasst: Theme, Schrift, Bewegung, Glas mit Hintergrundbild, UI-Größe und Tastatur.
- [KI-Provider und Harness je Rolle](provider.md) — Welcher Harness (Claude Code oder Glut) und welcher Provider (Anthropic, OpenRouter, Ollama) den Chat und die Skills bedienen, wie man Modelle wählt, Claude Code anmeldet und Gluts Websuche einrichtet.
- [Storage und Wartung](storage.md) — Wie Axiomata seine eigenen Daten in `~/.axiomata` aufräumt, was wie lange bleibt und wie man „Clean up now“ und die verwaisten Worktrees bedient.
