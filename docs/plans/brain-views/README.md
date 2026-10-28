# Brain-Ansichten: Alternativen zu Orbit, Ringe und Karte

Mock: [`brain-views-mock.html`](brain-views-mock.html) im Browser öffnen (Chrome/Safari, gebaut für 1600×1000). Sie
braucht nur d3@7.9.0 vom jsDelivr-CDN, alles andere steht in der Seite. Die Daten sind erfunden, aber mit festem
Startwert: 253 Notizen in 11 Bereichen, 310 Links, zwei Jahre Alter. Umschalten geht mit den Reitern oder den Tasten
1–6. Überall gilt: Ältere Notizen werden im Farbton ihres Bereichs dunkler, neue leuchten. Glut-Orange bekommt nur,
was lebt (Notiz jünger als 21 Tage, frische Links, „jetzt“).

## Die sechs Ansichten

| # | Ansicht | Zeigt gut | Schwäche |
|---|---|---|---|
| 1 | **Sternkarte**: Kraftgraph, jeder Bereich zieht seine Notizen zu einem Sternbild | Brücken zwischen Bereichen, verwaiste Notizen; wirkt lebendig | Das Layout ändert sich bei jedem Lauf, ab ~1000 Notizen wird es zum Knäuel |
| 2 | **Zellen**: Circle Packing, Klick zoomt in einen Bereich | Mengen und Hierarchie, semantischer Zoom ohne die Grenzlogik der Karte | Keine Links; liegt nah an „Ringe“ |
| 3 | **Bündel**: Hierarchical Edge Bundling, Notizen auf einem Ring (Bereich → Alter) | Querbezüge als gebündelte Ströme, stabil; Hover zeigt alle Links einer Notiz | Einzelne Notizen sind klein, Titel nur bei Hover |
| 4 | **Bänder**: Chord-Diagramm der Bereiche, außen ein Kranz mit einer Wabe pro Notiz | „Welche Bereiche reden miteinander?“; ruhig und lesbar | Zeigt nur Summen; über ~15 Bereiche wird es eng |
| 5 | **Strom**: Streamgraph, neue Notizen pro Monat und Bereich | Zeit als Achse: Phasen, wechselnde Interessen, eingeschlafene Bereiche | Keine einzelnen Notizen, keine Links, also eher Statistik |
| 6 | **Wachstum**: Zeitraffer wie Gource, Notizen keimen als Waben in Sonnenblumen-Spiralen (älteste innen) | Erzählt die Geschichte des Vaults und passt zu Glut + Hex; mit Schieber | Zum Anschauen gedacht, nicht zum Suchen; als Daueransicht zu unruhig |

Geprüft, aber nicht gebaut:
- **Voronoi-Treemap**: schön organisch, braucht aber eine zweite Bibliothek und kommt der Karte sehr nah.
- **Sunburst**: im Kern dasselbe wie die Ringe.
- **Räumliche Leinwand** (Heptabase/Obsidian Canvas): ist eine Arbeitsfläche, die man selbst anordnet, keine
  Übersicht über den ganzen Vault.

## Empfehlung

1. **Bündel als neues Layout neben Ringe/Karte/Hex.** Es ist die einzige der Ansichten, die die Links lesbar
   macht. Genau das fehlt der Karte und den Ringen. Das Layout ist stabil, was zur Wiedererkennung passt. Auf dem
   Ring kann man die Reihenfolge Bereich → Alter direkt aus der Karte übernehmen.
2. **Strom als schmales Zeitband** (zum Beispiel unter der Karte, ~120 px hoch) statt als eigene Ansicht. Ein Klick
   auf einen Monat könnte die Karte auf „was entstand damals“ filtern.
3. **Wachstum als Rückblick oder Leerlauf-Bild des Orbits** („So ist dein Vault gewachsen“), nicht als
   Arbeitsansicht.

Sternkarte und Zellen gehen gut, bringen gegenüber Orbit und Ringe aber wenig Neues. Bänder eignet sich als kleine
Kachel oder als Inspektor-Panel („Verbindungen dieses Bereichs“).

## Quellen

- Treemap, Voronoi-Treemap, Circle Packing im Vergleich: https://architect-api.readthedocs.io/en/latest/text/document-vis-hierarchy.html
- Hierarchical Edge Bundling: https://www.d3-graph-gallery.com/bundle und https://api.observablehq.com/document/@d3/hierarchical-edge-bundling/2
- Chord-Diagramm (Dependency): https://gist.github.com/mbostock/1046712
- Graph-Plugins für Obsidian (3D, Cluster, Workspace-Graph): https://community.obsidian.md/plugins/juggl,
  https://github.com/HEmile/juggl, https://community.obsidian.md/plugins/armillary,
  https://community.obsidian.md/plugins/graph-insight
- Netzwerk-Analyse von Notizen (Lücken zwischen Themen): https://infranodus.com/use-case/visualize-knowledge-graphs-pkm
- Gource (Wachstum als Zeitraffer): https://github.com/froh/Gource, https://concepts.dsebastien.net/concept/gource/
