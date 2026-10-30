/**
 * The texts of Settings → Storage (`shell/StorageSettings.svelte`) and of its helper (`core/maintenance.ts`). The
 * report's rows come from Rust in English (`axiomata_core::maintenance`); they are named here by their fixed keys,
 * and a row added later shows as the backend words it (`docs/plans/i18n.md`, Q11). Paths, Glut and Axiomata stay as
 * they are (Q13).
 */

import { texts } from "./texts";

const en = {
  title: "Storage",
  lead:
    "Axiomata cleans up its own data in {home} once a week at start. Your projects, the vault and Glut's own folder " +
    "are never touched here.",
  measureError: "Could not measure the storage: {error}",
  measuring: "Measuring…",
  lastCleanup: "Last clean-up",
  never: "never",
  wouldFree: "Would free",
  days: "days",
  kept: "kept",
  frees: "frees {amount}",
  glutKeep:
    "Glut cleans {folder} by its own rules, once a day and with “Clean up now”",
  glutUnknown: "Glut: unknown format",
  weekly: "Clean up once a week at start",
  saving: "Saving…",
  saveRules: "Save rules",
  cleaningUp: "Cleaning up…",
  cleanNow: "Clean up now",
  saveFirst: "Save the rules first: “Clean up now” cleans by the saved ones.",
  endedToRemove: "Ended sessions a clean-up removes",
  endedRemoved: "Ended sessions removed",
  endedHint:
    "Sessions whose harness ended days ago with no work of their own — nothing uncommitted, no commit beyond their " +
    "base, no open card. Their worktree and branch go with them; a session with work always stays.",
  orphansTitle: "Orphaned worktrees waiting for you",
  orphansHint:
    "Their agent is gone, but they hold work nobody saved, so no clean-up removes them.",
  unknownState: "unknown state",
  deleteWorktree: "Delete {path}",
  backupsTitle: "Backups of the old roles and engines",
  backupsHint:
    "The role copies and engines the move to roles at tiers put aside. Nothing uses them; delete one when you no " +
    "longer need it.",
  deleteBackup: "Delete backup {path}",

  // What the actions say.
  rulesSaved: "Storage rules saved",
  saveRulesFailed: "Could not save the rules: {error}",
  confirmCleanFrees:
    "Clean up now? This frees about {size}. Worktrees with changes stay.",
  confirmCleanNothing:
    "Clean up now? Nothing is due right now; the database is still compacted if it needs it.",
  cleaned: "Cleaned up — {size} freed",
  cleanFailed: "Clean-up failed: {error}",
  worktreeDirty: "It has changes nobody saved",
  worktreeUnknown: "Git cannot tell whether it has changes",
  confirmDeleteWorktree:
    "Delete the worktree {path}? {what}; they are gone for good.",
  worktreeDeleted: "Worktree deleted",
  worktreeDeleteFailed: "Could not delete the worktree: {error}",
  confirmDeleteBackup:
    "Delete the backup {path}? The old role files and engines in it are gone for good.",
  backupDeleted: "Backup deleted",
  backupDeleteFailed: "Could not delete the backup: {error}",

  // A rule out of its bounds (`core/maintenance.ts`).
  ruleRange: "{rule} must be a whole number from {min} to {max}",
  ruleDays: "{label} (days)",
  ruleKept: "{label} (kept)",

  // The report's rows, by their keys in `axiomata_core::maintenance`.
  itemRuns: "Runs",
  itemRoutineRuns: "Routine firings",
  itemChatTurns: "Chat turns",
  itemWebSearches: "Web searches",
  itemCardEvents: "Archived card history",
  itemSessionSpend: "Session spend",
  itemDatabase: "Database file",
  itemEndedSessions: "Ended sessions without work",
  itemAppLog: "App log",
  itemRunsLog: "Run log",
  itemBackups: "Backups",
  itemAgentEvents: "Agent status folders",
  itemClaudeSessions: "Claude sessions",
  itemWorktrees: "Orphaned worktrees",

  // What the rows count.
  unitRows: { one: "1 row", other: "{n} rows" },
  unitFiles: { one: "1 file", other: "{n} files" },
  unitSessions: { one: "1 session", other: "{n} sessions" },
  unitFolders: { one: "1 folder", other: "{n} folders" },
  unitEntries: { one: "1 entry", other: "{n} entries" },
  unitWorktrees: { one: "1 worktree", other: "{n} worktrees" },

  // How long a row without an editable rule keeps its data (the others show their rule's fields).
  keepDatabase: "compacted when over {percent} % is free",
  keepRunsLog: "rotated at {mb} MB, the last {n} kept",
  keepEndedSessions: {
    one: "ended, quiet for 1 day, no open card, nothing uncommitted and no commit beyond its base",
    other:
      "ended, quiet for {n} days, no open card, nothing uncommitted and no commit beyond its base",
  },
  keepAgentEvents: "while their agent exists",
  keepWorktrees: "removed with their agent; one with changes waits for you",

  // Glut's items (`glut clean --json`).
  glutSnapshots: "Snapshots",
  glutJournals: "Run journals",
  glutSessions: "Sessions",
  glutResults: "Tool results",
  glutImages: "Images",
  glutExports: "Exports",
  glutLean: "Lean savings log",
  glutOther: "Other",
};

export const tStorage = texts("storage", en, {
  title: "Speicher",
  lead:
    "Axiomata räumt seine eigenen Daten in {home} einmal pro Woche beim Start auf. Deine Projekte, der Vault und " +
    "Gluts eigener Ordner werden hier nie angefasst.",
  measureError: "Der Speicher ließ sich nicht messen: {error}",
  measuring: "Messe…",
  lastCleanup: "Letztes Aufräumen",
  never: "nie",
  wouldFree: "Würde freigeben",
  days: "Tage",
  kept: "behalten",
  frees: "gibt {amount} frei",
  glutKeep:
    "Glut räumt {folder} nach eigenen Regeln auf, einmal am Tag und mit „Jetzt aufräumen“",
  glutUnknown: "Glut: unbekanntes Format",
  weekly: "Einmal pro Woche beim Start aufräumen",
  saving: "Speichere…",
  saveRules: "Regeln speichern",
  cleaningUp: "Räume auf…",
  cleanNow: "Jetzt aufräumen",
  saveFirst:
    "Speichere zuerst die Regeln: „Jetzt aufräumen“ räumt nach den gespeicherten auf.",
  endedToRemove: "Beendete Sitzungen, die ein Aufräumen entfernt",
  endedRemoved: "Entfernte beendete Sitzungen",
  endedHint:
    "Sitzungen, deren Harness vor Tagen ohne eigene Arbeit endete — nichts Uncommittetes, kein Commit über ihre " +
    "Basis hinaus, keine offene Karte. Ihr Worktree und ihr Branch gehen mit; eine Sitzung mit Arbeit bleibt immer.",
  orphansTitle: "Verwaiste Worktrees, die auf dich warten",
  orphansHint:
    "Ihr Agent ist weg, aber sie enthalten Arbeit, die niemand gesichert hat, deshalb entfernt kein Aufräumen sie.",
  unknownState: "Zustand unbekannt",
  deleteWorktree: "{path} löschen",
  backupsTitle: "Backups der alten Rollen und Engines",
  backupsHint:
    "Die Rollen-Kopien und Engines, die der Umzug auf Rollen mit Stufen beiseitegelegt hat. Nichts nutzt sie; " +
    "lösche eines, wenn du es nicht mehr brauchst.",
  deleteBackup: "Backup {path} löschen",

  rulesSaved: "Speicher-Regeln gespeichert",
  saveRulesFailed: "Die Regeln ließen sich nicht speichern: {error}",
  confirmCleanFrees:
    "Jetzt aufräumen? Das gibt etwa {size} frei. Worktrees mit Änderungen bleiben.",
  confirmCleanNothing:
    "Jetzt aufräumen? Gerade ist nichts fällig; die Datenbank wird trotzdem kompaktiert, wenn sie es braucht.",
  cleaned: "Aufgeräumt — {size} freigegeben",
  cleanFailed: "Aufräumen fehlgeschlagen: {error}",
  worktreeDirty: "Er hat Änderungen, die niemand gesichert hat",
  worktreeUnknown: "Git kann nicht sagen, ob er Änderungen hat",
  confirmDeleteWorktree:
    "Den Worktree {path} löschen? {what}; sie sind endgültig weg.",
  worktreeDeleted: "Worktree gelöscht",
  worktreeDeleteFailed: "Der Worktree ließ sich nicht löschen: {error}",
  confirmDeleteBackup:
    "Das Backup {path} löschen? Die alten Rollendateien und Engines darin sind endgültig weg.",
  backupDeleted: "Backup gelöscht",
  backupDeleteFailed: "Das Backup ließ sich nicht löschen: {error}",

  ruleRange: "{rule} muss eine ganze Zahl von {min} bis {max} sein",
  ruleDays: "{label} (Tage)",
  ruleKept: "{label} (behalten)",

  itemRuns: "Läufe",
  itemRoutineRuns: "Läufe der Routines",
  itemChatTurns: "Chat-Schritte",
  itemWebSearches: "Websuchen",
  itemCardEvents: "Verlauf archivierter Karten",
  itemSessionSpend: "Sitzungskosten",
  itemDatabase: "Datenbankdatei",
  itemEndedSessions: "Beendete Sitzungen ohne Arbeit",
  itemAppLog: "App-Log",
  itemRunsLog: "Lauf-Log",
  itemBackups: "Backups",
  itemAgentEvents: "Status-Ordner der Agenten",
  itemClaudeSessions: "Claude-Sitzungen",
  itemWorktrees: "Verwaiste Worktrees",

  unitRows: { one: "1 Zeile", other: "{n} Zeilen" },
  unitFiles: { one: "1 Datei", other: "{n} Dateien" },
  unitSessions: { one: "1 Sitzung", other: "{n} Sitzungen" },
  unitFolders: { one: "1 Ordner", other: "{n} Ordner" },
  unitEntries: { one: "1 Eintrag", other: "{n} Einträge" },
  unitWorktrees: { one: "1 Worktree", other: "{n} Worktrees" },

  keepDatabase: "kompaktiert, wenn über {percent} % frei sind",
  keepRunsLog: "rotiert bei {mb} MB, die letzten {n} bleiben",
  keepEndedSessions: {
    one: "beendet, seit 1 Tag still, keine offene Karte, nichts Uncommittetes und kein Commit über ihre Basis hinaus",
    other:
      "beendet, seit {n} Tagen still, keine offene Karte, nichts Uncommittetes und kein Commit über ihre Basis hinaus",
  },
  keepAgentEvents: "solange ihr Agent existiert",
  keepWorktrees:
    "mit ihrem Agenten entfernt; einer mit Änderungen wartet auf dich",

  glutSnapshots: "Snapshots",
  glutJournals: "Lauf-Journale",
  glutSessions: "Sitzungen",
  glutResults: "Tool-Ergebnisse",
  glutImages: "Bilder",
  glutExports: "Exporte",
  glutLean: "Lean-Ersparnis-Log",
  glutOther: "Sonstiges",
});
