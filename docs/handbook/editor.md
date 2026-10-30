# Datei-App und Editor

Wie man Dateien öffnet, liest und bearbeitet: das Dateifenster, der Editor im Studio, Leseansicht, Vi-Modus, Suche, Sprachserver (LSP) und Sprungverlauf.

## Was es ist

Dateien öffnen sich an zwei Orten mit demselben Editor:

- im **schwebenden Dateifenster** — aus dem Second Brain, mit `/open <pfad>`, ⌘K oder wenn ein Agent dir etwas
  zeigt; mit „In der Datei-App öffnen“ wechselst du ins Studio;
- im **Studio**, Modus „Editor“, mit Dateibaum, Reitern und geteilten Gruppen (siehe [Studio](studio.md)).

Markdown und HTML lassen sich als Quelle oder gerendert zeigen; Bilder erscheinen als Bild. Für Code starten
Sprachserver von selbst (Fehler, Sprung zur Definition, Vervollständigen, Umbenennen, Formatieren), sofern sie auf
dem Mac installiert sind.

## Wo die Knöpfe sind

- **Ansicht:** ⇧⌘V wechselt „Quelltext“ / „Vorschau“ / „Nebeneinander“. ⌥Z schaltet den Zeilenumbruch je Datei.
- **Seitenleiste im Studio:** „Dateien“ (Rechtsklick: „Neue Datei“, „Neuer Ordner“, „Umbenennen“, „Löschen…“), „Suche“,
  „Git“; dazu „Symbole filtern“ in der Gliederung.
- **Suchen:** ⌘F in der Datei (Schalter „Groß/klein beachten“, „Ganzes Wort“, „Regulärer Ausdruck“, „In der Auswahl
  suchen“), ⌥⌘F Suchen und Ersetzen, ⌘G / ⇧⌘G weiter/zurück; ⇧⌘F im ganzen Projekt (mit „Einschließen“ /
  „Ausschließen“); ⌘P Datei nach Namen („:12“ springt zur Zeile).
- **Sprachserver:** F12 oder ⌘-Klick Definition, ⇧F12 alle Verwendungen, ⌘F12 Implementierungen, Maus ruhen lassen
  für Hilfe, F8 / ⇧F8 nächster/voriger Fehler, ⌘. Code-Aktionen, F2 Umbenennen, ⇧⌥F Formatieren, ⌃Leertaste
  Vervollständigen. In Vi: `gd`, `K`, `grr`, `gri`, `grt`, `gra`, `grn`, `]d` / `[d`.
- **Sprungverlauf:** ⌃- zurück, ⌃⇧- vor (Windows/Linux: Ctrl+Alt+← / →); in Vi auch `Ctrl-o` / `Ctrl-i`.
- **Einstellungen:** im Studio „Editor-Einstellungen“ (Reiter „Einstellungen“ / „Tastenkürzel“): „Tasten“ („Normal
  (Mac)“ oder „Vi“), „Vi-Zwischenablage“, Schrift, „Zeilennummern“, „Zeilenumbruch“, „Einrückung“, „Cursor“,
  „Editor-Theme“, „Minimap“, „Mitlaufende Kopfzeilen“, „Beim Speichern formatieren“, „Automatisch speichern“ und mehr.
  „Tastenkürzel“ im Studio-Kopf listet alle Tasten.

## Häufige Handgriffe

1. **Datei schnell finden:** ⌘P, Namen tippen, Enter (im Studio) — oder ⌘K überall.
2. **Vi einschalten:** Studio → „Editor-Einstellungen“ → „Tasten“ → „Vi“. `:w` speichert, `:q` schließt.
3. **Editor teilen:** ⌘\ legt den Reiter rechts daneben, ⇧⌘\ darunter; oder einen Reiter an den Rand ziehen.

## Fallen

- Ohne installierten Sprachserver gibt es keine Fehler und keine Definitionen — die App zeigt dann nur einen
  leisen Hinweis zur Installation.
- „Automatisch speichern“ steht auf Wunsch auf „Nach 1 s Pause“ oder „Beim Verlassen des Editors“; eine **neue Notiz**
  speichert es trotzdem nie (siehe [Notizen](notizen.md)).
- ⌘L wählt ohne Sprachserver die Zeile aus (nochmal: die nächste dazu); läuft ein Sprachserver, öffnet ⌘L die
  Code-Aktionen wie ⌘.
- Unter Windows/Linux nehmen einige Kürzel Ctrl+Shift statt Ctrl (⌘P/N/O/B/W/R); die Liste unter „Tastenkürzel“ zeigt
  sie für die eingestellte Tastatur.
- Ein Klick im Baum öffnet eine Vorschau, die der nächste Klick ersetzt; ein Doppelklick öffnet den Reiter fest.

## Siehe auch

[Notizen](notizen.md) · [Tags und Links](tags-links.md) · [Studio](studio.md) · [Einstellungen](einstellungen.md)
