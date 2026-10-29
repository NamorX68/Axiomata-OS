# Wartung — Axiomata hält seine eigene Ablage sauber

Stand: 2026-10-29, **gebaut, am Mac abgenommen und gemergt** (MT1–MT4, 2026-10-29). Gegrillt (A1–A5, Owner 2026-10-28), Plan abgenommen 2026-10-29 (M1: keine Build-Ordner in „Storage“). Anlass: die Platte war voll
(Build-Ordner, alte Worktrees, Harness-Reste); aufgeräumt wurde von Hand. Damit sich Axiomatas eigene Daten nicht
wieder ansammeln, räumt die App sie selbst auf. Glut bekommt sein eigenes Gegenstück (`glut clean`, im Glut-Repo, A5).

## 1. Grundsatz

Die Wartung hält **nur Axiomatas eigenes System** sauber (A1): `~/.axiomata` und die Worktrees, die Axiomata selbst
für seine Agenten anlegt (`~/Worktrees/axiomata-agents`). **Nie** Entwicklungsprojekte, ihre Build-Ordner, den
Vault, `~/.glut` (A5: Glut räumt selbst auf) oder Inhalte des Owners (`wallpaper/`, `theme.css`, Skills).

## 2. Entscheidungen (Owner, 2026-10-28)

- **A1 Klar getrennt:** Axiomata-Funktionalität, nicht Dev-Pflege. Build-Ordner sind Sache der Projekte.
- **A1b Umfang** wie in §3; die beiden Altlasten (`~/.axiomata/opencode`, `~/.axiomata/prototypes`) sind von Hand
  weg (Owner, 2026-10-28), der Prototyp liegt jetzt in `prototypes/hexball/`.
- **A2 Eingebaut, ohne Agent:** einmal pro Woche selbsttätig beim Start, dazu ein Bereich **„Storage“** in den
  Einstellungen (Größen, letzter Lauf, „Clean up now“) und `axiomata-cli maintenance status|clean [--dry-run]`.
  Kein Skill, keine Routine — Aufräumen kostet nie Geld.
- **A3b Selbsttätig** für Datenbank, Logs, Sicherungen, Ereignisordner und Claude-Sitzungen.
- **A3c Worktrees:** werden **beim Löschen des Agenten** mit entfernt; hat einer ungesicherte Arbeit, fragt
  Axiomata dabei. Als Netz für Altfälle: ein verwaister Worktree ohne Änderungen geht selbsttätig, einer mit
  Änderungen wartet in „Storage“ auf den Klick des Owners.
- **A4 Fristen** wie in §3, in „Storage“ änderbar.
- **A5 Glut** räumt `~/.glut` selbst (`glut clean`); Axiomata zeigt nur Gluts Platzbedarf an (§5).

## 3. Was wie lange bleibt (A4)

| Posten | Ort | bleibt | heute |
|---|---|---|---|
| Läufe | `runs` | 90 Tage, mindestens die letzten 200 | schon gekappt auf 2000 (`runlog::prune_runs`), bleibt |
| Routinen-Feuer | `routine_runs` | 30 Tage | schon gekappt auf 500 je Routine, bleibt |
| Chat-Züge | `chat_turns` | 90 Tage | wächst |
| Websuchen | `web_searches` | 90 Tage | wächst |
| Ereignisse archivierter Karten | `card_events` | 90 Tage nach dem Archivieren | wächst; die Geschichte *lebender* Karten bleibt unangetastet (A19) |
| Kostenverlauf | `session_spend` | 1 Jahr | wächst |
| App-Log | `logs/app.log.<datum>` | 14 Tage | rotiert täglich, 7 Dateien (`APP_LOG_MAX_FILES`) → 14 |
| Lauf-Log | `logs/runs.log` | rotiert ab 10 MB, die letzten 2 bleiben | wächst (15 MB) |
| Sicherungen | `*.backup-*` in `~/.axiomata` | die neuesten 2 je Datei | von Hand entstanden, liegen |
| Ereignisordner | `agent-events/<agent>` ohne Agent | sofort | gehen heute schon mit dem Agenten |
| Claude-Sitzungen | `claude/projects`, `claude/runs`, `claude/shell-snapshots`, `claude/session-env` | 30 Tage | wachsen; **nie** der ganze Ordner (Login!) |
| Worktrees | `~/Worktrees/axiomata-agents` ohne Agent | A3c | bleiben heute liegen |

Nach dem Löschen von Zeilen: `PRAGMA wal_checkpoint(TRUNCATE)` und `VACUUM`, nur wenn mehr als 10 % der Seiten frei
sind (die DB ist klein, 16 MB; ein VACUUM sperrt sie kurz).

Bewusst **nicht** dabei: `harness/runs` (die Durable-Journale räumt `durable::on_start` schon selbst, D8),
`editor-recovery/` (räumt der Editor beim Start selbst, älter als 30 Tage: `editor_recovery::MAX_AGE_DAYS`), `wallpaper/`, `dashboard.json` und die übrigen kleinen
Zustandsdateien.

## 4. Bau

Ein Modul `axiomata_core::maintenance` mit zwei Teilen, beide rein über Pfade aus `paths.rs` und die DB:

- **`report(core) -> Report`** — je Posten Bytes, Anzahl, was frei würde, die Frist; dazu der letzte Lauf
  (`app_meta`, Schlüssel `maintenance.last_run`) und die verwaisten Worktrees (mit „hat Änderungen“).
- **`clean(core, dry_run) -> Report`** — dasselbe, nur löscht es. Jeder Posten für sich, ein Fehler bei einem
  bricht die anderen nicht ab (best effort, im App-Log). Dateien werden nur gelöscht, wenn sie **innerhalb** ihres
  Ordners liegen und keine Links sind (`symlink_metadata`, kein Folgen) — dieselbe Vorsicht wie beim Hintergrundbild.

Einstellungen in `config.toml` unter `[maintenance]` (`Config::maintenance`, alle Felder mit den Werten aus §3 als
Vorgabe; `enabled` schaltet nur den Wochenlauf ab, nie den Knopf).

### Checkpoints

- **MT1 Kern:** `maintenance::{report, clean}` für DB-Zeilen, Logs, Sicherungen, Ereignisordner und
  Claude-Sitzungen; `[maintenance]` in der Config; `app.log` auf 14 Dateien; `runs.log` rotiert ab 10 MB
  (`runlog::append_jsonl`, `runs.log.1`, `runs.log.2`). Tests gegen ein Scratch-`AXIOMATA_HOME` mit gealterten
  Dateien und Zeilen.
- **MT2 Worktrees (A3c):** `ide agents delete` und `delete_ide_agent` entfernen Worktree und Branch mit
  (`worktree::remove(…, force = false)`, `worktree::delete_branch`). Hat der Worktree Änderungen: die CLI verweigert
  ohne `--force`, die App fragt im Löschdialog („Unsaved work in the worktree — delete anyway?“). Dazu das Netz:
  `report` findet Worktrees unter `~/Worktrees/axiomata-agents` ohne Agenten; `clean` entfernt die sauberen.
- **MT3 Ablauf:** der Wochenlauf beim Start (nach `spend::reconcile_recorded_costs` in der Bootstrap, im Hintergrund,
  nie den Start aufhaltend) und `axiomata-cli maintenance status|clean [--dry-run] [--json]`.
- **MT4 „Storage“:** ein Abschnitt in den Einstellungen: Gesamtgröße von `~/.axiomata`, die Posten mit Größe und
  Frist (Fristen änderbar), letzter Lauf, „Clean up now“ (zeigt vorher, was frei wird), die verwaisten Worktrees mit
  Änderungen je mit „Delete“ (fragt nach). Dazu eine Zeile **Glut** aus `glut clean --dry-run --json` (§5), wenn
  `glut` es schon kann; sonst fehlt die Zeile still.

Prüfungen: der volle Gate vor jedem Commit; `security-auditor` vor dem Commit von MT1/MT2 (löscht Dateien, neue
Tauri-Befehle), ein kombinierter Review vor dem Abschluss.

## 5. Glut (A5)

Glut baut `glut clean` selbst (Auftrag an die Glut-Session, 2026-10-29). Vorläufiges Format, **version 1**:
`glut clean --dry-run --json` schreibt ein JSON-Objekt (`version`, `dryRun`, `totalBytes`, `freeableBytes`,
`lastRun`, `items[]` mit `name`, `bytes`, `freeableBytes`, `count`, `freeableCount`, `keep`); Fehler: Exit ≠ 0, eine
Zeile auf stderr. Axiomata ruft es nur auf und zeigt es an — es greift **nie** selbst in `~/.glut` und löscht dort
nichts. Der Knopf „Clean up now“ ruft `glut clean` mit auf, wenn es vorhanden ist. Eine andere `version` → die Zeile
zeigt nur „Glut: unknown format“.

## 6. Offene Fragen

- **M1** Soll „Storage“ auch zeigen, wie groß die Build-Ordner der Studio-Projekte sind — nur als Hinweis, ohne
  Löschknopf? (A1 trennt das; eine Anzeige verletzte die Trennung nicht.)
