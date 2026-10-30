/**
 * The texts of the app shell: the dock and its heartbeat (`shell/Dock.svelte`, `shell/Heartbeat.svelte`), the ⌘K
 * spotlight (`shell/Spotlight.svelte`, `core/spotlight.ts`, `core/spotlightItems.ts`), the chat panel and the chat's
 * questions, the module and theme pickers, the handbook panel and the toasts. Module titles, theme names, skill and
 * file names come from data or their own definitions and stay as they are (`docs/plans/i18n.md`, Q13).
 */

import { texts } from "./texts";

const en = {
  // A name in both languages, kept here so the markup carries no literal of its own.
  appName: "Axiomata",

  // The dock.
  dockLabel: "Dashboard actions",
  targetNote: "Note",
  atWork: "{n} at work",
  leaveFocus: "Leave the focus mode ({keys})",
  focus: "Focus",
  searchOrRun: "Search or run",
  searchOrRunKeys: "Search or run ({keys})",
  addModule: "Add module",
  theme: "Theme",
  handbook: "Handbook",
  settingsKeys: "Settings ({keys})",

  // The heartbeat and the tank.
  now: "now",
  todayStrip: { one: "Today's work: 1 run or session", other: "Today's work: {n} runs and sessions" },
  spentNoCap: "Spent today: {amount}, no daily limit",
  spentOfCap: "Spent today: {amount} of {cap}",
  todaysWork: "Today's work",
  todayHead: { one: "Today · 1 entry", other: "Today · {n} entries" },
  nothingToday: "Nothing ran today yet.",
  running: "running",
  runLabel: "Run {name}",
  beatRoutine: "{name} (routine)",
  durSeconds: "{n} s",
  durMinutes: "{n} min",
  durHours: "{h} h",
  durHoursMinutes: "{h} h {m} min",

  // The chat panel.
  agent: "Agent",
  session: "session {id}…",
  newSession: "new session",
  toolsUsed: "Actions the agent used",
  turnStopped: "A turn stopped when the app did:",
  noText: "(no text)",
  resume: "Resume",
  drop: "Drop",
  thinking: "Axiomata is thinking",

  // The chat's questions.
  questionsForYou: "Questions for you",
  deny: "Deny",
  allow: "Allow",
  askerChat: "The chat asks",
  askerAgent: "Agent {name} asks",
  askerCommand: "A command asks",
  runNow: "Run it now",
  tooLongTitle: "Too long to show in full: set it up yourself",
  tooLongNote: "Too long to show in full — it cannot be allowed from here.",

  // The module picker.
  noModules: "No modules registered.",
  background: "background",
  single: "single",
  placed: "placed",

  // The handbook panel.
  handbookView: "Handbook — {title}",
  handbookPages: "Handbook pages",

  // The toasts.
  dismiss: "Dismiss",

  // The spotlight.
  search: "Search",
  searchPlaceholder: "Search, or ask Axiomata …",
  results: "Results",
  askGroup: "Ask",
  askAxiomata: "Ask Axiomata",
  quoted: "“{text}”",
  searchingFiles: "Searching files …",
  stillAnswering: "The agent is still answering — ask again in a moment.",

  // The spotlight's groups.
  groupSuggestion: "Jump to",
  groupCommand: "Commands",
  groupModule: "Modules",
  groupSkill: "Skills",
  groupRoutine: "Routines",
  groupCard: "Cards",
  groupPlan: "Plans",
  groupProject: "Projects",
  groupHandbook: "Handbook",
  groupTag: "Tags",
  groupFile: "Files",

  // The spotlight's items.
  cmdNewNote: "New note",
  cmdSettings: "Settings",
  cmdKanban: "Open Kanban",
  cmdStudio: "Open Studio",
  cmdBrain: "Open Second Brain",
  placeModule: "Place module",
  routineOn: "on",
  routineOff: "off",
  notes: { one: "1 note", other: "{n} notes" },
};

export const tShell = texts("shell", en, {
  appName: "Axiomata",

  dockLabel: "Aktionen des Dashboards",
  targetNote: "Notiz",
  atWork: "{n} bei der Arbeit",
  leaveFocus: "Fokusmodus verlassen ({keys})",
  focus: "Fokus",
  searchOrRun: "Suchen oder ausführen",
  searchOrRunKeys: "Suchen oder ausführen ({keys})",
  addModule: "Modul hinzufügen",
  theme: "Theme",
  handbook: "Handbuch",
  settingsKeys: "Einstellungen ({keys})",

  now: "jetzt",
  todayStrip: { one: "Heute: 1 Lauf oder Sitzung", other: "Heute: {n} Läufe und Sitzungen" },
  spentNoCap: "Heute ausgegeben: {amount}, kein Tageslimit",
  spentOfCap: "Heute ausgegeben: {amount} von {cap}",
  todaysWork: "Heute erledigt",
  todayHead: { one: "Heute · 1 Eintrag", other: "Heute · {n} Einträge" },
  nothingToday: "Heute lief noch nichts.",
  running: "läuft",
  runLabel: "Lauf {name}",
  beatRoutine: "{name} (Routines)",
  durSeconds: "{n} s",
  durMinutes: "{n} Min.",
  durHours: "{h} Std.",
  durHoursMinutes: "{h} Std. {m} Min.",

  agent: "Agent",
  session: "Gespräch {id}…",
  newSession: "neues Gespräch",
  toolsUsed: "Aktionen, die der Agent genutzt hat",
  turnStopped: "Ein Schritt brach ab, als die App beendet wurde:",
  noText: "(kein Text)",
  resume: "Fortsetzen",
  drop: "Verwerfen",
  thinking: "Axiomata denkt nach",

  questionsForYou: "Fragen an dich",
  deny: "Ablehnen",
  allow: "Erlauben",
  askerChat: "Der Chat fragt",
  askerAgent: "Agent {name} fragt",
  askerCommand: "Ein Befehl fragt",
  runNow: "Jetzt ausführen",
  tooLongTitle: "Zu lang, um es ganz zu zeigen: richte es selbst ein",
  tooLongNote:
    "Zu lang, um es ganz zu zeigen — von hier aus lässt es sich nicht erlauben.",

  noModules: "Keine Module registriert.",
  background: "Hintergrund",
  single: "einmalig",
  placed: "platziert",

  handbookView: "Handbuch — {title}",
  handbookPages: "Seiten des Handbuchs",

  dismiss: "Schließen",

  search: "Suche",
  searchPlaceholder: "Suchen oder Axiomata fragen …",
  results: "Ergebnisse",
  askGroup: "Fragen",
  askAxiomata: "Axiomata fragen",
  quoted: "„{text}“",
  searchingFiles: "Durchsuche Dateien …",
  stillAnswering: "Der Agent antwortet noch — frag gleich noch einmal.",

  groupSuggestion: "Gehe zu",
  groupCommand: "Befehle",
  groupModule: "Module",
  groupSkill: "Skills",
  groupRoutine: "Routines",
  groupCard: "Karten",
  groupPlan: "Pläne",
  groupProject: "Projekte",
  groupHandbook: "Handbuch",
  groupTag: "Tags",
  groupFile: "Dateien",

  cmdNewNote: "Neue Notiz",
  cmdSettings: "Einstellungen",
  cmdKanban: "Kanban öffnen",
  cmdStudio: "Studio öffnen",
  cmdBrain: "Second Brain öffnen",
  placeModule: "Modul platzieren",
  routineOn: "an",
  routineOff: "aus",
  notes: { one: "1 Notiz", other: "{n} Notizen" },
});
