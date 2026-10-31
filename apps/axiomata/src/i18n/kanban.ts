/**
 * The texts of the Kanban board: the board panel and a card's detail (`modules/kanban.svelte`), the boards popover
 * (`modules/KanbanBoards.svelte`), a card's step forms and usage (`modules/Card*.svelte`), the waiting badge
 * (`ui/WaitingBadge.svelte`) and the pure helpers whose words only they show (`core/kanban.ts`, `ide/cardPlan.ts`,
 * `core/agentWaiting.ts` `waitedFor`, `modules/kanbanApp.ts`). Column names, labels and a card's own text are data and
 * never pass through here; the tiers Light/Medium/Heavy and role names stay as they are (`docs/plans/i18n.md`, Q13).
 */

import { texts } from "./texts";

const en = {
  // A card's state as the board shows it (`core/kanban.ts` `STATE_LABEL`).
  stateProposed: "proposal",
  stateBlocked: "waiting",
  stateReady: "ready",
  stateWorking: "in progress",
  statePlanned: "plan",
  stateInputRequired: "question",
  stateInReview: "in review",
  stateDone: "done",
  stateVerified: "checked",
  stateIntegrated: "in the plan",
  stateTakenOver: "taken over",
  stateFailed: "failed",
  stateCanceled: "canceled",

  // A line of a card's history (`core/kanban.ts` `EVENT_LABEL`).
  eventStarted: "started",
  eventReported: "reported done",
  eventApproved: "checked",
  eventReturned: "returned",
  eventEscalated: "escalated",
  eventLimitStop: "limit reached",
  eventInputRequired: "question",
  eventInputProvided: "answered",
  eventFailed: "failed",
  eventCanceled: "called off",
  eventReleased: "released",
  eventTakenOver: "taken over",
  eventIntegrated: "taken into the plan",
  eventNote: "note",

  // Where a plan made from a card stands (`ide/cardPlan.ts` `PHASE_LABEL`).
  phasePlanning: "planning",
  phaseRunning: "running",
  phaseWaiting: "waits for your take-over",

  // A due date (`core/kanban.ts` `dueState`).
  dueOverdue: "{n} d overdue",
  dueToday: "today",
  dueTomorrow: "tomorrow",
  dueIn: "in {n} d",

  // How long an agent has waited (`core/agentWaiting.ts` `waitedFor`).
  waitedNow: "now",
  waitedMinutes: "{n} min",
  waitedHours: "{n} h",

  // What a column's role means.
  roleOpen: "open",
  roleDoing: "in progress",
  roleDone: "done",

  // A card on the board.
  approvedBy: "approved by {name}",
  planBadge: "plan · {phase}",
  planBadgeSteps: "plan · {phase}\u00a0· {done}/{total}",
  stateOn: "{state} on {ids}",
  returnedByReviewer: "returned by the reviewer",

  // A card's detail.
  cardNumber: "Card #{id}",
  title: "Title",
  text: "Text",
  noText: "No text.",
  labels: "Labels",
  labelsPlaceholder: "rust, design",
  due: "Due",
  assignedTo: "Assigned to",
  assigneePlaceholder: "human:owner or agent:name",
  agentFlow: "Agent flow",
  kind: "Kind",
  kindPlaceholder: "implement, review, test, doc",
  rolePlaceholder: "e.g. allrounder",
  reason: "Reason",
  reasonPlaceholder: "why this role",
  plan: "Plan",
  goalOfPlan: "The goal of plan #{id}; it is no step of a plan",
  noPlan: "no plan",
  acceptancePlaceholder:
    "Acceptance criteria — how the reviewer tells the card is done.",
  removeDependency: "Remove dependency on #{id}",
  addDependency: "Add dependency",
  addDependencyOption: "+ Dependency…",
  none: "none",
  state: "State",
  returnedTimes: "{n}× returned",
  stepsDone: "{done}/{total} steps done",
  question: "Question",
  dueLabel: "Due",
  claimedBy: "Claimed by",
  approvedByLabel: "Approved by",
  archived: "Archived",
  yes: "yes",
  takeOverInFlowTitle: "Every step is ready: take the plan over in the Flow",
  openInFlowTitle: "The plan's steps, planner and graph",
  takeOverInFlow: "Take over in the Flow…",
  openInFlow: "Open in the Flow",
  startAsPlanTitle:
    "The card becomes a plan: a planner splits it, then it runs by itself",
  startEllipsis: "Start…",
  startReviewTitle:
    "The reviewer starts by itself; here with an engine of your choice",
  startReviewEllipsis: "Start review…",
  takeOverEllipsis: "Take over…",
  releaseTitle:
    "The card waits in Open again; the session's secret is withdrawn",
  release: "Release",
  acceptProposal: "Accept proposal",
  reopen: "Reopen",
  callOff: "Call off",
  archive: "Archive",
  restore: "Restore",
  writeNote: "Write a note…",
  noteLabel: "Note for the history",
  nothingYet: "Nothing has happened yet.",

  // The board panel.
  noBoardYet: "No board yet",
  createFirstBelow: "Create the first one below.",
  loading: "Loading…",
  cardGone: "This card no longer exists.",
  chooseBoard: "Choose a board",
  filterPlaceholder: "Filter cards…",
  filterLabel: "Filter cards",
  hideArchived: "Hide archived cards",
  showArchived: "Show archived cards",
  reloadBoard: "Reload the board",
  addColumn: "Add a column",
  boardsAndView: "Boards & view",
  columnName: "Name of the column",
  columnActions: "Column {name}: actions",
  rename: "Rename",
  remove: "Remove",
  reviewColumn: "Review column of the flow — stays on every board",
  proposalColumn: "Proposal column of the flow — stays on every board",
  removeColumn: "Remove the column?",
  cardsWhereTo: { one: "{n} card — where to?", other: "{n} cards — where to?" },
  targetColumn: "Target column",
  choose: "choose…",
  noCards: "No cards",
  newCardPlaceholder: "Title, Enter",
  newCardLabel: "Title of the new card",
  addCard: "Card",
  card: "Card",
  closeCard: "Close the card (Esc)",
  boardEmpty: "Nothing here yet — create the first card in a column below.",
  newColumn: "New column",
  columnHoldsCards:
    "The column still holds cards — choose where they should go.",
  deletedCard: "deleted",

  // Moving a card by keyboard, for a screen reader.
  pickedUp: "{title} picked up. Arrow keys move, Space drops, Escape cancels.",
  pickedUpAt:
    "{title} picked up — {column}, position {n}. Arrow keys move, Space drops, Escape cancels.",
  dropped: "{title} dropped.",
  staysPut: "{title} stays where it was.",

  // What closing the panel would lose.
  lostTitle: "the new card title “{title}” not yet added",
  lostNote: "the history note “{note}” not yet written",
  lostForm: "the open {form} form of card #{card} with what was entered in it",
  formPlan: "plan",
  formStart: "start",
  formReview: "review",
  formTakeOver: "take-over",
  and: "and",

  // The boards popover (`KanbanBoards.svelte`).
  boards: "Boards",
  noBoardCreated: "No board created yet.",
  boardName: "Name of the board",
  deleteBoard: "Delete the board",
  deleteBoardQuestion: "Delete the board?",
  cardsDeletedWith: {
    one: "{n} card is deleted with it.",
    other: "{n} cards are deleted with it.",
  },
  newBoard: "New board…",
  newBoardName: "Name of the new board",
  create: "Create",
  cards: "Cards",
  colourStripe: "Colour stripe by the first label",
  allBoards: "Applies to all boards.",
  loadBoardsFailed: "Kanban could not load the boards: {error}",

  // A card's step forms (`CardStartForm`, `CardPlanStartForm`, `CardReviewForm`, `CardTakeOverForm`).
  noProject:
    "There is no project the card could run in. Create one in the Studio.",
  project: "Project",
  engine: "Engine",
  roleOwnEngine: "The role {role}'s own",
  roleOwnEngineWith: "The role {role}'s own ({engine})",
  roleNamesNoEngine: "The role “{role}” names no engine — pick one.",
  starting: "Starting…",
  start: "Start",
  startedInSession: "Card #{id} runs in the session {name}.",
  startAsPlan: "Start as plan",
  nowPlan: "Card #{id} is now plan #{plan}: start its planner in the Flow.",
  planStartHint:
    "The card becomes a plan. In the Studio you choose whether to be grilled and start the planner, which proposes " +
    "the steps. Once you release the plan, the steps run by themselves (up to $5 unless you change it) and the card " +
    "moves along — to Review when everything is ready for your take-over.",
  reviewerEngine: "The reviewer's engine",
  reviewerOwnEngine: "The reviewer role's own",
  reviewerOwnEngineWith: "The reviewer role's own ({engine})",
  allowAgentConfig:
    "Even if the work changes the configuration of agents (.claude, .mcp.json, opencode.json, AGENTS.md…)",
  startReview: "Start review",
  beingReviewed: "Card #{id} is being reviewed by {name}.",
  commitMessage: "Commit message",
  takeOverHint:
    "Exactly the state the reviewer saw is taken over, as one commit on the project's main branch. Nothing is pushed.",
  takeOverConflict:
    "Conflict, nothing was changed. These files are in conflict: {files}",
  takingOver: "Taking over…",
  takeOver: "Take over",
  alreadyOnBase:
    "Card #{id} closed: its work was already on the main line ({commit}), so nothing was committed.",
  takenOver: "Card #{id} was taken over ({commit}).",

  // A card's usage (`CardUsage.svelte`).
  usage: "Usage",
  reviewer: "Reviewer",
  worker: "Worker",
  usageUnknown: "Usage unknown.",
  stopped: "Stopped: {reason}",

  // The waiting badge (`ui/WaitingBadge.svelte`).
  anAgent: "An agent",
  agentWaiting: "{name} is waiting for you",
  agentsWaiting: "{n} agents are waiting for you",
  agentsWaitingLabel: "Agents waiting for you",
  waitingForYou: "Waiting for you",
  noCard: "no card",
  waitsOnPlan: "waits for your answer on its plan",
};

/** The Kanban's texts in the current language. */
export const tKanban = texts("kanban", en, {
  stateProposed: "Vorschlag",
  stateBlocked: "wartet",
  stateReady: "bereit",
  stateWorking: "in Arbeit",
  statePlanned: "Plan",
  stateInputRequired: "Frage",
  stateInReview: "in Prüfung",
  stateDone: "erledigt",
  stateVerified: "geprüft",
  stateIntegrated: "im Plan",
  stateTakenOver: "übernommen",
  stateFailed: "fehlgeschlagen",
  stateCanceled: "abgesagt",

  eventStarted: "gestartet",
  eventReported: "als erledigt gemeldet",
  eventApproved: "geprüft",
  eventReturned: "zurückgegeben",
  eventEscalated: "eskaliert",
  eventLimitStop: "Grenze erreicht",
  eventInputRequired: "Frage",
  eventInputProvided: "beantwortet",
  eventFailed: "fehlgeschlagen",
  eventCanceled: "abgesagt",
  eventReleased: "freigegeben",
  eventTakenOver: "übernommen",
  eventIntegrated: "in den Plan übernommen",
  eventNote: "Notiz",

  phasePlanning: "plant",
  phaseRunning: "läuft",
  phaseWaiting: "wartet auf deine Übernahme",

  dueOverdue: "{n} T überfällig",
  dueToday: "heute",
  dueTomorrow: "morgen",
  dueIn: "in {n} T",

  waitedNow: "jetzt",
  waitedMinutes: "{n} Min.",
  waitedHours: "{n} Std.",

  roleOpen: "offen",
  roleDoing: "in Arbeit",
  roleDone: "erledigt",

  approvedBy: "abgenommen von {name}",
  planBadge: "Plan · {phase}",
  planBadgeSteps: "Plan · {phase}\u00a0· {done}/{total}",
  stateOn: "{state} auf {ids}",
  returnedByReviewer: "vom Reviewer zurückgegeben",

  cardNumber: "Karte #{id}",
  title: "Titel",
  text: "Text",
  noText: "Kein Text.",
  labels: "Labels",
  labelsPlaceholder: "rust, design",
  due: "Fällig",
  assignedTo: "Zugewiesen an",
  assigneePlaceholder: "human:owner oder agent:name",
  agentFlow: "Agenten-Flow",
  kind: "Art",
  kindPlaceholder: "implement, review, test, doc",
  rolePlaceholder: "z. B. allrounder",
  reason: "Begründung",
  reasonPlaceholder: "warum diese Rolle",
  plan: "Plan",
  goalOfPlan: "Das Ziel von Plan #{id}; es ist kein Schritt eines Plans",
  noPlan: "kein Plan",
  acceptancePlaceholder:
    "Abnahmekriterien — woran der Reviewer erkennt, dass die Karte fertig ist.",
  removeDependency: "Abhängigkeit von #{id} entfernen",
  addDependency: "Abhängigkeit hinzufügen",
  addDependencyOption: "+ Abhängigkeit…",
  none: "keine",
  state: "Zustand",
  returnedTimes: "{n}× zurückgegeben",
  stepsDone: "{done}/{total} Schritte erledigt",
  question: "Frage",
  dueLabel: "Fällig",
  claimedBy: "Beansprucht von",
  approvedByLabel: "Abgenommen von",
  archived: "Archiviert",
  yes: "ja",
  takeOverInFlowTitle: "Jeder Schritt ist bereit: übernimm den Plan im Flow",
  openInFlowTitle: "Die Schritte des Plans, Planer und Graph",
  takeOverInFlow: "Im Flow übernehmen…",
  openInFlow: "Im Flow öffnen",
  startAsPlanTitle:
    "Die Karte wird ein Plan: ein Planer zerlegt sie, dann läuft sie von selbst",
  startEllipsis: "Starten…",
  startReviewTitle:
    "Der Reviewer startet von selbst; hier mit einer Engine deiner Wahl",
  startReviewEllipsis: "Prüfung starten…",
  takeOverEllipsis: "Übernehmen…",
  releaseTitle:
    "Die Karte wartet wieder in Open; das Geheimnis der Sitzung wird entzogen",
  release: "Freigeben",
  acceptProposal: "Vorschlag annehmen",
  reopen: "Wieder öffnen",
  callOff: "Absagen",
  archive: "Archivieren",
  restore: "Wiederherstellen",
  writeNote: "Schreibe eine Notiz…",
  noteLabel: "Notiz für den Verlauf",
  nothingYet: "Noch nichts passiert.",

  noBoardYet: "Noch kein Board",
  createFirstBelow: "Lege unten das erste an.",
  loading: "Lade…",
  cardGone: "Diese Karte gibt es nicht mehr.",
  chooseBoard: "Wähle ein Board",
  filterPlaceholder: "Karten filtern…",
  filterLabel: "Karten filtern",
  hideArchived: "Archivierte Karten ausblenden",
  showArchived: "Archivierte Karten zeigen",
  reloadBoard: "Board neu laden",
  addColumn: "Spalte hinzufügen",
  boardsAndView: "Boards & Ansicht",
  columnName: "Name der Spalte",
  columnActions: "Spalte {name}: Aktionen",
  rename: "Umbenennen",
  remove: "Entfernen",
  reviewColumn: "Review-Spalte des Flows — bleibt auf jedem Board",
  proposalColumn: "Vorschlagsspalte des Flows — bleibt auf jedem Board",
  removeColumn: "Spalte entfernen?",
  cardsWhereTo: { one: "{n} Karte — wohin?", other: "{n} Karten — wohin?" },
  targetColumn: "Zielspalte",
  choose: "wählen…",
  noCards: "Keine Karten",
  newCardPlaceholder: "Titel, Enter",
  newCardLabel: "Titel der neuen Karte",
  addCard: "Karte",
  card: "Karte",
  closeCard: "Karte schließen (Esc)",
  boardEmpty:
    "Noch nichts hier — lege die erste Karte unten in einer Spalte an.",
  newColumn: "Neue Spalte",
  columnHoldsCards: "Die Spalte enthält noch Karten — wähle, wohin sie sollen.",
  deletedCard: "gelöscht",

  pickedUp:
    "{title} aufgenommen. Pfeiltasten verschieben, Leertaste legt ab, Escape bricht ab.",
  pickedUpAt:
    "{title} aufgenommen — {column}, Position {n}. Pfeiltasten verschieben, Leertaste legt ab, Escape bricht ab.",
  dropped: "{title} abgelegt.",
  staysPut: "{title} bleibt, wo sie war.",

  lostTitle: "der neue Kartentitel „{title}“, noch nicht hinzugefügt",
  lostNote: "die Verlaufsnotiz „{note}“, noch nicht geschrieben",
  lostForm:
    "das offene Formular „{form}“ von Karte #{card} mit dem, was darin eingegeben wurde",
  formPlan: "Plan",
  formStart: "Start",
  formReview: "Prüfung",
  formTakeOver: "Übernahme",
  and: "und",

  boards: "Boards",
  noBoardCreated: "Noch kein Board angelegt.",
  boardName: "Name des Boards",
  deleteBoard: "Board löschen",
  deleteBoardQuestion: "Board löschen?",
  cardsDeletedWith: {
    one: "{n} Karte wird mit gelöscht.",
    other: "{n} Karten werden mit gelöscht.",
  },
  newBoard: "Neues Board…",
  newBoardName: "Name des neuen Boards",
  create: "Anlegen",
  cards: "Karten",
  colourStripe: "Farbstreifen nach dem ersten Label",
  allBoards: "Gilt für alle Boards.",
  loadBoardsFailed: "Kanban konnte die Boards nicht laden: {error}",

  noProject:
    "Es gibt kein Projekt, in dem die Karte laufen könnte. Lege eins im Studio an.",
  project: "Projekt",
  engine: "Engine",
  roleOwnEngine: "Die eigene der Rolle {role}",
  roleOwnEngineWith: "Die eigene der Rolle {role} ({engine})",
  roleNamesNoEngine: "Die Rolle „{role}“ nennt keine Engine — wähle eine.",
  starting: "Starte…",
  start: "Starten",
  startedInSession: "Karte #{id} läuft in der Sitzung {name}.",
  startAsPlan: "Als Plan starten",
  nowPlan: "Karte #{id} ist jetzt Plan #{plan}: starte seinen Planer im Flow.",
  planStartHint:
    "Die Karte wird ein Plan. Im Studio wählst du, ob du gegrillt wirst, und startest den Planer, der die Schritte " +
    "vorschlägt. Sobald du den Plan freigibst, laufen die Schritte von selbst (bis 5 $, wenn du es nicht änderst), " +
    "und die Karte zieht mit — nach Review, wenn alles für deine Übernahme bereit ist.",
  reviewerEngine: "Engine des Reviewers",
  reviewerOwnEngine: "Die eigene der Rolle reviewer",
  reviewerOwnEngineWith: "Die eigene der Rolle reviewer ({engine})",
  allowAgentConfig:
    "Auch wenn die Arbeit die Konfiguration von Agenten ändert (.claude, .mcp.json, opencode.json, AGENTS.md…)",
  startReview: "Prüfung starten",
  beingReviewed: "Karte #{id} wird von {name} geprüft.",
  commitMessage: "Commit-Nachricht",
  takeOverHint:
    "Übernommen wird genau der Stand, den der Reviewer gesehen hat, " +
    "als ein Commit auf dem Haupt-Branch des Projekts. Es wird nichts gepusht.",
  takeOverConflict:
    "Konflikt, nichts wurde verändert. Diese Dateien sind in Konflikt: {files}",
  takingOver: "Übernehme…",
  takeOver: "Übernehmen",
  alreadyOnBase:
    "Karte #{id} geschlossen: ihre Arbeit war schon auf dem Haupt-Branch ({commit}), also wurde nichts committet.",
  takenOver: "Karte #{id} wurde übernommen ({commit}).",

  usage: "Verbrauch",
  reviewer: "Reviewer",
  worker: "Bearbeiter",
  usageUnknown: "Verbrauch unbekannt.",
  stopped: "Gestoppt: {reason}",

  anAgent: "Ein Agent",
  agentWaiting: "{name} wartet auf dich",
  agentsWaiting: "{n} Agenten warten auf dich",
  agentsWaitingLabel: "Agenten, die auf dich warten",
  waitingForYou: "Wartet auf dich",
  noCard: "keine Karte",
  waitsOnPlan: "wartet auf deine Antwort zu seinem Plan",
});

/** A key of the Kanban's catalog. */
export type KanbanKey = Parameters<typeof tKanban>[0];

/**
 * A label table whose texts are read on access (`STATE_LABEL[state]`), so every place that shows an entry follows a
 * language switch without its callers changing: each entry is a getter over its catalog key.
 */
export function kanbanLabels<K extends string>(
  keys: Readonly<Record<K, KanbanKey>>,
): Readonly<Record<K, string>> {
  const table = {} as Record<K, string>;
  for (const [id, key] of Object.entries(keys) as [K, KanbanKey][]) {
    Object.defineProperty(table, id, {
      get: () => tKanban(key),
      enumerable: true,
    });
  }
  return table;
}
