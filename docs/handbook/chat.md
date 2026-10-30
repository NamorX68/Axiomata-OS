# Der Chat in der Leiste

Wie man mit dem Agenten unten in der Leiste spricht, welche Slash-Befehle es gibt, was Plan und Build bedeuten und was der Chat in der App tun darf.

## Was es ist

Unten auf dem Bildschirm liegt die Eingabe „Frag den Agenten, oder /help für Kommandos“ (auf Englisch „Ask the agent,
or /help for commands“). Was du dort schreibst, geht an den
Chat-Agenten — auf dem Harness und Provider, die unter Settings → AI providers für den Chat gewählt sind. Er arbeitet
im Vault, kennt die Aktionen der offenen Kacheln und die Kanban-Aktionen und antwortet aus diesem Handbuch; steht
etwas nicht darin, sagt er das. Seine Antworten erscheinen im Fenster „Agent“ mit den benutzten Aktionen und den
Kosten des Zugs.

## Wo die Knöpfe sind

- **Die Leiste:** das Eingabefeld; der Knopf „Plan“ / „Build“ (auch ⇧⇥ im Feld); bei Glut der Schalter „Effort“
  (nur für dieses Gespräch); „New session“ (⌘N im Feld; Windows/Linux Ctrl+Shift+N); rechts „chat“ / „hide chat“
  mit der Zahl der Nachrichten, während der Arbeit „working…“.
- **Slash-Befehle** (`/` öffnet die Liste; ↑/↓, Tab übernimmt, Esc schließt):
  - Gespräch: `/help`, `/clear` (neues Gespräch), `/compact`, `/model [modell]` (`default` = zurück), `/effort
    [stufe]` (nur Glut), `/plan`, `/build`, `/skills [run <name>]`, `/cost`, `/resume [sitzung]`, `/undo` (nimmt die
    Dateiänderungen des letzten Zugs zurück), `/do <text>` (ein Einmal-Auftrag außerhalb des Gesprächs).
  - App: `/tile add|remove <typ>` bzw. `/tile <typ> <aktion> [json]`, `/theme <id>`, `/open <pfad>`, `/newfile
    <pfad>`, `/brain [pfad | ? suche]`.
  - Dazu die eigenen Befehle des Harness, unter ihrem Namen oder mit `/glut:` bzw. `/cc:` davor.
- **Das Panel über der Leiste:** Sobald `/model`, `/skills`, `/resume`, `/help` oder `/effort` ihr Leerzeichen
  haben, öffnet sich die Liste darüber („Modelle“, „Skills“, „Frühere Gespräche“, „Kommandos und Handbuch“,
  „Effort“); was du weitertippst, filtert sie (`/model deep`). ↑/↓ wählt, ⏎ übernimmt, Esc oder ein Klick daneben
  schließt. Ein Skill fragt erst „‹name› starten? ⏎ ja · Esc nein“. Passt bei `/model` kein Eintrag, nimmt
  „‹id› verwenden“ die getippte ID. `/cost`, `/undo` und die Ausgabe einer Kachel-Aktion erscheinen im selben Panel —
  kein Kommando schreibt mehr in den Gesprächsverlauf.
- ↑/↓ ohne offene Liste blättern durch die letzten 50 Eingaben.

## Häufige Handgriffe

1. **Nur planen lassen:** „Plan“ einschalten (⇧⇥) — der Agent liest und plant, ändert nichts; „Build“ lässt ihn
   ändern.
2. **Neu anfangen:** „New session“ oder `/clear`; ein früheres Gespräch holt `/resume` zurück.
3. **Anderes Modell für dieses Gespräch:** `/model <id>`; `/model default` kehrt zur Einstellung zurück.

## Was er darf

- **Nachlesen und Lage kennen:** Er liest dieses Handbuch und die Lage der App — laufende Agenten und wartende
  Fragen im Studio, Kosten, letzte Läufe, Routinen, wartende Karten, Storage, Einstellungen (Schlüssel nie, nur „set“ /
  „not set“) und die Tag-Liste. Er antwortet nur daraus; steht etwas nicht darin, sagt er das.
- **Sofort:** Ansichten öffnen (Studio, ein Projekt, Kanban, eine Karte, das Second Brain mit `#tag`-Filter, eine
  Handbuchseite, einen Reiter der Einstellungen, eine Datei, eine neue Terminal-Kachel) und die Darstellung umstellen (Theme, Layout des Second
  Brain, Glas-Blur und -Abdunklung, Bewegung); einfache Karten anlegen und verschieben, ToDo, die lesenden Aktionen der
  Kacheln. Sagen, was offen ist, und schließen, was dabei nichts verliert: ein Kanban- oder Datei-Panel, eine
  Terminal-Kachel mit wartender Shell, das Second Brain, das Handbuch, die Einstellungen.
- **Nur nach deinem Ja:** einen Skill starten, eine Routine anlegen oder ändern, eine Karte starten, einen Digest
  auffrischen, in Kalender oder Erinnerungen schreiben; etwas schließen, das dabei etwas verlöre — eine Datei mit
  ungespeichertem Text (danach fragt das Panel selbst: speichern oder verwerfen), ein getippter, noch nicht
  abgeschickter Kartentitel oder Verlaufseintrag, ein offenes Formular einer Karte, ein Terminal, in dem ein Programm
  läuft, auch im Hintergrund (es endet mit der Kachel). Die Frage erscheint als Leiste in der App mit allem, was er tun
  will; „Allow“ führt genau das aus, „Deny“ nichts. Eine Frage verfällt nach 2 Minuten.
- **Nie:** etwas löschen, Schlüssel oder Provider ändern, Agenten im Studio steuern oder seine Panes schließen, deine
  Schritte auf dem Board (Freigeben, Review, Übernehmen, Integrieren).
- Im Plan-Modus ändert er nichts.

## Fallen

- Ein unbekannter `/befehl` wird **nicht** gesendet, nur gemeldet („unknown command …“). Für einen freien Auftrag:
  `/do <text>`.
- `/clear`, `/compact`, `/resume` und `/undo` gehen nicht, solange der Agent arbeitet.
- Jeder Zug kostet bei einem bezahlten Provider Geld und zählt gegen die Tagesgrenze ([Kosten](kosten.md)).
- Endet die App mitten in einem Zug, bietet das Chatfenster beim nächsten Start „Resume“ oder „Drop“ an.
- Aktionen einer Kachel kennt er nur, wenn die Kachel auf der Fläche liegt; Ansichten, Darstellung und Kanban gehen
  immer.
- Was er nicht weiß, liest du selbst im Handbuch nach: `?` im Dock, ⌘K (Gruppe „Handbook“) oder `/help <thema>`.

## Siehe auch

[Spotlight](spotlight.md) · [Provider](provider.md) · [Kanban](kanban.md) · [Skills](skills.md)
