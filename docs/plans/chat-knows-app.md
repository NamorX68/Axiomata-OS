# Der Chat kennt Axiomata — Handbuch, Lage, Bedienen (Karte #78)

Stand: 2026-10-29, CH1–CH5 **gebaut, am Mac abgenommen und gemergt**; dazu verschiebbare, größenveränderliche Fenster
(das Handbuch) und `close_view` — der Chat schließt große Fenster (Grundlage für Sprache, #104). Karte #78: „Der Chat muss Axiomata in und auswendig kennen.“

## 1. Heute

- Der Chat unten in der Leiste (`agents::chat`, `agents/mod.rs:636`) bekommt `~/.axiomata/module-context.md`: die
  feste Kurzanleitung `APP_GUIDE` (`bridge.rs:84-120`, ~37 Zeilen: Vault, Router, Skill-Format, Routinen, Board,
  Verhaltensregeln), die Aktionen der geöffneten Kacheln, die Kanban-Aktionen; dazu die Slash-Befehle. Zusammen
  ~1,5–3 k Tokens je Zug.
- Er kennt die Einstellungen, das Studio, Kosten, Wartung, Tags, Second Brain und Glas nicht; Lage bekommt er keine,
  und unter Glut erreicht er die Lese-Befehle der CLI nicht (die Sandbox verbirgt `~/.axiomata`; frei sind nur
  `DIRECT_COMMANDS`, `agent_entry.rs:141`).
- Eine Hilfe in der App gibt es nicht (nur `/help` mit der Befehlsliste).

## 2. Entscheidungen (Owner, 2026-10-29)

- **Q1** Der Chat **erklärt**, kennt die **Lage** und **bedient** — gebaut in dieser Reihenfolge.
- **Q2** Das Wissen ist ein **Handbuch** auf Deutsch, eine Seite je Bereich, mit den echten Namen der Knöpfe; im
  Prompt steht nur das Inhaltsverzeichnis, der Chat liest Seiten bei Bedarf.
- **Q3** Nur der Chat in der Leiste (das Handbuch liegt so, dass später jeder es lesen kann).
- **Q4** Er antwortet nur aus Handbuch und Live-Daten; steht etwas nicht drin, sagt er es und schlägt vor, das
  Handbuch zu ergänzen.
- **Q5** **Gepflegt im Projekt** (`docs/handbook/`), **ausgeliefert mit dem Build** und beim Start nach
  `~/.axiomata/handbook/` gelegt. Es gehört zum Build: die App ersetzt es, wenn es nicht zum installierten Build passt
  (anders als die Skills, die nur einmal angelegt werden); eine `README.md` im Ordner sagt, dass man es dort nicht
  bearbeitet. Eigene Notizen zur App gehören in den Vault.
- **Q6** Regel in `AGENTS.md` (wer eine sichtbare Funktion baut oder ändert, pflegt ihre Seite; sonst ist die Runde
  nicht fertig) und ein **Test**, dass jeder Bereich der App eine Seite hat.
- **Q7** Lese-Befehle `axiomata-cli app studio|spend|runs|routines|board|storage|settings|tags` — nur lesend, für den
  Chat freigeschaltet (auch unter Glut); Schlüssel und Tokens erscheinen nie, auch nicht maskiert (nur Feldnamen).
- **Q8** Bedienen: **direkt** Ansichten öffnen und Darstellung umstellen; **nach Rückfrage** einen Skill starten, eine
  Routine anlegen/ändern, eine Karte starten; **nie** löschen, Schlüssel ändern, Agenten im Studio steuern,
  Owner-Schritte auf dem Board.
- **Q9** Der Owner liest selbst: ⌘K findet Handbuchseiten, ein `?` in der Leiste öffnet das Handbuch als Panel
  (Leseansicht), `/help <Thema>` im Chat.
- **Q10** Erste Fassung: jeder Bereich kurz — was er ist, wo die Knöpfe sitzen, die drei häufigsten Handgriffe, die
  Fallen; ~20 Seiten zu je etwa einer Bildschirmseite, ohne Screenshots.

## 3. Die Seiten

`docs/handbook/<name>.md`, Titel als `# …`, ein Satz Zusammenfassung darunter (er wird das Inhaltsverzeichnis):

| Datei | Bereich |
|---|---|
| `index.md` | Überblick: was Axiomata ist, wie die Seiten zusammenhängen |
| `orbit.md` | Orbit, App-Ring, Kacheln, Dock |
| `second-brain.md` | Second Brain: Bündel, Map, Hex, Zeitreise, Wachstum, Filter |
| `notizen.md` | Notizen anlegen und speichern (Ablage durch den Agenten), Inbox |
| `editor.md` | Datei-App und Editor: Vi, Suche, LSP, Leseansicht, Sprungverlauf |
| `tags-links.md` | Tags, Wikilinks, Tag-Liste, Vault-Lauf |
| `studio.md` | Studio: Projekte, Agenten, Panes, Git, Run/Tasks, Debug |
| `flow.md` | Pläne, Agentenfluss, Karte als Plan, Grenzen, Kostenbuch |
| `modelle-rollen.md` | Modelle, Stufen, Standard-Engines, Rollen, die Umstellung (`agents-model.md`, AM7) |
| `kanban.md` | Kanban: Spalten, Vorschläge, Review |
| `chat.md` | der Chat in der Leiste, Slash-Befehle, was er darf |
| `spotlight.md` | ⌘K |
| `skills.md` | Skills, Skills Deck, gebündelte Skills |
| `routinen.md` | Routinen und ihr Zeitplan |
| `verbindungen.md` | Kalender, Mail, Erinnerungen (Digests) |
| `terminal.md` | Terminal |
| `glut.md` | Glut in Axiomata: Master-Pane, Karten-Läufe, Fragen |
| `kosten.md` | Kosten, Preise, Tagesgrenze |
| `einstellungen.md` | Einstellungen: Darstellung, Glas, Hintergrundbild, UI-Größe, Bewegung, Tastatur |
| `provider.md` | KI-Provider und Harness je Rolle |
| `storage.md` | Storage und Wartung |

## 4. Bau

- **CH1 Handbuch:** die Seiten (§3); eingebettet in den Build, beim Start nach `~/.axiomata/handbook/` geschrieben,
  wenn sie nicht zum Build passen (Hash), mit `README.md`; Regel in `AGENTS.md`; Test: jede Seite aus §3 vorhanden,
  jede mit Titel und Zusammenfassung, und jeder Eintrag des App-Rings / jede Ansicht / jeder Reiter der Einstellungen
  ist einer Seite zugeordnet (eine Tabelle im Code, die der Test gegen die echten Listen prüft).
- **CH2 Chat liest nach:** `APP_GUIDE` wird kurz (Verhaltensregeln + Inhaltsverzeichnis aus den Zusammenfassungen +
  wie man eine Seite liest: `axiomata-cli app handbook <name>`), Q4 als Regel; für den Chat auch unter Glut frei.
- **CH3 Lage:** die Befehle aus Q7, `--json` und lesbarer Text; Schlüssel nie; für den Chat freigeschaltet
  (`DIRECT_COMMANDS`, `skill_run_may` bleibt eng).
- **CH4 Selbst lesen:** ⌘K-Gruppe „Handbuch“, `?` in der Leiste → schwebendes Panel mit der Leseansicht (Links
  zwischen Seiten), `/help <Thema>`.
- **CH5 Bedienen:** Shell-Aktionen für die Ansichten und die Darstellung (direkt), Skill/Routine/Karte mit
  Rückfrage in der App (wie Berechtigungsfragen), die „nie“-Liste in der CLI-Sperre.
  - **Nachtrag Schließen** (Owner, 2026-10-29, im Geist von Q8: nichts darf verloren gehen): `open_views` listet,
    was offen ist (Kanban- und Datei-Panels, Terminal-Kacheln, Second Brain, Handbuch, Einstellungen) mit Id, Titel,
    Pfad und was das Schließen verlöre; `close_view` (`view` + `id`/`path`) schließt **direkt**, wenn nichts verloren
    geht, und **fragt** sonst in der Leiste der Rückfragen und sagt genau was: ungespeicherter Text einer Datei
    (`setLossProbe` neben dem Guard, `core/staging.ts`), ein getippter, nicht abgeschickter Kartentitel oder Verlaufs-
    eintrag oder ein offenes Schritt-Formular im Kanban, ein Programm in einem Terminal (`terminal_foreground`: die
    Vordergrund-Prozessgruppe des PTY ist nicht die Shell, oder ein Hintergrundjob hängt noch am Terminal — `ps -t`;
    nicht prüfbar = fragen). `run` prüft erneut, wenn der Owner nicht gefragt wurde (kein Rennen zwischen Prüfen und
    Schließen). Neue Marke `chat: "guarded"`
    (`registry.ts`). Geschlossen wird immer über den eigenen Weg des Panels: `requestClose` mit seinem Guard (eine
    Datei fragt nach dem Ja selbst nach Speichern/Verwerfen), das × der Kachel, das `close` des Fensters
    (`core/openViews.ts`); ein Kanban-Feld bekommt vorher sein Blur und speichert. Einstellungen verlieren nichts
    (die Komponente bleibt geladen). Das Studio, seine Panes und Agenten schließt der Chat nie.

Prüfungen: der volle Gate vor jedem Commit; `security-auditor` für CH3 und CH5 (neue Freigaben für den Chat, Lesen
von Einstellungen, Aktionen); ein kombinierter Review am Ende; Sicht im Browser-Mock.
