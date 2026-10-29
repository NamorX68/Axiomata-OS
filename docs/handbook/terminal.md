# Terminal

Wie man die Terminal-Kachel öffnet und einstellt (Shell, Schrift, Farben) und welche Tasten dort anders sind.

## Was es ist

Die Kachel „Terminal“ ist eine echte Shell in Axiomata. Jede Kachel hat ihre eigene Shell; man kann beliebig viele
öffnen. Dieselbe Terminal-Technik steckt in den Panes des Studios (Agenten, Run, Master-Panes, Glut). Die Einstellungen
gelten für **alle** Terminal-Kacheln zugleich.

## Wo die Knöpfe sind

Rückseite der Kachel (Zahnrad):

- **„Session“:** „Shell“ (leer = `$SHELL`), „Start directory“, „Scrollback (lines)“, „Environment variables“ (eine
  Zeile `KEY=value` je Variable), „Visual bell“. Gilt erst für das **nächste** geöffnete Terminal.
- **„Font“:** Schrift (mitgelieferte, „Installed on this Mac (monospaced)“ oder „Custom…“), „Font family“, „Size
  (px)“, „Weight“.
- **„Appearance“:** „Theme“ (u. a. „xterm (default)“, „Dracula“, „Nord“, „Catppuccin …“, „Tokyo Night“, „GitHub …“,
  „Rosé Pine“, „One Dark“), „Background opacity“, „Cursor style“, „Cursor blink“, „Bold text in bright colour“.
  Gilt sofort.

## Häufige Handgriffe

1. **Terminal öffnen:** im App-Ring auf „Terminal“ (jedes Mal eine neue Kachel), Dock → „Add module“ oder den Chat
   bitten („öffne ein Terminal“).
2. **Text kopieren:** mit der Maus markieren — beim Loslassen ist er kopiert. Mit gedrückter Shift-Taste geht das
   auch, während ein Programm die Maus benutzt.
3. **Zurückblättern:** Mausrad, ⌘⇧↑/↓ oder Shift+PageUp/PageDown (am Mac Shift+fn+Pfeile).

## Fallen

- Am Mac erreicht ⌘ nie die Shell; die App-Kürzel gehen also immer. Unter Windows/Linux gehört Ctrl der Shell: dort
  nehmen App-Kürzel im Terminal Ctrl+Shift (Kopieren/Einfügen Ctrl+Shift+C/V, Schließen Ctrl+Shift+W).
- ⌘W schließt im Terminal nichts.
- Für Prompts mit Symbolen (Powerline, Starship) die Schrift „JetBrainsMono Nerd Font Mono“ wählen — sie ist die
  einzige echte Nerd-Font unter den mitgelieferten.
- Option bleibt Option (kein Alt/Meta).
- Ein Programm darf in die Zwischenablage nur schreiben, wenn sein Terminal vorn ist, und sie nie lesen.
- Links im Terminal öffnet ⌘+Klick.

## Siehe auch

[Studio](studio.md) · [Glut](glut.md) · [Einstellungen](einstellungen.md)
