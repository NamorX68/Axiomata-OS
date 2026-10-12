# Karte als Plan — eine Board-Karte wird zum Ziel eines Plans

Stand: 2026-10-12, entworfen, K1–K4 vom Owner entschieden; Checkpoint 0 (Bestätigung) offen. Gearbeitet wird im
Worktree `card-as-plan`. Baut auf A2A auf (`docs/plans/a2a.md`, A3/A4, A14, CP-A6a, CP-A7, CP-A8a).

## Context

Der Owner will die Board-Karten (Board #1, 16 im Backlog: #75 Tags/Wikilinks, #78 Chat-Ausbau …) abarbeiten lassen
wie wir hier arbeiten: planen, grillen, Schritte, Abnahme. Heute geht „Start …“ auf einer Karte nur als *eine*
Agentensitzung; einen Plan aus einer bestehenden Karte zu machen braucht vier Handgriffe im Flow (neuer Plan, Karte
zuordnen, freigeben, übernehmen), und die Karte weiß nichts vom Stand ihres Plans (sie bleibt nach der Integration
in Done liegen, Pläne haben keinen gespeicherten „läuft/fertig“-Zustand).

Entscheidungen (Owner, 2026-10-12, AskUserQuestion):
- **K1 Karte = Planziel.** Start legt einen Plan an, dessen Ziel die Karte ist; der Planer zerlegt sie in Schritte
  (eine kleine Karte darf ein einziger Schritt bleiben).
- **K2 Schritte nur im Flow.** Auf dem Board steht nur die Ursprungskarte und wandert nach rechts; die Unterkarten
  sieht man im Studio-Flow und als Fortschritt auf der Karte („3/5“).
- **K3 Review = Abnahme des Owners.** Die Karte steht in Review, sobald alle Schritte geprüft und integriert sind und
  der Plan auf „Take over“ wartet; nach der Übernahme nach Done.
- **K4 Erst grillen, dann loslegen.** Start legt den Plan (Entwurf) an und startet den Planer, der Rückfragen stellt
  und Schritte vorschlägt; die Karte steht dabei schon in In Progress. Nach der Freigabe läuft der Plan von selbst.

## Was schon da ist (wiederverwenden)

- Plan anlegen: `flow::create_plan` (`crates/axiomata-board/src/flow.rs:425`), Tauri `create_board_plan`
  (`src-tauri/src/commands.rs:1686`).
- Planer: `plan_session::start_plan_session` (`crates/axiomata-core/src/plan_session.rs:166`), Vorschläge per MCP
  `create_card` in die Proposal-Spalte; Freigabe `approve_board_plan` (`commands.rs:1748`, `run_by_itself` →
  `auto_start_max = AUTO_UNBOUNDED`).
- Selbstlauf: `PlanRun::tick` (`crates/axiomata-core/src/plan_run.rs:346`, alle 5 s aus `src-tauri/src/plan_watch.rs`)
  startet, prüft (Reviewer via `card_watch.rs`), integriert in die Plan-Linie, meldet `ReadyToTakeOver`.
- Übernahme: `card_session::take_over_plan` (`card_session.rs:1876`), schließt den Plan, archiviert die Karten.
- Spalten: Status liegt in der Spalte (`maps_to_status`), Rollen `proposal`/`review`; In Progress = erste schlichte
  doing-Spalte (`column_with(columns, Doing, None)`, `flow.rs:544`); Zustand abgeleitet in `flow::derive_state`
  (`flow.rs:75`). Owner-Bewegung `store::move_card_as(..., "human:owner")` (`store.rs:1079`).
- UI: Kanban-Seitenpanel `modules/kanban.svelte` (Start-Knopf → `modules/CardStartForm.svelte`, Sichtbarkeit
  `ide/cardStart.ts`), Flow `ide/panes/PlanPane.svelte` + `ide/planning.ts`, Studio öffnen per
  `ide/projectRequest.ts` / `ide/modeRequest.ts`.

## Entwurf

**Datenmodell.** Neue Spalte `plans.origin_card_id` (nullable, unique; Migration in `crates/axiomata-board`). Die
Ursprungskarte bekommt **kein** `plan_id` — sonst würde `PlanRun` sie selbst starten. Unterkarten tragen wie heute
`plan_id`. Ein Plan mit Ursprungskarte läuft immer von selbst (Freigabe setzt `run_by_itself`).

**Start (`flow::start_card_as_plan`, neu, Owner only).** Prüft: Karte offen und bereit, ohne `plan_id`, ohne Claim,
nicht selbst Ursprung eines offenen Plans, Projekt ist ein Git-Repo. Legt in einer Transaktion den Plan an (Name
`#<id> <Titel>`, Ziel = Titel + Beschreibung + Akzeptanzkriterien der Karte, Projekt, Board der Karte,
`origin_card_id`) und schiebt die Karte nach In Progress (Ereignis `planned`). Danach startet der Aufrufer den Planer.
Tauri `start_card_plan(card_id, project_id)`, CLI `board plan from-card <card> --project <p>`.

**Die Karte folgt dem Plan (`flow::sync_origin_card`, neu, idempotent).** Ein Zielort aus dem Planzustand:
- Entwurf oder freigegeben und nicht fertig → In Progress;
- alle Unterkarten integriert oder abgesagt (mind. eine integriert, = `ReadyToTakeOver`) → Review-Spalte;
- Plan übernommen → Done; Plan gelöscht oder ohne Übernahme geschlossen → zurück oben in Open.
Aufgerufen aus `PlanRun::tick` (je Plan mit Ursprung), `take_over_plan`, Plan löschen/schließen, und nach „Karte zum
laufenden Plan hinzufügen“ (eine neue Unterkarte holt die Karte aus Review zurück). Bewegt als `human:owner`-gleicher
Systemschritt, nie als Agent.

**Falle Review-Spalte:** die Ursprungskarte steht in der Review-Rolle, ist aber keine Agentenarbeit. `card_watch.rs`
(`cards_awaiting_review` / `start_review_session`) darf sie nie aufgreifen — ausdrücklich ausschließen
(Ursprungskarten haben keinen Worker-Claim; trotzdem eigener Test). `derive_state` liefert für sie einen eigenen
Zustand `Planned` (mit Planfortschritt), damit Kanban-Knöpfe und Heartbeat sie nicht als „in review“ behandeln.

**Kanban (Frontend).**
- „Start …“ auf einer bereiten Karte ohne Plan öffnet einen schlanken Start-Dialog (Projekt wählen; Planer-Engine wie
  im Flow), ruft `start_card_plan`, startet den Planer und öffnet das Studio im Flow-Modus auf diesem Plan — dort
  wird gegrillt und freigegeben („Automatisch abarbeiten“ fest an).
- Unterkarten von Plänen mit Ursprungskarte werden auf dem Board ausgeblendet; die Ursprungskarte zeigt Planname,
  Fortschritt „integriert/gesamt“, Zustand (plant · läuft · wartet auf dich) mit Glut nur, wenn eine Sitzung läuft,
  und die Knöpfe „Im Flow öffnen“ und — in Review — „Take over plan“.
- Nebenbei behoben: `canTakeOver` zeigt „Take over …“ nicht mehr für verifizierte Karten eines selbstlaufenden Plans
  (das Backend lehnt sie ab).
- Board-Spiegel (`core/board_mirror.rs`): Unterkarten nicht als eigene Zeilen, Ursprungskarte mit Fortschritt.

**Flow.** PlanPane zeigt bei einem Plan mit Ursprungskarte deren Titel/Link; „Release plan“ setzt dort immer
„Automatisch abarbeiten“. Sonst unverändert.

**Außerhalb:** bestehende Pläne ohne Ursprungskarte bleiben wie sie sind; der Einzelsitzungs-Start bleibt für
Unterkarten im Flow. Vorgemerkt, nicht Teil: `update_card` erlaubt heute, das `plan_id` einer Karte mit laufender
Sitzung zu ändern (Linien-Falle) — eigene Karte.

## Checkpoints

0. Plan nach `docs/plans/card-as-plan.md` (K1–K4 + dieser Entwurf), Owner bestätigt.
1. **Backend:** Migration `origin_card_id`, `start_card_as_plan`, `sync_origin_card` + Aufrufstellen, Zustand `Planned`,
   Ausschluss im Review-Watcher, Tauri-Befehl, CLI, Board-Spiegel; Rust-Tests je Übergang (Start, Freigabe, fertig →
   Review, neue Unterkarte → zurück, Übernahme → Done, Löschen → Open, Reviewer greift nicht).
2. **Frontend:** Start-Dialog + Studio-Sprung, Ausblenden der Unterkarten, Ursprungskarte mit Fortschritt/Knöpfen,
   `canTakeOver`-Korrektur, Flow-Hinweis; Mock-Fixture (eine Karte mit Plan) + Vitest für die reine Logik.
3. Review (ein Sonnet-Review; `security-auditor`, weil ein neuer Tauri-Befehl dazukommt), Commit, App installieren,
   Mac-Test mit einer echten kleinen Karte (z. B. #83), danach architecture.md §7.

**Stand CP1 (2026-10-12, gebaut, nicht committet):** Migration 30 `plan_origin` (Seitentabelle wie `plan_grilled`,
`card_id` nicht unique — nach einem geschlossenen Plan darf die Karte wieder starten). `Plan.origin_card_id`,
`Card.origin_of` (offener Plan, beim Lesen berechnet), `TaskState::Planned` (vor der Spalte abgeleitet, also nie
`in_review` → der Review-Watcher greift die Karte nie auf). `flow::start_card_as_plan` (Owner, ein Transaktionsschritt,
Plan mit `auto_start_max = AUTO_UNBOUNDED`, jetzt in der Board-Crate statt in `commands.rs`), `flow::sync_origin_card(s)`
(idempotent; aus `PlanRun::tick` am Ende jedes Blicks, schreibt den Spiegel), `close_plan` setzt die Karte nach Done
(eine Unterkarte wurde übernommen) oder oben nach Open, `delete_plan` eines offenen Plans nach Open. Die Fertig-Regel
ist jetzt eine Funktion (`flow::plan_settled`) für Plan-Lauf und Karte. Schutz: die Karte wird kein Schritt eines Plans
(`update_card`), ein Karten-Plan verliert den Selbstlauf nicht (`update_plan`), die Freigabe in der App setzt ihn immer.
Core `card_session::start_card_plan` (Projekt + Git-Repo), Tauri `start_card_plan`, CLI `board plan from-card <card>
--project <p>`, `board list` zeigt `state:planned goal-of:#<plan>`. Board-Spiegel: Schritte eines Karten-Plans fehlen,
die Karte trägt „plan #N, k/n steps done“ (`plan_progress`). Frontend nur Typen (`planned`, `origin_of`,
`origin_card_id`). Tests: Board 132, Core +1, Gate grün; CLI-Durchlauf auf Scratch-Home (Start, doppelt abgelehnt, kein
Repo abgelehnt, Freigabe, Schließen → Open, neu starten, Löschen → Open) wie erwartet.

**Stand CP2 (2026-10-12, gebaut, nicht committet):** `ide/cardPlan.ts` (rein, getestet): Pläne mit Ursprungskarte,
sichtbare Karten (Schritte offener Karten-Pläne fehlen), Plan der Karte, Fortschritt „k/n“ (wie der Spiegel), Phase
planning/running/waiting (`readyToTakeOver`), Glut bei Sitzung an einem Schritt, `canStartAsPlan`. Kanban: „Start …“
auf einer bereiten Karte ohne Plan → `CardPlanStartForm.svelte` (nur Projekt) → `start_card_plan` → „Danach grillen“
gesetzt → `showPlanInFlow` (`ide/planRequest.ts`: Plan-, Projekt-, Modus-Wunsch + `shell:studio`), das Kanban-Panel
schließt sich. Karten in einem Plan behalten den Einzelstart. Die Ursprungskarte zeigt „plan · <Phase> · k/n“, glimmt,
trägt die Projektkante ihres Plans; im Seitenpanel Zeile „Plan“, Knopf „Open in the Flow“ bzw. „Take over in the Flow …“
(primär, sobald alles bereit ist), kein „Absagen“, Plan-Auswahl gesperrt. `PlanPane` nimmt den Plan-Wunsch an (Board,
Plan) und startet den Planer selbst, wenn dessen Rolle eine Engine hat (sonst Hinweis); Hinweis „The goal of card #N“;
bei Karten-Plänen kein Schalter „Automatisch abarbeiten“ (immer an). `canTakeOver` verweigert verifizierte Schritte
selbstlaufender Pläne. Nebenbefund behoben: ein Projekt-Wunsch für das schon offene Projekt öffnete es neu und setzte den
Modus zurück (betraf auch Spotlight „Projekt + Modus“); ein Modus-Wunsch wartet jetzt auf ein laufendes Öffnen. Mock:
Karte #11 mit Plan #2 (Schritte #12 integriert, #13 in Arbeit mit Sitzung), Handler `start_card_plan`. Abweichung vom
Entwurf: „Take over plan“ sitzt nicht auf der Karte, der Knopf öffnet den Flow, wo die Übernahme (Konflikte, Push-Hinweis)
schon lebt.

## Verifikation

- `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace`;
  `cd apps/axiomata && npm run check && npx vitest run`.
- CLI-Durchlauf auf einem Scratch-`AXIOMATA_HOME`: Karte anlegen → `board plan from-card` → Karte in In Progress →
  Planvorschlag, `board plan approve` mit Selbstlauf → nach Integration steht die Karte in Review → `board
  take-over-plan` → Karte in Done; `board list --board` nach jedem Schritt.
- Mock (`vite --port 1431`, danach beenden): Board zeigt nur die Ursprungskarte mit Fortschritt, Unterkarten nur im Flow.
- Mac: echte kleine Karte einmal ganz durch (grillen, freigeben, automatisch, Review, Take over).
