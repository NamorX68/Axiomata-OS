# Tags und Wikilinks

Wie man Notizen mit `#tags` und `[[Wikilinks]]` verknüpft, wie die Tag-Liste in TAGS.md gepflegt wird und wie der einmalige Vault-Lauf Tags und Links für alle Notizen vorschlägt.

## Was es ist

Axiomata schreibt Tags und Links **wie Obsidian**, damit der Vault auch ohne die App lesbar bleibt:

- **Tags:** `#tag` im Text oder `tags:` im Frontmatter, verschachtelt als `#rust/async`. Kein Tag ist eine
  Überschrift, Code, ein Link/URL oder eine reine Zahl (`#1`). Groß/klein zählt nicht.
- **Wikilinks:** `[[Notiz]]`, `[[Notiz|Anzeigetext]]`, `[[Notiz#Überschrift]]`.
- **Die Tag-Liste** steht in `TAGS.md` an der Wurzel des Vaults, eine Zeile `- #tag — Beschreibung` je Tag. Die App
  schreibt sie; Agenten wählen Tags **nur aus ihr** und schlagen einen neuen Tag vor, wenn keiner passt. Ziel: etwa
  30–60 Tags im Vault, 1–4 je Notiz.
- **Agenten-Links** stehen in einem Abschnitt `## Related` am Ende einer Notiz, höchstens drei, nur auf Notizen, die
  es gibt; der übrige Text bleibt unverändert.

## Wo die Knöpfe sind

- **Einstellungen → Workspace → „Tags“:** „Von Agenten vorgeschlagen“ mit „Annehmen“ / „Ablehnen“; „Auf der Liste (…)“
  mit einer Beschreibungszeile je Tag, dem Stift („… umbenennen“: erst „Stellen zählen“, dann „Umbenennen“) und dem
  Papierkorb („… von der Liste nehmen“); darunter eine Zeile zum Hinzufügen („Hinzufügen“). Ein Sechseck mit Zahl am
  Reiter „Workspace“ und im Dock zeigt wartende Vorschläge.
- **Darunter „Den ganzen Vault taggen“:** „1 · Eine Tag-Liste vorschlagen“ („Vorschlagen“) und „2 · Tags und Links für
  jede Notiz planen“ („Planen“, später „Den Rest planen“), dann „Vorschau (… Notizen)“ mit „In … Notizen schreiben“ oder
  „Verwerfen“.
- **Im Editor:** nach `[[` Notizen vervollständigen, nach `#` Tags. ⌘-Klick (oder Vi `gd`, oder Klick in der
  Leseansicht) folgt dem Link; ⌃- springt zurück.
- **In der Leseansicht** stehen die Tags aus dem Frontmatter als Chips oben; ein Klick zeigt die Notizen mit diesem
  Tag im Second Brain. Dasselbe tun die Tag-Chips im Detailfeld des Second Brain, die Tag-Gruppe in ⌘K und das
  Suchfeld „Notizen oder #Tag finden“.

## Häufige Handgriffe

1. **Vorschlag annehmen:** Einstellungen → Workspace → „Von Agenten vorgeschlagen“ → „Annehmen“. Der Tag kommt auf die
   Liste und auf die Notizen, die ihn wollten.
2. **Tag umbenennen oder zusammenlegen:** beim Tag den Stift → neuen Namen → „Stellen zählen“ → „Umbenennen“. Ein Name,
   der schon auf der Liste steht, legt beide zusammen. Alle Notizen werden mit umgeschrieben.
3. **Den ganzen Vault taggen:** „Den ganzen Vault taggen“ → Schritt 1 „Vorschlagen“, Vorschläge annehmen → Schritt 2
   „Planen“ → „Vorschau“ prüfen → „In … Notizen schreiben“.

## Fallen

- Der Vault-Lauf kostet Geld (jeder Schritt nennt vorher, was er liest und kostet) und schreibt **nichts**, bevor
  du „In … Notizen schreiben“ klickst.
- „… von der Liste nehmen“ nimmt den Tag nur von der Liste; die Notizen behalten ihn.
- Ein Link auf eine Notiz, die es nicht gibt, wird gedämpft gezeigt; ⌘-Klick **legt sie an** (neben der aktuellen
  Notiz). Passt ein Name auf mehrere Notizen, nenne den Ordner mit.
- Benennst du eine Notiz in der App um, fragt sie „… Links in … Notizen anpassen?“ — mit „Anpassen“ ziehen die
  `[[Links]]` mit.
- `TAGS.md` nicht von Hand bearbeiten; dafür sind die Einstellungen da. Im Terminal gibt es für den Owner
  `axiomata-cli notes tags rename <alt> <neu>`, `notes tags describe <tag> "<Text>"` und `notes tags lift-inline`
  (holt Inline-Tags ins Frontmatter); `rename` und `lift-inline` zeigen mit `--dry-run` erst, was
  sich ändern würde.

## Siehe auch

[Second Brain](second-brain.md) · [Editor](editor.md) · [Notizen](notizen.md) · [Einstellungen](einstellungen.md)
