# Bündel + Wachstum — zwei neue Darstellungen im Second Brain

Stand: 2026-10-29, **gegrillt** (Q1–Q11, Owner). Grundlage: der Mock `docs/plans/brain-views/brain-views-mock.html`
(sechs Alternativen, Recherche und Quellen in `docs/plans/brain-views/README.md`); der Owner wählte daraus „Bündel“
und „Wachstum“. Heute: Rings, Map, Hex (`core/brainView.ts` `layout`, `graph/layout.ts`, `graph/render.ts`,
`shell/SecondBrainView.svelte`).

## 1. Entscheidungen (Owner, 2026-10-29)

- **Q1 Bündel ersetzt Rings.** Die Anordnungen sind danach **Bündel, Map, Hex**. Bündel ist eine vollwertige
  Arbeitsansicht: Anklicken, Detailpanel, Hover, „Fly to“, Gruppieren. Hex behält intern die Keil-Geometrie der
  Rings (`layoutHex` legt die Waben in die Keile der Bereiche) — nur die Anordnung „Rings“ verschwindet.
- **Q2 Wachstum ist ein Wiedergabe-Knopf** („▶ How your vault grew“) im Second Brain, nicht im Umschalter. Er
  spielt einmal ab. Ein Leerlaufbild im Orbit kommt später als eigener Punkt.
- **Q3 Zeitreise auch in Bündel:** Notizen und Links, die es am gewählten Tag noch nicht gab, fehlen auf dem Ring.
- **Q4 Reihenfolge:** Auf jedem Bereichsbogen kommen die **neuesten zuerst**, dann im Uhrzeigersinn die älteren.
- **Q5 Ring-Aufbau:** Notizen auf dem Ring, Bereichssymbole und -namen außen am Ring, Skills als zweiter Ring weiter
  außen (anklickbar, startbar), **kein Hub** in der Mitte (dort laufen die Kurven).
- **Q6 Der Grund bleibt**, wie er ist (keine Farbflecken wie im Mock).
- **Q7** Eine gespeicherte Anordnung `rings` liest sich als `bundle`.
- **Q8 Gruppieren nach Ordnern** auch in Bündel: Der Ring wird feiner unterteilt, die Kurven bündeln entlang der
  Ordnerhierarchie (Hierarchical Edge Bundling, Holten 2006: Kontrollpunkte über die Vorfahren).
- **Q9 Wachstum spielt in fester Dauer** (≈ 20 s, unabhängig vom Alter des Vaults), mit Pause/Weiter, Schieber zum
  Spulen; Esc oder Klick beendet. Bei „Motion: Off“ springt es auf den Endstand und zeigt nur den Schieber
  (`core/motion.ts`).
- **Q10 Ende:** Wachstum blendet weich in die gewählte Anordnung über.
- **Q11 Kurvenfarbe:** in der Farbe des Bereichs der Quelle, gedimmt; Links der letzten 21 Tage in Glut-Orange
  (Akzentregel: frisch = lebt). Hover/Auswahl hebt die Kurven einer Notiz hervor und dimmt den Rest.

Farben überall wie seit 2026-10-29: Bereichsfarbe vom Ring, älter = dunkler im selben Farbton (`hive::agedRgb`),
frisch = kleiner Glutpunkt (`EMBER_SIZE`).

## 2. Bau

### Checkpoints

- **BV1 Bündel:** `layoutBundle` in `graph/layout.ts` (Bereiche → Bögen mit Lücken, Notizen neueste zuerst; bei
  Ordnern feinere Bögen), Rendermodus `bundle` in `graph/render.ts` (Kurven als B-Splines über die Hierarchie mit
  Bündelungsstärke β ≈ 0,85, Bereichs- und Skill-Ring, Treffertest, Zeitreise, Beschriftungen wachsen mit dem Zoom).
  `BrainLayout` = `bundle | map | hex`, `rings` → `bundle` beim Lesen; Rings-Code, der nur der Anordnung diente,
  fällt weg. Legende und Hilfetext. Tests: Ordnung auf dem Bogen, Kontrollpunkte, Migration.
- **BV2 Wachstum:** eine Wiedergabe über der gewählten Anordnung (`graph/growth.ts` für die reine Zeitrechnung und
  Positionen: Kern, Bereichsäste, Sonnenblumen-Spirale; Zeichnen im Renderer), Knopf im Second Brain, Bedienleiste,
  Überblendung am Ende, Motion Off. Tests für die Zeitrechnung und die Positionen.

Prüfungen: der volle Gate vor jedem Commit, ein kombinierter Review vor dem Abschluss, Sicht im Browser-Mock.
