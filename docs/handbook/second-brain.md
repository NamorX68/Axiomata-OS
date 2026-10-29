# Second Brain

Wie man den Vault als Bild ansieht: die Ansichten Bundle, Map und Hex, das Filterfeld, „How your vault grew“, die Zeitreise und das Detailfeld einer Notiz.

## Was es ist

Das Second Brain zeigt den ganzen Vault als Grafik: jede Notiz ein Punkt oder eine Zelle, jeder Bereich (oberster
Ordner) eine eigene Farbe, Links als Linien, die Skills außen herum. Es liegt als Scheibe über dem
[Orbit](orbit.md), nicht als eigener Bildschirm.

- **Bundle:** alle Notizen auf einem Ring, ein Bogen je Bereich, die neuesten zuerst; Links laufen gebündelt durch
  die Mitte, die der letzten 21 Tage leuchten.
- **Map:** eine Landkarte — jeder Bereich ein Land, jede Notiz ein Feld, die ältesten in der Mitte; Links sind
  Straßen (hineinzoomen).
- **Hex:** eine Wabe — jede Notiz eine eigene Zelle, jeder Bereich ein Keil, die neuesten nahe der Mitte.

## Wo die Knöpfe sind

- **Öffnen:** Dock „Brain“, ein Klick in die Orbit-Wolke, ⌘K „Open Second Brain“ oder `/brain` im Chat.
- **Oben in der Ansicht:** „Layout“ („Bundle“ / „Map“ / „Hex“), „Group by“ („Areas“ / „Folders“), das Suchfeld
  „Find notes or #tag“, „▶ How your vault grew“, „Reset view“, „Reload“ und „?“ (Hilfe mit Legende); darunter
  „… notes · … areas · … links“.
- **Zeitreise:** die Uhr „Time travel: how the vault grew“ öffnet einen Schieber; „Grow the vault“ spielt ab,
  „Back to today“ schließt.
- **Detailfeld** (nach Klick auf eine Notiz, verschieb- und vergrößerbar): „Folder“, „Size“, „Modified“, „Path“,
  „Links“, „Tags“ als Chips, eine Vorschau; Knöpfe „Open“, „Copy path“, „Fly to“, „Delete“; die Listen „Links to“
  und „Linked from“. Bei einem Bereich „Notes in this area“, bei einem Skill „▶ Run“.
- **Dauerhaft einstellen:** Settings → Appearance → „Orbit & Second Brain“ („Speed“, „Labels“, „File names“,
  „Shimmer“, „Layout“).

## Häufige Handgriffe

1. **Notizen finden:** ins Suchfeld Wörter tippen (jedes muss passen; Name, Pfad oder Bereich) oder `#tag` — passt
   auch auf verschachtelte Tags darunter. Alles andere wird abgedunkelt.
2. **Eine Notiz öffnen:** anklicken → „Open“ (öffnet sie lesend im Dateifenster).
3. **Wachstum ansehen:** „▶ How your vault grew“ spielt die Geschichte des Vaults in 20 Sekunden ab; Esc beendet.

## Fallen

- „Delete“ im Detailfeld löscht die **Datei im Vault** (nach der Rückfrage „Really delete“) — nicht nur den Punkt.
- Esc wirkt stufenweise: erst die Wiedergabe beenden, dann die Auswahl aufheben, dann die Ansicht schließen.
- Fehlt eine eben angelegte Notiz, liest „Reload“ den Vault neu ein.
- Es gibt keine Schreibweise `tag:x` — nur `#x`.

## Siehe auch

[Orbit](orbit.md) · [Tags und Links](tags-links.md) · [Notizen](notizen.md) · [Editor](editor.md)
