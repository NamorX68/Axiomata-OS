# Tags + Links — #Tags und [[Wikilinks]] wie in Obsidian (Karte #75)

Stand: 2026-10-29, **gegrillt** (Q1–Q10, Owner), Go zum Bau. **TL1, TL3–TL6 gebaut** auf Branch `tags` (2026-10-29,
29db892 · bacd7b2 · 88e03d0 · eaebc26 · 843fe25, dazu 8811865: die Leseansicht lässt das Frontmatter weg); TL2 im Branch
`editorlinks`. Nicht gemergt, nicht installiert; offen: Mac-Test, `skills reseed --force`, den Lauf TL6 startet der
Owner. Karte #75 (Owner, 2026-10-11): Tags, damit man Notizen
in Spotlight und im Second Brain findet; Links mit einer echten Möglichkeit, ins verlinkte Dokument zu springen und
zurück. Obsidian wird auf lange Sicht nicht mehr genutzt — Axiomata macht es **wie dort**, damit der Vault ohne die
App lesbar bleibt.

## 1. Was es heute gibt

- Der Vault-Scan (`crates/axiomata-core/src/graph.rs` `extract_links`) erkennt `[[Ziel]]`, `[[Ziel|Alias]]`,
  `[[Ziel#Überschrift]]` und relative Markdown-Links; daraus sind die Kanten im Second Brain, das Detailpanel zeigt
  aus- und eingehende Links. Tags liest niemand (nur der Obsidian-Import, `importer.rs`, entfernte sie).
- Im Editor sind Wikilinks Text. ⌘-Klick und `gd` gehen nur mit einem Sprachserver (`FileEditor.svelte`
  `goToDefinition`, `onDefinitionAt` nur bei `lspDoc`). Ein Sprungverlauf über Dateien fehlt (`<C-o>` nur im
  Dokument, `editor/vi/marks.ts`). Die Vervollständigung hat eine Quelle-Schnittstelle (`fileapp/completionMenu.ts`
  `CompletionPort`). Markdown hat eine Leseansicht (`fileapp/MarkdownPreview.svelte`, `core/markdown.ts`).
- `inbox-sort` verschiebt nur (`allowed_tools: Read Bash(axiomata-cli inbox move:*)`), ändert nie eine Datei.
- Eine neue Notiz: ⌘S → `create_note` → ein Agent-Zug mit `notes::placement_prompt` wählt `{area, file_name,
  title?}`; kein Frontmatter.
- Der Vault: 157 Notizen, 3 mit `tags:`, inline fast nur `#ki-news` / `#ki-modelle` (die News-Skills).

## 2. Entscheidungen (Owner, 2026-10-29)

- **Q1 Schreibweisen wie Obsidian:** `#tag` im Text **und** `tags:` im Frontmatter (Liste oder Komma-Text),
  verschachtelt `#rust/async`; mehrere je Notiz. Kein Tag: `#` am Zeilenanfang einer Überschrift, in Code (Block und
  Inline), in URLs/Links, reine Zahlen (`#1`). Erlaubte Zeichen wie Obsidian (Buchstaben inkl. Umlaute, Ziffern,
  `_ - /`). Vergleich ohne Groß/Klein.
- **Q2 Springen:** ⌘-Klick auf `[[…]]` im Editor, `gd` im Vi-Modus, Klick in der Leseansicht; **zurück** mit ⌃-
  (und `<C-o>` über Dateien hinweg, wenn der Verlauf im Dokument leer ist) — ein app-weiter Sprungverlauf.
- **Q3 Link ins Leere** wird gedämpft gezeigt; ⌘-Klick legt die Notiz an (im Ordner der aktuellen Notiz, Titel =
  Linkziel), wie Obsidian.
- **Q4 Vervollständigung:** nach `[[` die Notizen (unscharf, Bereich als Hinweis), nach `#` die Tags der Tag-Liste
  und die im Vault vorkommenden.
- **Q5 Tags in der Suche:** Spotlight und die Second-Brain-Suche finden `#tag`; ein Tag-Filter im Second Brain
  dimmt alles andere; das Detailpanel zeigt die Tags einer Notiz als Chips (Klick = Filter). Keine Tag-Gruppierung,
  keine Tag-Knoten (später prüfen).
- **Q6 Eine Tag-Liste** mit je einer Zeile Beschreibung. Agenten wählen **nur aus ihr**; passt nichts, schlagen sie
  einen neuen Tag vor, der Owner nimmt an oder lehnt ab. Von Hand setzt der Owner jeden Tag.
- **Q7 Größe:** Ziel 30–60 Tags im Vault, 1–4 je Notiz — Vorgabe in den Prompts, nicht hart erzwungen.
- **Q8 Agenten-Links:** nur auf vorhandene, klar verwandte Notizen, höchstens 3, in einem Abschnitt `## Related` am
  Ende; der Text der Notiz bleibt unverändert; nie ein Link auf eine Notiz, die es nicht gibt.
- **Q9 Einmaliger Lauf** über den Vault: zuerst schlägt er die Tag-Liste vor (Owner gibt frei), dann Tags + Related
  je Notiz — mit Vorschau dessen, was er ändert, und der Kosten, bevor etwas geschrieben wird.
- **Q10** Tags aus der freigegebenen Liste setzt ein Agent direkt; nur ein **neuer** Tag geht als Vorschlag.
- Dazu, beim Owner-Go mitgenommen: auch `inbox-sort` und das Speichern einer neuen Notiz setzen Tags und Related.

**Vorgaben ohne eigene Frage** (Owner kann widersprechen):
- **V1 Ort der Tag-Liste:** `<workspace>/TAGS.md` (Wurzel, neben `AGENTS.md`), eine Zeile je Tag
  `- #tag — Beschreibung`; geschrieben **nur von der App** (Ansicht in den Einstellungen, „Nichts von Hand in
  Dateien“), gelesen von Agenten und lesbar ohne App. Vorschläge liegen in der DB (`tag_proposals`), nicht in der Datei.
- **V2 `inbox-sort` bekommt keine allgemeinen Schreibrechte**, sondern einen engen CLI-Befehl
  `axiomata-cli notes tag <file> [--tag t]… [--related <note>]… [--propose t:"Beschreibung"]…`: er setzt nur
  `tags:` im Frontmatter und den Abschnitt `## Related`, prüft Tags gegen die Liste (unbekannte → Vorschlag) und
  Related-Ziele gegen den Vault (unbekannte → abgelehnt). Ein direkter CLI-Befehl wie `inbox move`
  (`agent_entry::cli_direct_program`, `skill_run_may`).
- **V3 Umbenennen in der App** (Datei-App, Second Brain) zieht `[[Name]]`-Links auf die Notiz in den anderen Notizen
  mit, wie Obsidian; Alias und `#Überschrift` bleiben.
- **V4** `#ki-news` / `#ki-modelle` der News-Skills kommen als erste Einträge in die Liste.

## 3. Bau

Schnittstelle zwischen den Teilen: `WorkspaceGraph.files[].tags: string[]` (klein, ohne `#`, aus Frontmatter und
Text vereinigt) und `get_tag_list` → `{ tag, description }[]`.

- **TL1 Kern:** Tag-Erkennung im Scan (`graph.rs`), `tags` je Datei; `TAGS.md` lesen/schreiben (`core::tags`),
  `tag_proposals` (Migration), Tauri `get_tag_list|save_tag_list|tag_proposals|answer_tag_proposal`, CLI
  `notes tag` (V2) und `notes tags list`. Tests: Erkennung inkl. Ausnahmen, Frontmatter-Formen, Schreiben von
  `tags:`/`## Related` ohne den Rest der Datei zu ändern.
- **TL2 Editor:** Wikilinks und Tags farbig, ⌘-Klick/`gd`/Klick in der Leseansicht springen, Link ins Leere legt an,
  app-weiter Sprungverlauf mit ⌃-, Vervollständigung für `[[` und `#` (`CompletionPort`), Umbenennen zieht Links mit
  (V3).
- **TL3 Suche + Second Brain:** `#tag` in Spotlight und der Second-Brain-Suche, Tag-Filter, Chips im Detailpanel.
- **TL4 Tag-Liste in der App:** Einstellungen → Workspace → „Tags“: Liste mit Beschreibung, anlegen, umbenennen
  (zieht in den Notizen mit), löschen; wartende Vorschläge mit Annehmen/Ablehnen (Warte-Abzeichen wie bei Fragen).
- **TL5 Agenten:** `create_note` liefert zusätzlich `tags` (aus der Liste), `related` (≤ 3) und `propose`; schreibt
  Frontmatter + Related. `inbox-sort` nutzt `notes tag` nach dem Verschieben (`allowed_tools` um
  `Bash(axiomata-cli notes tag:*)` erweitert, reseed). Prompts nennen Q7/Q8.
- **TL6 Einmaliger Lauf (Q9):** ein Skill `vault-tagging` in zwei Schritten — `--propose-list` (Vorschlag der Liste
  → Vorschläge zum Freigeben) und danach je Notiz `notes tag`; vorher eine Vorschau (welche Notiz bekommt was) und
  eine Kostenschätzung; gestartet vom Owner in TL4s Ansicht.

**Abweichungen beim Bau** (2026-10-29): Der Lauf TL6 ist ein Skill für beide Schritte; welchen er tut, sagt ihm
`axiomata-cli notes tags todo` (die App setzt den Schritt beim Start, `tag_run`). Die Vorschau ist kein zweiter Lauf:
im Notizen-Schritt schreibt `notes tag --dry-run` den Plan in `tag_plan`, und „Write into N notes“ schreibt ihn ohne
Agent; die dort geplanten Vorschläge gehen erst dabei an den Owner (mit der Notiz). Neue Befehle `notes tags
todo|propose|skip` (`skip`: eine Notiz bleibt aus dem Lauf, mit Grund; eine verweigerte Notiz bleibt von selbst
draußen). Eine Notiz, die beim Schreiben scheitert, bleibt in der Vorschau für einen zweiten Versuch. Aus den Security-Audits: der Platzierungszug für eine neue Notiz darf nur
lesen und bekommt höchstens 40 passende Notiznamen (nicht alle); ein unbeaufsichtigter Skill-Lauf taggt (nicht
probeweise) nur eine Notiz, die in der letzten Stunde an ihren Platz kam; Agenten-Sitzungen des Studios taggen nicht;
ein `tags:` in einer Form, die nicht unverändert zurückgeschrieben werden kann, wird nie umgeschrieben;
`rename_note_links` lehnt Namen mit `[ ] | # ^` u. ä. ab und verlangt, dass genau eine Seite existiert.

**Bekannte Grenzen** (kombinierter Review, offen gelassen): `rename_note_links` zieht nur `[[…]]` im Text mit, nicht im
Frontmatter und nicht relative Markdown-Links `[a](Alt.md)`; `rename_tag` meldet Notizen nicht, die es auslässt (ein
`tags:` in fremder Form, ein Schreibkonflikt); ein `---`-Block ganz oben gilt immer als Frontmatter; ein unbekannter
`--tag` wird wie in V2 ein Vorschlag (ohne Beschreibung), auch über die „ein Vorschlag“-Grenze des Prompts hinaus.

Prüfungen: der volle Gate vor jedem Commit; `security-auditor` für TL1/TL5 (schreibt in Vault-Dateien, neue
Tauri-Befehle und ein CLI-Befehl für Skills); ein kombinierter Review vor dem Abschluss. Karte #125 (Datum je Link)
ist nicht Teil davon.

## Nachtrag 2026-10-29: Q4–Q9 aus dem Tagging-Konzept

Aus der Grill-Runde des Owners über seine Konzeptnotiz (2026-10-29), gebaut auf Branch `tags`:

- **Q4/Q5 Werkzeug zum Aufräumen der Liste** — dieselbe Kernlogik wie Einstellungen → Tags, aus dem Terminal:
  `axiomata-cli notes tags rename <alt> <neu> [--description "…"] [--dry-run] [--json]`,
  `axiomata-cli notes tags describe <tag> "<Text>" [--json]`. `rename_tag` zieht den Tag im Frontmatter und im Text
  mit (verschachtelte mit), und ein neuer Name, der schon auf der Liste steht, ist eine **Zusammenführung**: eine Notiz
  mit beiden behält einen, der Eintrag behält seine Beschreibung (außer `--description` gibt eine). Danach steht der
  neue Name immer auf der Liste (auch wenn der alte nur in Notizen vorkam). Abgelehnt: ein alter Tag, der weder in
  einer Notiz noch auf der Liste steht (Tippfehler), und ein neuer Name unterhalb des alten (`ai` → `ai/alt`). Notizen,
  die es nicht (ganz) umschreiben konnte (fremde `tags:`-Form, Schreibkonflikt), meldet es jetzt (`skipped`) — damit ist
  die bekannte Grenze „meldet ausgelassene Notizen nicht“ behoben. `describe` setzt die Beschreibung und nimmt einen
  fehlenden Tag auf. Jede Änderung der Liste prüft die gelesene Version (`list::update`).
- **Q6 Chips in der Leseansicht**: `MarkdownPreview.svelte` zeigt die `tags:` des Frontmatters einer Vault-Notiz als
  schlichte Chips oben (kein Orange); ein Klick öffnet das Second Brain mit `#tag` gefiltert
  (`core/frontmatterTags.ts`, liest wie `detect::tags_key`).
- **Q7 Tags nur im Frontmatter**: `news-rss`, `news-web` und `model-news` schreiben kein `· #ki-news` mehr in die
  Einträge, ihr Tag steht nur im Frontmatter — `ai/news` bzw. `ai/models`. Einmalig:
  `axiomata-cli notes tags lift-inline [--tag t]… [--dry-run] [--json]` (`tags::lift`) hebt Inline-Tags (die genannten
  samt Kindern, sonst alle) ins `tags:` — eine Reihe von Tags am Zeilenanfang oder -ende geht mit ihrem Trenner
  (`· #ki-news`), eine Zeile, die nur noch Tags oder ein `Tags:`-Label hatte, fällt weg (samt einer Leerzeile), ein Tag
  mitten im Satz behält sein Wort ohne `#`; Code, Links, URLs und Überschriften bleiben unberührt; geschützte Dateien
  und `Kanban/` ausgelassen, eine Notiz mit fremder `tags:`-Form gemeldet und nicht geschrieben.
- **V4 neu**: eine neue Tag-Liste beginnt mit `#ai/models` und `#ai/news`, deutsch beschrieben.
- Alle drei Befehle gehören dem Owner (`owner_only`): in Agenten-Sitzungen, Skill-Läufen und Chat-Zügen gesperrt, auch
  als Probelauf, und keine direkten CLI-Befehle (`skill_run_may` nennt von `notes tags` nur `list|todo|skip|propose`).
- Aus dem Security-Audit: `vault-tagging` deklariert statt `notes tags:*` nur noch `notes tags list|todo|propose|skip`
  (ein Test prüft, dass kein mitgelieferter Skill die Owner-Befehle abdeckt); ein Probelauf von `rename` legt kein
  `TAGS.md` an, die neue Liste wird vor dem ersten Schreiben einer Notiz geprüft; `lift-inline` lässt HTML-Kommentare
  und eingerückten Code stehen und entfernt nie eine Tabellenzeile; höchstens 500 Chips je Notiz. Offen gelassen
  (LOW): `#fff`/`#abc123` in Fließtext gelten wie in Obsidian als Tag; scheitert das Schreiben der Liste nach den
  Notizen (Konflikt), tragen die Notizen schon den neuen Namen — dann den Befehl wiederholen.
- Die echte Umstellung (Umbenennungen, deutsche Beschreibungen, `lift-inline`) macht der Koordinator mit diesen
  Befehlen am echten Vault, jeweils zuerst mit `--dry-run`.
