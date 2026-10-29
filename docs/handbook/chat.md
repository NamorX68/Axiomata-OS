# Der Chat in der Leiste

Wie man mit dem Agenten unten in der Leiste spricht, welche Slash-Befehle es gibt, was Plan und Build bedeuten und was der Chat in der App tun darf.

## Was es ist

Unten auf dem Bildschirm liegt die Eingabe „Ask the agent, or /help for commands“. Was du dort schreibst, geht an den
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
- ↑/↓ ohne offene Liste blättern durch die letzten 50 Eingaben.

## Häufige Handgriffe

1. **Nur planen lassen:** „Plan“ einschalten (⇧⇥) — der Agent liest und plant, ändert nichts; „Build“ lässt ihn
   ändern.
2. **Neu anfangen:** „New session“ oder `/clear`; ein früheres Gespräch holt `/resume` zurück.
3. **Anderes Modell für dieses Gespräch:** `/model <id>`; `/model default` kehrt zur Einstellung zurück.

## Was er darf

- Er nutzt die Aktionen der **offenen** Kacheln (Routinen, Kalender, Erinnerungen, ToDo …) und die Kanban-Aktionen
  (Karten auflisten, anlegen, verschieben — nur einfache Karten, nie die Schritte, die dir gehören: Freigeben,
  Review, Übernehmen). Er kann dir eine Datei im Dateifenster öffnen.
- Vor dem Verschieben, Umbenennen oder Löschen von Dateien und vor vielen Änderungen auf einmal sagt er, was er vorhat,
  und wartet auf dein Ja.
- Im Plan-Modus ändert er nichts.

## Fallen

- Ein unbekannter `/befehl` wird **nicht** gesendet, nur gemeldet („unknown command …“). Für einen freien Auftrag:
  `/do <text>`.
- `/clear`, `/compact`, `/resume` und `/undo` gehen nicht, solange der Agent arbeitet.
- Jeder Zug kostet bei einem bezahlten Provider Geld und zählt gegen die Tagesgrenze ([Kosten](kosten.md)).
- Endet die App mitten in einem Zug, bietet das Chatfenster beim nächsten Start „Resume“ oder „Drop“ an.
- Aktionen einer Kachel kennt er nur, wenn die Kachel auf der Fläche liegt; Kanban geht immer.

## Siehe auch

[Spotlight](spotlight.md) · [Provider](provider.md) · [Kanban](kanban.md) · [Skills](skills.md)
