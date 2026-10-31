# Second Brain

Wie man den Vault als Bild ansieht: die Ansichten Bündel, Karte und Hex, das Filterfeld, „Wie dein Vault wuchs“, die Zeitreise und das Detailfeld einer Notiz.

## Was es ist

Das Second Brain zeigt den ganzen Vault als Grafik: jede Notiz ein Punkt oder eine Zelle, jeder Bereich (oberster
Ordner) eine eigene Farbe, Links als Linien, die Skills außen herum. Es liegt als Scheibe über dem
[Orbit](orbit.md), nicht als eigener Bildschirm.

- **Bündel:** alle Notizen auf einem Ring, ein Bogen je Bereich, die neuesten zuerst; Links laufen gebündelt durch
  die Mitte, die der letzten 21 Tage leuchten.
- **Karte:** eine Landkarte — jeder Bereich ein Land, jede Notiz ein Feld, die ältesten in der Mitte; Links sind
  Straßen (hineinzoomen).
- **Hex:** eine Wabe — jede Notiz eine eigene Zelle, jeder Bereich ein Keil, die neuesten nahe der Mitte.

## Wo die Knöpfe sind

- **Öffnen:** Dock „Brain“, ein Klick in die Orbit-Wolke, ⌘K „Second Brain öffnen“ oder `/brain` im Chat.
- **Oben in der Ansicht:** „Anordnung“ („Bündel“ / „Karte“ / „Hex“), „Gruppieren nach“ („Bereichen“ / „Ordnern“), das
  Suchfeld „Notizen oder #Tag finden“, „▶ Wie dein Vault wuchs“, „Ansicht zurücksetzen“, „Neu laden“ und „?“ („Hilfe“
  mit Legende); darunter „… Notizen · … Bereiche · … Links“.
- **Zeitreise:** die Uhr „Zeitreise: wie der Vault wuchs“ öffnet einen Schieber; „Den Vault wachsen lassen“ spielt ab,
  „Zurück zu heute“ schließt.
- **Detailfeld** (nach Klick auf eine Notiz, verschieb- und vergrößerbar): „Ordner“, „Größe“, „Geändert“, „Pfad“,
  „Links“, „Tags“ als Chips, eine Vorschau; Knöpfe „Öffnen“, „Pfad kopieren“, „Hinfliegen“, „Löschen“; die Listen
  „Verlinkt auf“ und „Verlinkt von“. Bei einem Bereich „Notizen in diesem Bereich“, bei einem Skill „▶ Ausführen“.
- **Dauerhaft einstellen:** Einstellungen → Darstellung → „Orbit & Second Brain“ („Tempo“, „Beschriftungen“,
  „Dateinamen“, „Schimmer“, „Anordnung“).

## Häufige Handgriffe

1. **Notizen finden:** ins Suchfeld Wörter tippen (jedes muss passen; Name, Pfad oder Bereich) oder `#tag` — passt
   auch auf verschachtelte Tags darunter. Alles andere wird abgedunkelt.
2. **Eine Notiz öffnen:** anklicken → „Öffnen“ (öffnet sie lesend im Dateifenster).
3. **Wachstum ansehen:** „▶ Wie dein Vault wuchs“ spielt die Geschichte des Vaults in 20 Sekunden ab; Esc beendet.

## Fallen

- „Löschen“ im Detailfeld löscht die **Datei im Vault** (nach der Rückfrage „Wirklich löschen“) — nicht nur den Punkt.
- Esc wirkt stufenweise: erst die Wiedergabe beenden, dann die Auswahl aufheben, dann die Ansicht schließen.
- Fehlt eine eben angelegte Notiz, liest „Neu laden“ den Vault neu ein.
- Es gibt keine Schreibweise `tag:x` — nur `#x`.

## Siehe auch

[Orbit](orbit.md) · [Tags und Links](tags-links.md) · [Notizen](notizen.md) · [Editor](editor.md)
