# Plan: Agentische IDE (M7)

Status: **M7.0 und M7.1 komplett, M7.2 begonnen (CP4 steht).**
Folgt dem schrittweisen Workflow des Owners: dieser Plan legt die Kette und die
bereits getroffenen Entscheidungen fest; **jeder Meilenstein wird einzeln
durchgeplant und bestätigt, bevor Code entsteht.** Was gebaut ist, steht bei den
Checkpoints in §5 — inklusive dessen, was beim Bauen anders kam.

Dies ist das bislang ambitionierteste Vorhaben in Axiomata — bewusst in sieben
Meilensteine geschnitten, von denen jeder für sich genommen einen benutzbaren
Zustand hinterlässt.

## 1. Idee

Eine agentische IDE als Teil von Axiomata-OS, die — wie schon das Terminal —
später mit vertretbarem Aufwand aus der App herausgelöst werden kann und
eigenständig läuft.

Kern des Konzepts:

- Eine eigene IDE-Ansicht, in der Terminal, Agenten-Fenster, Git-Diffs und
  Dateien nebeneinander liegen.
- Mehrere Agenten gleichzeitig: fremde Harnesses (Claude Code, Opencode) **und**
  ein eigenes Mini-Harness für einfache, aber sehr präzise erledigte Aufgaben.
- Die Agenten können miteinander reden und sich gegenseitig Arbeit zuteilen
  (A2A).
- Pro Agent ein eigener Git-Kontext, dessen Diffs man in Tabs am rechten
  Fensterrand einsieht, im Betrachter öffnet und bearbeitet.
- **Ein Plan-Tab pro Agent**, der jederzeit zeigt, was der Agent vorhat und wo
  er darin steht — so, wie Opencode es heute schon macht.
- **Ein Kanban-Brett als gemeinsame Aufgabenebene** für Mensch und Agenten;
  eigenständig nützlich und deshalb der erste Meilenstein.
- Projektverwaltung: die IDE sieht beim Öffnen eines Projekts wieder so aus, wie
  man sie verlassen hat.

## 2. Was schon existiert (und deshalb nicht neu gebaut wird)

| Baustein | Wo | Wiederverwendung in der IDE |
|---|---|---|
| PTY-/VT100-Engine | `crates/axiomata-terminal` | Trägt jedes Agent-Fenster |
| `PtySession::spawn(…)` | `crates/axiomata-terminal/src/pty.rs:76` | Startet `claude`/`opencode` im Worktree |
| Terminal-Frontend | `apps/dashboard/src/modules/terminal.svelte` + `TerminalScreen.ts` | Terminal- und Agent-Pane |
| Dateibetrachter/-editor | `shell/StagingPanel.svelte`, `core/staging.ts` | Diff → Datei öffnen und bearbeiten |
| Modul-Kontrakt | `core/types.ts` (`ModuleContext`) | Adapter, um Module ohne `Tile` zu mounten |
| Agenten-Harness + Sessions | `crates/axiomata-core/src/agents/` | Modellaufrufe, Session-Resume |
| Kosten-Metering | `crates/axiomata-core/src/spend.rs` | Zählt auch Agentenläufe der IDE |
| Provider pro Rolle | `AgentDefaults::provider_for(ProviderRole)` | Bekommt eine dritte Rolle `Agent` |
| Runs-DB + Migrationen | `crates/axiomata-core/src/db/` | Projekte, Agenten, Nachrichten |
| Theme-Tokens | `themes/tokens.css` | Gilt unverändert auch in der IDE |

**Nicht vorhanden und damit echte Neuarbeit:** jede Form von Git-Integration
(kein `git2`, kein `Command::new("git")` im ganzen Repo), ein Dock-/Split-/
Tab-Layout, Projektverwaltung, das Mini-Harness, das Postfach und der
MCP-Server.

## 3. Getroffene Entscheidungen (Owner, 2026-09-20)

- **E1 — Verortung: eigene Vollbild-Ansicht.** Nicht ein weiteres Canvas im
  bestehenden Shell, sondern eine eigene IDE-Ansicht (Vorbild `SecondBrainView`)
  mit **eigenem Dock-/Split-/Tab-Layout**. Begründung: eine IDE soll sich wie
  eine IDE anfühlen (andocken, maximieren, Tab-Leisten), nicht wie
  frei schwebende Dashboard-Kacheln; und der Schnitt für die spätere
  Herauslösung ist sauberer, weil die Ansicht ohnehin ihr eigenes Ding ist.
  Preis, bewusst akzeptiert: eigene Layout-Engine samt Persistenz, ein bis zwei
  Checkpoints Vorlauf, bevor der erste Agent läuft. Module werden über einen
  `ModuleContext`-Adapter wiederverwendet, nicht neu geschrieben.
- **E2 — Fremde Harnesses laufen als PTY-TUI.** Ein Agent-Fenster ist im Kern
  eine Terminal-Pane mit Startprofil (Befehl, Arbeitsverzeichnis, Env) und
  Statuszeile. Echtes CLI-Erlebnis inklusive Rückfragen, Modi und
  Berechtigungsdialogen. Kein Headless-Zweitpfad in v1.
- **E3 — A2A läuft über MCP, nicht über den Bildschirm.** Die PTY ist die
  *Sicht* auf einen Agenten, nicht der Integrationskanal. Axiomata betreibt
  einen eigenen MCP-Server mit den Postfach-Werkzeugen; Claude Code und Opencode
  sprechen beide MCP und adressieren sich damit **selbst** gegenseitig. Das
  Mini-Harness spricht dasselbe Postfach nativ.
  **Mechanik = Postfach (Adressen, `@name`, Aufgabenzuweisung), Darstellung =
  IDE, nicht n8n**: Inbox-Tab pro Agent-Fenster, schmales Team-Panel, optionale
  abschaltbare Bogen-Animation als reine Dekoration. Ausdrücklich **kein**
  Kabelziehen als Bedienkonzept.
- **E4 — Ein Git-Worktree pro Agent.** Jeder Agent arbeitet in einem eigenen
  `git worktree` auf eigenem Branch. Nur so sind die Diffs pro Agent überhaupt
  trennbar; Merge und Verwerfen werden dadurch ebenfalls pro Agent möglich.
- **E5 (Empfehlung, mit E1 hinfällig geworden): Zonen-Snapping statt Tiling** —
  entfällt, da das Dock-Layout aus E1 das Problem direkt löst. Freie
  Platzierung bleibt nur für schwebende Panels (Dateibetrachter) erhalten,
  analog zum heutigen `StagingPanel`.
- **E6 — Das Kanban kommt zuerst, als eigenständiges Modul.** Nicht als
  Unterfunktion der IDE, sondern davor und für sich: der Owner wollte ein Kanban
  ohnehin haben, es ist ohne einen einzigen Agenten nützlich, und es ist exakt
  die Datenschicht, auf der das Aufgabenbrett der Agenten später sitzt. Deshalb
  M7.0 (§5).
- **E7 — Jedes Agent-Fenster hat einen Plan-Tab.** Ausdrücklicher Owner-Wunsch,
  Vorbild ist Opencodes heutige Anzeige: jederzeit sichtbar, **welchen Plan der
  Agent verfolgt und wo er darin steht**. Gespeist aus derselben strukturierten
  Quelle wie der Status, nicht vom Bildschirm abgelesen. Zusammen mit dem
  Diff-Tab und später dem Inbox-Tab sitzt er in der Tab-Leiste am rechten
  Fensterrand (CP4, CP6b).
- **E8 — Eigenständig entwickeln, nicht nachbauen.** Der Owner hat ausdrücklich
  abgelehnt, ein bestehendes Werkzeug (amux) erst selbst zu benutzen: Ziel ist
  etwas Eigenes. Fremde Projekte werden daher als *Quelle für Entwurfsmuster*
  ausgewertet (§9) und nicht als Vorlage, an der sich unser Bedienkonzept
  auszurichten hätte.

Nicht mehr im Weg (Owner-Entscheidungen derselben Sitzung): **M4
(Always-on/Background-Scheduling) ist ersatzlos gestrichen**, Spotlight-Suche
und weitere Provider-Arbeit sind nach hinten gestellt.

## 4. Architektur-Skizze

### Crates

- **`axiomata-ide` (neu)** — die herauslösbare Substanz: Projektmodell,
  Worktree-Verwaltung, Git-Schicht, Agenten-Supervisor, Postfach-Kern,
  MCP-Server. **Hängt nicht von `axiomata-core` ab** — sonst ist die
  Herauslösung schon beim ersten Commit tot. Speicherung bleibt außen vor: die
  Crate nimmt Pfade und liefert Daten, persistiert wird auf der Core-/
  Dashboard-Seite. Genau der Schnitt, der `axiomata-terminal` herauslösbar hält.
- **`axiomata-miniagent` (neu, ab M7.4)** — der eigene Agent-Loop mit kleinem,
  präzisem Toolset. Bewusst getrennt von `axiomata-ide`, weil er auch außerhalb
  der IDE nützlich ist (perspektivisch für Skills/Routinen).
- **`axiomata-board` (neu, M7.0)** — Kanban-Kern: Spalten, Karten, Zuständige,
  Status-Gates, atomares Beanspruchen. **Eigene Crate, nicht in `axiomata-core`
  und nicht in `axiomata-ide`**, weil beide es brauchen: das Dashboard-Modul
  über Core, das Agenten-Brett über die IDE. Läge es in Core, müsste
  `axiomata-ide` von Core abhängen — und genau das darf es nicht.
- **`axiomata-core`** — bekommt die Persistenz (Migration für `boards`, `cards`,
  `projects`, `ide_agents`, `plan_steps`, `agent_messages`) und die dritte
  Provider-Rolle.
- **`apps/dashboard/src-tauri`** — dünne Befehlsschicht wie schon bei
  `terminal.rs`.

### Frontend (`apps/dashboard/src/ide/`)

- `IdeView.svelte` — die Vollbild-Ansicht.
- `layout.ts` — reines Dock-Baum-Modell, unit-testbar ohne DOM:
  `Split { dir: "row" | "col", children, sizes }` und
  `TabGroup { tabs, active }`, dazu Einfügen/Verschieben/Schließen/Größe,
  Serialisierung.
- `panes/` — `AgentPane`, `TerminalPane`, `DiffPane`, `FilePane`, `TeamPane`.
- `projects.ts`, `mailbox.ts`, `git.ts` — Frontend-Seite der jeweiligen
  Tauri-Befehle.
- `moduleAdapter.ts` — stellt einem bestehenden Modul den `ModuleContext`
  bereit, den es sonst von `Tile.svelte` bekäme.

### Datenmodell (Grobskizze; Board in M7.0 CP-K1, Rest in M7.1 CP0 festgezurrt)

```
Board    { id, name, columns: [{ id, name, order }] }
Card     { id, board_id, column_id, order, title, body,
           assignee: { kind: "human" | "agent", id } | null,
           status: "open" | "doing" | "done" | "verified",
           claimed_by, claimed_at, verified_by, created_at, updated_at }
Project  { id, name, repo_root, created_at, last_opened_at, layout_json }
Agent    { id, project_id, name, harness: "claude-code"|"opencode"|"mini",
           command, model, worktree_path, branch, port, status }
PlanStep { id, agent_id, order, text, state: "todo"|"doing"|"done",
           card_id | null }        // card_id = befördert aufs Board
Message  { id, project_id, from_agent, to_agent, kind: "msg"|"task"|"result",
           body, created_at, delivered_at, read_at }
```

`claimed_by` wird ausschließlich per Compare-and-Swap gesetzt („von `null` auf
mich, sonst Fehlschlag") — das ist die eine Zeile, an der hängt, dass zwei
Agenten nie dieselbe Karte greifen. `verified_by` muss von `claimed_by`
verschieden sein.

## 5. Meilensteinkette

### M7.0 — Kanban (eigenständig, **vor** der IDE) — **KOMPLETT** (2026-09-20)

**Eigener Detailplan: [`kanban.md`](kanban.md)** — dort stehen Datenmodell,
Checkpoints, Entscheidungen und was beim Bauen anders kam.

Gebaut und committet (`e70ca08`…`1e2ba0b`): die Crate `axiomata-board`, Migration
0008 samt WAL, `axiomata-cli board …`, das Dashboard-Modul mit Ziehen und
Tastaturumzug, Spaltenverwaltung, Modul-Actions und der Vault-Spiegel. Danach
CP-K4: die Fixrunden aus dem Live-Test am echten Mac gegen die echte Datenbank
und den echten Vault (`kanban.md` §6b) — darunter drei Korrekturen am geteilten
Schalenwerk, die jetzt für alle Module gelten.

⚠️ **Was M7.5 daraus erbt:** Der geteilte Store im Frontend kennt keinen
Aktualisierungspfad von außen. Er reicht, solange ausschließlich diese App das
Brett ändert. Sobald Agenten Karten greifen, ändert sich das Brett **ohne Zutun
des Benutzers**, und die Oberfläche merkt nichts davon — dann braucht es ein
Ereignis vom Backend oder einen Takt. Das ist der einzige bekannte Punkt, an dem
M7.0 für die Agenten noch nicht fertig ist.

Kurzfassung: das Brett ist für sich allein nützlich, ist die Datenschicht des
späteren Agenten-Bretts (M7.5) und wird deshalb zuerst gebaut. Der Kern kommt in
eine eigene Crate `axiomata-board`, die weder Verbindung noch Dateipfad besitzt,
sondern gegen ein hereingereichtes `&Connection` arbeitet — damit kann
`axiomata-ide` sie später nutzen, **ohne** von `axiomata-core` abzuhängen.
Quelle der Wahrheit ist die Datenbank, dazu ein einseitiger Markdown-Spiegel in
den Vault. Auch dieses Modul soll die App eines Tages verlassen können.

### M7.1 — Skelett: IDE-Ansicht und Projekte

Ziel: eine IDE-Ansicht, in der ein Terminal in einem echten Dock-Layout liegt,
und ein Projektwechsel, der das Layout wiederherstellt.

- **CP0** — `axiomata-ide` anlegen, Projektmodell, Persistenzentscheidung
  (DB-Migration vs. JSON) treffen und umsetzen, Projekte anlegen/auflisten/
  öffnen auf CLI-Ebene prüfbar machen.
- **CP1** — `ide/layout.ts` als reines Modell samt Tests: Baum, Split, Tabs,
  Einfügen/Verschieben/Schließen, Größenverteilung, Serialisierung. Kein DOM.
  **Erledigt.** Drei Dinge, die CP2 wissen muss: Andocken nimmt jeden Knoten als
  Ziel, auch die Wurzel — das ist das Andocken an den äußeren Rand; ein Tab, der
  als einziger seiner Gruppe auf eben diese Gruppe fällt, lässt das Layout
  *identisch* (dieselben Ids, nicht nur dieselbe Form), weil CP2 Zieh- und
  Übergangszustände an Knoten-Ids hängen wird; und `MIN_PANE_FRACTION` gilt nach
  jeder Operation, nicht nur nach dem Teiler-Zug, also auch für eine Vorgabe und
  für ein aus `layout_json` gelesenes Layout.
- **CP2** — `IdeView.svelte`: Baum rendern, Splitter ziehen, Tab-Leisten,
  Andocken per Drag (Kante = Split, Mitte = Tab). Erster echter Inhalt ist die
  Terminal-Pane über `moduleAdapter.ts`. **Erledigt.** Vier Entscheidungen, die
  M7.2 erbt:
  - **Panes werden nie ausgehängt, nur versteckt** — weder beim Tab-Wechsel,
    noch beim Verlassen der Ansicht, noch beim Umbauen des Layouts
    (`visibility` + `inert`, nicht `{#if}`). Für den dritten Fall reichte das
    ursprüngliche CP2-Design **nicht**: Svelte kann eine Komponente nicht
    zwischen zwei `{#each}`-Blöcken verschieben, also wurde bei jeder
    Struktur­änderung *jede* Pane zerstört und neu gebaut — drei Agenten, drei
    Neustarts, gemeldet vom Owner nach CP4. Seitdem liegen die Panes in einem
    flachen Speicher (`ide/paneStore.ts`) und werden nur per `appendChild` in
    die leeren Slots des Baums *verschoben*; geparkt wird in `$effect.pre`,
    verteilt in `$effect`. Dieselbe Regel gilt **innerhalb** der Agent-Pane:
    der Wechsel auf Plan/Diffs/Inbox versteckt das Terminal, hängt es aber
    nicht aus — ein Blick auf den Plan darf den Agenten nicht neu starten
    (Owner-Fund nach CP4).
    Das Terminal schließt seine PTY-Sitzung in `onDestroy`; ein Blick aufs
    Dashboard würde sonst jede laufende Shell töten. `visibility` statt
    `display: none`, weil eine versteckte Pane ihre gemessene Größe behalten
    muss — sonst meldet der `ResizeObserver` 0 Zeilen an die PTY. Für
    Agenten-Panes gilt das genauso, nur teurer.
  - **Escape schließt die IDE nicht.** Die Taste gehört dem, was in der Pane
    läuft (vim im Terminal). Der Weg hinaus ist der Knopf.
  - **Die Zieh-Geometrie wird einmal beim Dragstart gemessen** (`ide/dock.ts`,
    Schnappschuss-Muster wie `core/kanban.ts`), nicht bei jeder Bewegung.
  - **Geometrie ist testbar, nicht in der Komponente**: `dock.ts` rechnet
    Zeigerposition → Dock-Ziel (Kantenzone 25 %, Wurzelrand 3 %, Tab-Leiste hat
    Vorrang), die Svelte-Dateien messen nur Rechtecke und mounten Module.

  Bewusst offen geblieben: der Splitter lässt sich nur mit dem Zeiger bewegen,
  nicht mit der Tastatur (`role="separator"` ohne `tabindex`/Pfeiltasten), und
  die Tab-Leiste hat kein Roving-Tabindex. Beides ist ein Nachzügler für später,
  kein Teil von CP3.
- **CP3** — Projektwechsel: Projektliste, Repo-Ordner wählen, Layout pro Projekt
  laden und speichern. Damit ist „die IDE sieht aus wie verlassen" erfüllt.
  **Erledigt** — und damit **M7.1 komplett**. Drei Entscheidungen:
  - **Ohne Projekt keine Panes.** Die Ansicht startet leer und verlangt ein
    Projekt, statt ein Terminal zu öffnen, das nirgendwo hingehört.
  - **Ein Terminal in einer Pane startet im Projektordner.** `terminal.svelte`
    bevorzugt dafür ein vom Host geliefertes `cwd` aus `ctx.config` gegenüber
    der globalen Einstellung. Das ist kein Rückbau von Checkpoint 5d (alle
    *Einstellungen* raus aus `ctx.config`): `cwd` ist hier keine Einstellung,
    sondern die Tatsache, wo diese Pane lebt. Auf dem Canvas setzt es niemand,
    dort ändert sich nichts. M7.2 setzt an derselben Stelle den Worktree ein.
  - **Der Ordner wird als Text eingegeben**, wie der Workspace-Pfad in den
    Einstellungen — kein nativer Ordnerdialog, also keine neue Tauri-
    Abhängigkeit. Ein fehlender Ordner wird markiert, nicht entfernt, und der
    Pfad ist an Ort und Stelle korrigierbar (Projekt behält Id und Layout).

### M7.2 — Agent-Fenster samt Tab-Leiste

- **CP4** — Agent-Profile (Name, Harness, Befehl, Env, Modell) und `AgentPane` =
  Terminal-Pane mit Profil, Statuszeile, Neustart. Dazu die **Tab-Leiste am
  rechten Rand des Agent-Fensters** — einmal gebaut, später nur noch bewohnt:
  „Plan" jetzt (CP6b), „Diffs" ab M7.3, „Inbox" ab M7.5. **Erledigt.** Vier
  Entscheidungen:
  - **Das Harness wird in die Shell getippt, nicht direkt gestartet.** Die Pane
    spawnt eine Shell und schreibt den Befehl hinein. So darf ein Profil eine
    echte Kommandozeile tragen (Argumente, Pipes), ohne dass das Terminal-Modul
    parsen lernt — und wenn der Agent endet, steht seine Ausgabe noch da.
  - **Der Startbefehl ist ein Prop, kein `ctx.config`-Feld** (Security-Audit
    dieses Checkpoints). Eine Pane-Config wird aus dem gespeicherten
    `layout_json` zurückgelesen; ein Befehl darauf hieße „ein Projekt zu öffnen
    führt aus, was im Layout steht". Dieselbe Regel wie bei `cwd`
    (`ide/paneCwd.ts`), nur schärfer.
  - **Eindeutig pro Projekt, ohne Rücksicht auf Groß-/Kleinschreibung**
    (`UNIQUE … COLLATE NOCASE`): CP5 macht aus dem Namen ein Verzeichnis, und
    macOS-Dateisysteme unterscheiden nicht.
  - **`effective_command` kommt aus Rust mit** (berechnet beim Lesen, wie
    `root_exists` beim Projekt) statt einer zweiten Tabelle im Frontend.

  Status („arbeitet / wartet / fertig") steht bewusst noch nicht im Schema — der
  kommt mit CP6, als eigene Migration. **Noch offen aus F7:** Agenten überleben
  das Schließen der App nicht; die Pane besitzt die PTY-Sitzung.
- **CP5** — Worktree-Anbindung: beim Anlegen eines Agenten `git worktree add`
  unter `~/.axiomata/worktrees/<projekt>/<agent>`, Agent startet dort;
  sauberes Entfernen inklusive. Dazu, von amux übernommen (§9): **Identitäts-
  und Port-Env** für jeden Agenten — `AXIOMATA_AGENT_ID`,
  `AXIOMATA_AGENT_NAME`, `AXIOMATA_WORKTREE`, `AXIOMATA_BRANCH` und ein
  **reservierter Port pro Agent** (`AXIOMATA_PORT`), damit zwei Agenten, die
  beide `npx vite --port 1420` starten, sich nicht gegenseitig abschießen.
  Geht direkt über den vorhandenen `extra_env`-Parameter von
  `PtySession::spawn`, kostet also fast nichts. **Erledigt.** Vier
  Entscheidungen:
  - **F3 beantwortet: `git` als Unterprozess, nicht `git2`/libgit2.** CP5
    brauchte Worktrees, bevor M7.3 Diffs braucht, also fiel die Frage hier.
    Gründe: `git worktree` ist die Referenzimplementierung eines Features, das
    libgit2 nur teilweise modelliert; die Konfiguration des Nutzers
    (Credential-Helper, `includeIf`, Hooks) gilt beim Unterprozess umsonst —
    entscheidend, sobald ein Agent committet; und keine C-Abhängigkeit in einer
    Crate, die herauslösbar bleiben soll. Preis: `git` muss installiert sein,
    was `ensure_available` zu einem Satz statt zu einem Rätsel macht.
  - **Vorbereiten ist idempotent und passiert bei jedem Start**, nicht nur beim
    Anlegen: ein Agent von vor CP5 oder einer, dessen Verzeichnis jemand
    gelöscht hat, repariert sich dadurch selbst.
  - **Kein Repo ist kein Fehler.** Ist der Projektordner kein Git-Repository,
    teilen sich die Agenten ihn — wie vor CP5 —, bekommen aber trotzdem einen
    Port. Die Statuszeile sagt „shared folder", statt den Start zu verweigern.
  - **Die Identität steht am Ende der Env-Liste**, damit ein Profil sich nicht
    per `AXIOMATA_AGENT_ID=999` zu einem anderen Agenten erklären kann: der
    letzte Eintrag gewinnt, sowohl in `PtySession::spawn` als auch in
    `mergeEnv`.

  Der Worktree-Pfad trägt die Agent-Id (`<slug>-<id>`), weil ein Slug nicht
  eindeutig ist: „A B" und „A-B" wären sonst ein Verzeichnis und damit zwei
  Agenten, die sich gegenseitig überschreiben.
- **CP6** — Lebenszyklus-Ereignisse: pro Worktree eine
  `.claude/settings.local.json` mit Hooks, die `axiomata-cli agent-event …`
  aufrufen → echter Status (arbeitet / wartet auf Eingabe / fertig) statt
  Bildschirmraten. Für Opencode gibt es Hinweise auf eine strukturierte
  Schnittstelle (§9, F2). Der Status ist nicht nur Anzeige, sondern
  **Zustellbedingung** für das Postfach (CP13).
- **CP6b — der Plan-Tab.** Ausdrücklicher Owner-Wunsch, Vorbild ist das, was
  Opencode heute schon zeigt: ein Tab, der **jederzeit den aktuellen Plan des
  Agenten und seinen Stand darin** anzeigt — welche Schritte er sich vorgenommen
  hat, welcher gerade läuft, was erledigt ist. Nicht nachgebaut aus dem
  Bildschirminhalt, sondern aus derselben strukturierten Quelle wie CP6: bei
  Claude Code hängt an dessen Todo-Werkzeug ein Hook, der den Planstand an
  Axiomata meldet; bei Opencode über dessen Schnittstelle (F2); beim eigenen
  Mini-Harness ohnehin nativ, weil wir den Loop selbst schreiben.
  **Verbindung zum Kanban:** ein Planschritt kann zur Karte auf dem Brett
  befördert werden, und eine beanspruchte Karte erscheint umgekehrt im Plan des
  Agenten, der sie hält. Board = die gemeinsame, bleibende Ebene; Plan-Tab = was
  ein einzelner Agent gerade daraus macht.

### M7.3 — Git-Schicht

- **CP7** — Git-Engine in `axiomata-ide`: status, diff, branch, worktree
  add/list/remove, commit, merge. Entscheidung `git2` vs. `git`-Aufruf steht
  noch aus (§6).
- **CP8** — Diff-Tabs am Fensterrand pro Agent: geänderte Dateien, Diff-Ansicht.
- **CP9** — Diff → Datei im Betrachter öffnen und bearbeiten; Stage/Verwerfen/
  Commit pro Datei; Merge des Agenten-Branches in den Hauptbaum.

### M7.4 — Mini-Harness

- **CP10** — `ProviderRole::Agent` als dritte Rolle neben `Chat` und `Skill`,
  samt Einstellungsseite. Eigenes, günstiges, präzises Modell zuweisbar, ohne
  Chat oder Skills anzufassen.
- **CP11** — Agent-Loop in `axiomata-miniagent`: kleines Toolset (`read_file`,
  `write_file`, `list_dir`, `grep`, `apply_patch`, `run_command` mit
  Freigabeliste), strukturierte Ereignisse, Abbruch, Kostenmetering über
  `spend.rs`.
- **CP12** — Mini-Agent-Pane: eigene Oberfläche statt PTY — Nachrichten,
  Werkzeugaufrufe, Diff-Vorschau.

### M7.5 — A2A

- **CP12b** — Aktualisierungspfad fürs Brett (siehe M7.0): sobald Agenten Karten
  greifen, ändert sich das Brett ohne Zutun des Benutzers und die Oberfläche
  erfährt es nicht. Ereignis vom Backend oder Takt — zu entscheiden, wenn der
  erste Agent tatsächlich schreibt.
- **CP13** — Postfach-Kern in `axiomata-ide`: Adressen, Nachrichten, Aufgaben,
  Zustellung, Persistenz. Drei Eigenschaften sind dabei nicht verhandelbar,
  alle drei aus amux' Erfahrung übernommen (§9):
  **(a) Zustellung an Zugwechseln** — eine Nachricht erreicht einen TUI-Agenten
  erst, wenn er laut CP6-Status auf Eingabe wartet, nie mitten im Werkzeugaufruf.
  **(b) Atomares Beanspruchen** — eine Aufgabe wird per Compare-and-Swap
  übernommen, damit zwei Agenten nie dieselbe Karte greifen.
  **(c) `fertig` ≠ `geprüft`** — ein Agent kann eine Aufgabe auf „fertig"
  setzen, „geprüft" setzt nur ein *anderer*. Verhindert, dass sich ein Agent
  selbst abnickt.
- **CP14** — MCP-Server (stdio) mit `list_agents`, `send_message`, `read_inbox`,
  `claim_task`, `report_done`; Eintragung in die Konfiguration der TUI-Agenten
  pro Worktree — **sichtbar und bestätigt, nie heimlich** (§7).
  **Absenderidentität stempelt der Server, nicht der Agent** (§9): der
  MCP-Serverprozess wird je Agent mit dessen Identitäts-Env aus CP5 gestartet
  und leitet den Absender daraus ab. Ein `from`-Feld aus dem Werkzeugaufruf
  wird ignoriert — ein verwirrtes oder böswilliges Modell soll sich nicht als
  ein anderer Agent ausgeben können.
- **CP15** — Oberfläche: Inbox-Tab pro Agent, Team-Panel (wer arbeitet woran,
  wer darf wem schreiben), optionale Bogen-Animation.

### M7.6 — Herauslösung

- **CP16** — Crate-Grenzen prüfen und schärfen, eigener Build-Pfad/Bin, eigene
  Konfigurationswurzel, Dokumentation. Prüfkriterium: `axiomata-ide` und
  `axiomata-miniagent` bauen ohne `axiomata-core` im Abhängigkeitsbaum.

## 6. Offene Fragen (jeweils vor dem eigenen Meilenstein zu klären)

| # | Frage | Fällig |
|---|---|---|
| F1 | Projekte in der SQLite-DB (Migration) oder als JSON wie `dashboard.json`? | M7.1 CP0 |
| F2 | Welche strukturierte Schnittstelle bietet Opencode? (Für Status *und* Plan-Tab.) | M7.2 CP6 |
| ~~F3~~ | ~~`git2` oder Unterprozess?~~ **Beantwortet (CP5):** Unterprozess, Gründe in `worktree.rs`. | erledigt |
| F4 | Genaue Config-Orte für die MCP-Eintragung pro Harness (Projekt- vs. Benutzerebene). | M7.5 CP14 |
| F5 | Wie viele Agenten passen auf 21:9 sinnvoll nebeneinander — braucht es Layout-Vorlagen? | M7.2 CP4 |
| F6 | Übernimmt das Mini-Harness perspektivisch auch Skills/Routinen, oder bleibt es IDE-intern? | nach M7.4 |
| F7 | Sollen Agenten das Schließen der App überleben? Unsere PTY sagt nein, tmux würde ja sagen. | M7.2 CP4 |
| ~~F8~~ | ~~Kanban-Speicher?~~ **Beantwortet:** Datenbank als Quelle, Markdown-Spiegel in den Vault. | erledigt |

## 7. Risiken

- **Sicherheit.** Das Mini-Harness führt Befehle aus und schreibt Dateien; die
  MCP-Werkzeuge nehmen Eingaben von Agenten entgegen. `security-auditor` ist für
  CP11 und CP14 verpflichtend, nicht optional. Freigabeliste statt Blockliste
  für `run_command`; Pfade strikt auf Worktree-Wurzel begrenzen (derselbe
  Fehlerfall wie `prepend_files` mit `..`).
- **Fremde Konfigurationsdateien.** Das Eintragen des MCP-Servers verändert
  Dateien, die dem Nutzer gehören (`~/.config/opencode/opencode.json`,
  `.claude/settings.local.json`). Immer sichtbar machen, bestätigen lassen und
  rückgängig machbar halten.
- **Kosten.** Mehrere Agenten gleichzeitig vervielfachen die Tokenkosten. Das
  Tages-Cap aus dem Provider-Hardening muss auch hier greifen — und pro Projekt
  sichtbar sein, nicht nur global.
- **Scope.** Sieben Meilensteine, rund zwanzig Checkpoints. Der Plan ist so
  geschnitten, dass schon M7.0 ein für sich nutzbares Kanban liefert, M7.1 eine
  Projekt-IDE mit Terminal, und jeder weitere Meilenstein eigenständigen
  Mehrwert bringt. Kein Meilenstein beginnt, bevor der vorherige live bestätigt
  ist.

## 8. Verifikation (pro Checkpoint anwendbar)

- `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`.
- `cd apps/dashboard && npm run check && npx vitest run`.
- Die reinen Modelle (Board-Kern, `ide/layout.ts`, Postfach-Kern, Git-Engine)
  sind von ihrem jeweils ersten Checkpoint an eigenständig testbar — dieselbe
  Reihenfolge, die sich beim Terminal bewährt hat (Engine zuerst, Oberfläche
  danach). Für das atomare Beanspruchen gehört ein Nebenläufigkeitstest dazu:
  zwei gleichzeitige Ansprüche auf dieselbe Karte, genau einer gewinnt.
- Gebündelte Sub-Agent-Läufe pro Checkpoint und immer vor einem Commit, gemäß
  der projektlokalen Kadenz-Regel in `CLAUDE.md`.
- Für alles, was mit dem *Gefühl* zu tun hat (Andocken, Tab-Ziehen, mehrere
  Agenten nebeneinander), bleibt der Live-Test am echten Mac unverzichtbar. Die
  gesamte Terminal-Bugfixkette CP5d–CP6d entstand aus genau solchen Tests, nicht
  aus automatisierter Verifikation.

## 9. Verwandte Projekte — was wir übernehmen (Recherche 2026-09-20)

Es existieren **drei verschiedene Projekte namens „amux"**, alle im selben
Problemraum:

- [`andyrewlee/amux`](https://github.com/andyrewlee/amux) — Go, TUI, tmux +
  git-Worktrees. Workspace-first: ein Workspace = ein Worktree auf eigenem
  Branch unter `~/.amux/workspaces/<projekt>/<workspace>`. MIT.
- [`choplin/amux`](https://github.com/choplin/amux) — „Agent Multiplexer",
  Worktree-Workspaces, **bietet sich selbst als MCP-Server an**
  (`amux mcp --git-root …`), damit Agenten Workspaces selbst verwalten können.
- [`mixpeek/amux`](https://github.com/mixpeek/amux) — der mit Abstand nächste
  Verwandte: **Rust**, vier Crates, Axum-Server mit SQLite, parallele Claude-
  Code-/Codex-/Gemini-Worker, geteiltes Board, Nachrichten zwischen Agenten,
  Web-Dashboard plus iOS-App. MIT **+ Commons Clause** — lesen und daraus lernen
  ist unproblematisch, Code übernehmen wollen wir ohnehin nicht.

**Was wir übernehmen (Ideen, kein Code):**

| Muster | Warum | Wo im Plan |
|---|---|---|
| Worktree pro Agent | Alle drei machen es so — E4 ist damit bestätigt | E4, CP5 |
| Identitäts-Env im Agentenprozess | Macht den Agenten für Werkzeuge identifizierbar | CP5 |
| Port-Reservierung pro Worktree | Zwei Dev-Server auf 1420 killen sich sonst | CP5 |
| Absender stempelt der Server | Ein Agent darf sich nicht als anderer ausgeben | CP14 |
| Zustellung an Zugwechseln | Mitten im Werkzeugaufruf kommt nichts an | CP13 |
| Atomares Beanspruchen von Aufgaben | Sonst greifen zwei Agenten dieselbe Karte | CP13 |
| `fertig` ≠ `geprüft` | Kein Agent nickt seine eigene Arbeit ab | CP13 |
| Ebenen global → Gruppe → Agent für Instruktionen/Env | Passt zu Memory-Router und Rollen-Providern | später |
| Commit/Merge aus der UI, **nie** push; `--no-ff`; Git-Hooks aus | Sichere Vorgaben, wo Agenten schreiben | CP9 |
| Ins Terminal des Kollegen schauen, bevor man ihn unterbricht | Geschenkt, wir rendern die PTY ohnehin | CP15 |
| Nachrichten-Ledger als Datenbankzeilen | Nachvollziehbarkeit und Kostenzuordnung | CP13 |

**Was wir bewusst nicht übernehmen:**

- **Daemon mit HTTP-API** (mixpeek: Axum auf Port 8824, launchd/systemd-
  Autostart, Bearer-Token, Tunnel nach außen). Das ist genau die Always-on-Form,
  die der Owner mit M4 gerade gestrichen hat. Wir bleiben in-process. Falls der
  geparkte LAN-Server-Modus je wiederkommt, ist das hier allerdings die
  Blaupause.
- **tmux als Prozessträger.** Wir haben eine eigene PTY-Engine; eine
  Fremdabhängigkeit dafür wäre ein Rückschritt. Preis: Agenten überleben das
  Schließen der App nicht (→ F7).
- Berechnete `.mdai`-Dateiketten, iOS-App, macOS-Automatisierungsrechte — außerhalb
  des Zuschnitts.

**Vorschlag verworfen (Owner, 2026-09-20):** die Empfehlung, eines der amux'
vorab selbst zu benutzen, wurde ausdrücklich abgelehnt — es soll etwas
Eigenständiges entstehen (E8). Diese Recherche bleibt damit, was sie ist: eine
Quelle für Entwurfsmuster, aus der die Tabelle oben gezogen wurde. Kein
Bedienkonzept, an dem sich unseres messen müsste, und keine weitere
Konkurrenzanalyse ohne Anlass.

**Warum M7 trotzdem eigenständig gerechtfertigt ist:** Axiomata ist ein natives
Desktop-Programm statt Daemon plus Browser-Dashboard; die IDE soll mit Second
Brain, Dateibetrachter, Skills und Routinen im selben Fenster zusammenspielen;
und das Projekt ist ausdrücklich auch ein Lernvorhaben, in dem Selberbauen der
Zweck ist und nicht der Umweg. Was wir uns sparen, sind die Fehler, die andere
schon gemacht haben — dafür steht die Tabelle oben, mehr soll sie nicht sein.
