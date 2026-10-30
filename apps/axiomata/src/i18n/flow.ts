/**
 * The texts of the Studio's Flow mode and its planning: the Planning pane (`ide/panes/PlanPane.svelte`,
 * `ide/panes/ProposalEditor.svelte`), the Agents tab (`ide/panes/TeamPane.svelte`, `ide/SessionMail.svelte`,
 * `ide/AgentTeamEntry.svelte`), the Flow view (`ide/panes/GraphPane.svelte`) and the pure helpers whose words only they
 * show (`ide/planning.ts`, `ide/flowGraph.ts`, `ide/team.ts`, `ide/agentEntry.ts`). Role, engine, tier and git names
 * stay as they are (`docs/plans/i18n.md`, Q13).
 */

import { texts } from "./texts";

const en = {
  // A plan's status, as the list and the header show it.
  statusDraft: "draft",
  statusReleased: "released",
  statusClosed: "closed",
  cardFallback: "Card #{id}",
  needs: "needs {ids}",

  // What the plans that run by themselves did, toast-sized (`planning.ts` `runEventNote`).
  eventStarted: "Plan #{plan}: card #{card} starts by itself.",
  eventIntegrated: "Card #{card} is integrated into plan #{plan}.",
  eventGaveUp:
    "Card #{card} does not fit the plan the second time either ({files}); it stays as it is. You decide.",
  eventRedo:
    "Card #{card} no longer fit the plan ({files}) and will be redone on the new state.",
  eventBlocked: "Card #{card} made no progress: {reason}",
  eventReady:
    "The plan “{name}” is finished: all cards are integrated. Ready to take over.",
  eventEscalated:
    "Card #{card} was returned twice: “{role}” on “{engine}” takes over from “{from}” (same worktree, same " +
    "branch). If that does not work either, you decide.",
  eventPaused:
    "Plan “{name}” is paused: {reason}. Nothing new starts until you say “Continue” in the Flow.",
  eventDayCap:
    "Daily limit of the Studio sessions reached ({reason}). Nothing new starts until tomorrow or until you raise it " +
    "in the config.",
  spendTokens: "{spent} of {limit} tokens",
  spendCost: "{spent} of {limit}",

  // The graph's goal and review nodes (`flowGraph.ts`).
  endClosed: "closed",
  endReady: "ready to take over",
  endBlocked: "blocked",
  endOpen: "open",
  reviewRunning: "being reviewed",
  reviewSigned: "signed off",
  reviewReturned: "returned ({n}×)",

  // The Agents tab's tiles (`team.ts`, `TeamPane.svelte`).
  noPlan: "No plan",
  planGroup: "Plan #{id} · {name}",
  dutyWorker: "works on",
  dutyReviewer: "reviews",
  dutyPlanner: "plans",
  thePlan: "the plan",
  nothingRecorded: "nothing recorded yet",
  noStepYet: "no step yet",
  nothingRunning: "Nothing is running yet",
  nothingRunningText:
    "Once a plan is approved and its cards start (or a planner works), every session appears here as a tile — with " +
    "what it is doing right now.",
  branchTitle: "Checked out in its worktree: {branch}",
  unreadTitle: "Unread messages in the session's inbox",
  watchTitle:
    "Follow this session read-only in a pane of its own: nothing typed there reaches it",
  watch: "Watch",
  terminalTitle:
    "Bring this session's terminal to the front; it does not start a new one",
  terminal: "Terminal",
  waitingForYou: "Waiting for you: a question or an approval in the terminal.",
  asks: "Asks: {question}",
  latestSteps: "Latest steps",

  // A session's MCP entry (`agentEntry.ts`, `AgentTeamEntry.svelte`).
  teamTools: "Team tools",
  entryPreparing: "Preparing…",
  entryConnected: "Team tools connected",
  entryConnectedDetail:
    "This session can reach the other sessions and the board through the tools below.",
  entryUnavailable: "Team tools unavailable",
  entryUnavailableDetail: "The session runs without them.",
  entryOff: "Team tools off",
  entryCommandTitle:
    "The program the harness starts as the {server} MCP server",

  // A session's conversation (`SessionMail.svelte`).
  messages: "Messages",
  noMessages: "No messages yet.",
  unread: "unread",
  mailHeld: "held",
  mailUndeliverable: "undeliverable",
  savedUnread: "Saved, but nobody reads it: {reason}",
  writePlaceholder: "Write to the session… (⌘↩ sends)",
  send: "Send",

  // A proposal's editor (`ProposalEditor.svelte`).
  title: "Title",
  description: "Description",
  acceptance: "Acceptance criteria",
  acceptancePlaceholder: "How do you tell the card is done?",
  role: "Role",
  level: "Level",
  needsFirst: "Needs first",
  noCandidates: "The plan has no other card this one could wait for.",
  save: "Save",
  addCard: "Add card",
  cancel: "Cancel",
  cardInProgress:
    "The card is already in progress or done; it can no longer be changed here.",
  noColumn: "The board has no column for it.",

  // The Flow view (`GraphPane.svelte`).
  legend: "Colours",
  legendIdle: "ready / waiting",
  legendActive: "in progress",
  legendQuestion: "question",
  legendReview: "in review",
  legendDone: "checked / in the plan",
  legendFailed: "failed",
  noPlanYet: "No plan yet",
  noPlanYetText: "Create one under Planning; its cards appear here as a graph.",
  noCardsYet: "The plan has no cards yet",
  noCardsYetText: "The planner creates them as proposals.",
  graphLabel: "Graph of the plan {name}",
  startLabel: "Start of the plan",
  start: "Start",
  goalLabel: "Goal of the plan: {label}",
  goal: "Goal",
  reviewNodeLabel: "Review of card {id}, {state}",
  cardNodeLabel: "Card {id}, {state}",
  reviewTag: "REVIEW",
  cardKind: "card",
  card: "Card",
  roleLine: "Role {role}",
  needsFirstList: "needs first: {ids}",
  openSession: "Open session: {name}",
  gaveUp: "Does not fit the plan twice; the Studio gave up.",
  integrateAgain: "Integrate again",
  redo: "Redo",
  history: "History",
  clickCard: "Click a card to see its history and its sessions.",
  cardIntegrated: "Card #{id} is integrated into the plan.",
  cardStillConflicts: "Card #{id} still does not fit into the plan ({files}).",
  cardWorkerMoving:
    "The worker of card #{id} is still moving; try again in a moment.",
  cardRedone: "Card #{id} will be redone on the plan's new state.",

  // The Planning pane (`PlanPane.svelte`).
  plans: "Plans",
  board: "Board",
  newPlan: "New plan…",
  name: "Name",
  planName: "Name of the plan",
  goalOfPlan: "Goal of the plan",
  goalPlaceholder: "What should be there at the end?",
  create: "Create",
  noPlanOnBoard: "No plan on this board yet.",
  resetTitle: "Put Planning, Agents and the Flow view back in their places",
  resetLayout: "Reset layout",
  plan: "Plan",
  noBoard: "There is no board yet. Create one in the Kanban.",
  pickPlan:
    "Pick a plan or create one: you write the goal, the planner cuts it into cards.",
  runsByItself: "runs by itself",
  closeTitle:
    "Close the plan: nothing starts from it any more, and its line is removed",
  closeBlocked:
    "A session still works on a card of the plan: let it finish or release it first",
  close: "Close",
  reallyDelete: "Really delete",
  delete: "Delete",
  deleteTitle: "Delete the plan; its cards stay on the board without a plan",
  deleteBlocked: "The plan is still running: take it over or close it first",
  origin:
    "The goal of card #{id} on the board. Once released its steps run by themselves, and the card follows them — to " +
    "Review when everything waits for your take-over.",
  costLimit: "Cost limit (USD)",
  noLimit: "none",
  costInvalid: "The cost limit is a positive amount in USD, or empty for none.",
  suggestedGoal: "Suggested goal",
  sharpenedTitle: "Sharpened goal suggested",
  sharpenedText:
    "From the conversation with the grill session. It replaces your goal only when you take it over.",
  takeOver: "Take over",
  discard: "Discard",
  plannerIs: "Planner:",
  grillIs: "Grill:",
  showPane: "Show pane",
  plannerRole: "Role of the planner",
  grillRole: "Role for grilling",
  grillAfter: "Grill afterwards",
  needsGoal: "The planner needs a goal.",
  startPlanner: "Start planner",
  grillNoEngine:
    "Pick an engine for the grill role, or the grilling will not start.",
  grillAfterPlanner:
    "The grilling starts as soon as the planner has proposed its cards and waits for you.",
  grillSoon:
    "The grilling starts in a moment: the cards are there, the planner is no longer running.",
  grillAfterPlanning:
    "After planning, a grill session questions you about the cards.",
  proposals: "Proposals",
  proposalsCount: "Proposals ({n})",
  plannerNothingYet: "The planner has not proposed anything yet.",
  noProposals: "No proposals waiting for your yes.",
  changedTitle: "The planner changed this proposal after creating it",
  changedByPlanner: "changed by the planner",
  roleOf: "Role of {title}",
  levelOf: "Level of {title}",
  accept: "Accept",
  edit: "Edit",
  reallyDiscard: "Really discard",
  addCardRunning: "Add card to the running plan",
  releasePlan: "Release plan",
  runAutomatically: "Run by itself",
  autoStartText:
    "Cards start by themselves as soon as their predecessors are done, as many at once as possible; the review runs " +
    "by itself.",
  manualText:
    "The proposals move to the first open column; you start every card yourself.",
  plannerDoneAfter: "The planner is done after that.",
  autoMode: "Auto mode",
  autoRightsOn:
    "The plan's workers may run any command and edit without asking. A push, the CLI, the database and config files " +
    "stay blocked in their plain forms — patterns, not a sandbox: switch it on for cards you have read. Reviewers " +
    "may run any command too but get no edit tools; taking the plan over stays yours.",
  autoRightsOff:
    "The plan's workers ask before commands their role does not allow.",
  notGrilled:
    "This plan was not grilled: its goal has not been questioned yet. You can still release it.",
  spend: "Spend: {line}",
  paused: "Paused: {reason}.",
  continue: "Continue",
  continueText:
    "Sets the plan's limit to the spend so far plus a new allowance.",
  dayCapBefore:
    "Daily limit of the Studio sessions reached: {reason}. Nothing new starts until tomorrow; raise it:",
  dayCapAfter: "in the config.",
  today: "Today: {spent} of {cap}",
  autoModeRunning:
    "Anything but a push (blocked by patterns, not a sandbox), without asking. Applies to sessions started from now " +
    "on; running ones keep their rights — to stop one, stop its card.",
  takeOverPlan: "Take over plan",
  takeOverText:
    "All cards are on the plan's line. The work goes into the project's main branch, its commits stay separate as " +
    "long as the branch has not moved. Nothing is pushed; a conflict is rolled back.",
  conflictMain: "Conflict with the main branch, nothing was changed: {files}",
  cardsOfPlan: "Cards of the plan",
  plannerReading: "The planner {name} is reading the project.",
  grillQuestioning:
    "{name} is now questioning you about the cards in the terminal.",
  grillFailed: "The grilling could not be started; tick the box again.",
  autoModeGone: "The plan is closed or gone; its auto mode was not changed.",
  autoOn:
    "Auto mode on: sessions started from now on may do anything but push.",
  autoOff: "Auto mode off: sessions started from now on ask again.",
  noLongerDraft: "The plan is no longer a draft.",
  releasedWorking: {
    one: "Plan released: {n} card is being worked on.",
    other: "Plan released: {n} cards are being worked on.",
  },
  releasedWaiting: {
    one: "Plan released: {n} card waits in the first open column.",
    other: "Plan released: {n} cards wait in the first open column.",
  },
  takenOver: {
    one: "Plan “{name}” was taken over ({commit}): {n} card.",
    other: "Plan “{name}” was taken over ({commit}): {n} cards.",
  },
  notCleaned: "Not cleaned up: {note}",
  resumed:
    "Plan “{name}” keeps running: a new limit was set on top of the current spend.",
  deleted:
    "Plan “{name}” was deleted; its cards stay on the board, without a plan.",
  closeConfirm:
    "Close the plan “{name}”? Nothing starts from it any more and its line is removed; work on the line that was " +
    "not taken over stays on its branch.",
  closedNote: "Plan “{name}” was closed. {notes}",
  noLongerProposal: "The card is no longer a proposal and stays.",

  // A card session's usage (`cardUsage.ts`) and the note after a take-over (`cardStart.ts` `unpushedNote`).
  usageSteps: "Steps",
  usageTokens: "Tokens",
  usageCost: "Cost",
  unmeasuredNoSessionId:
    "No Claude Code session was recorded for this session (it ran with its own command).",
  unmeasuredNoRecordYet:
    "The session has not recorded anything yet — it may have just started.",
  unmeasuredReadFailed: "The session's record could not be read.",
  unmeasuredNotMeasured: "This harness is not measured.",
  unmeasuredLimit:
    "{why} A limit only applies once the session can be measured.",
  notPushedAhead:
    "Not pushed yet: ↑{ahead} to {target}. Push in the Studio's Git tab.",
  theUpstream: "the upstream",
  notPushedNoUpstream:
    "Not pushed yet: the branch has no upstream yet. Pushing in the Studio's Git tab creates it.",
};

export const tFlow = texts("flow", en, {
  statusDraft: "Entwurf",
  statusReleased: "freigegeben",
  statusClosed: "geschlossen",
  cardFallback: "Karte #{id}",
  needs: "braucht {ids}",

  eventStarted: "Plan #{plan}: Karte #{card} startet von selbst.",
  eventIntegrated: "Karte #{card} ist in Plan #{plan} integriert.",
  eventGaveUp:
    "Karte #{card} passt auch beim zweiten Mal nicht in den Plan ({files}); sie bleibt, wie sie ist. Du entscheidest.",
  eventRedo:
    "Karte #{card} passte nicht mehr in den Plan ({files}) und wird auf dem neuen Stand neu gemacht.",
  eventBlocked: "Karte #{card} kam nicht voran: {reason}",
  eventReady:
    "Der Plan „{name}“ ist fertig: alle Karten sind integriert. Bereit zum Übernehmen.",
  eventEscalated:
    "Karte #{card} kam zweimal zurück: „{role}“ auf „{engine}“ übernimmt von „{from}“ (gleicher Worktree, gleicher " +
    "Branch). Klappt auch das nicht, entscheidest du.",
  eventPaused:
    "Plan „{name}“ ist pausiert: {reason}. Nichts Neues startet, bis du im Flow „Weiter“ sagst.",
  eventDayCap:
    "Tagesgrenze der Studio-Sitzungen erreicht ({reason}). Nichts Neues startet bis morgen oder bis du sie in der " +
    "Config anhebst.",
  spendTokens: "{spent} von {limit} Tokens",
  spendCost: "{spent} von {limit}",

  endClosed: "geschlossen",
  endReady: "bereit zum Übernehmen",
  endBlocked: "blockiert",
  endOpen: "offen",
  reviewRunning: "wird geprüft",
  reviewSigned: "abgezeichnet",
  reviewReturned: "zurückgegeben ({n}×)",

  noPlan: "Kein Plan",
  planGroup: "Plan #{id} · {name}",
  dutyWorker: "arbeitet an",
  dutyReviewer: "prüft",
  dutyPlanner: "plant",
  thePlan: "den Plan",
  nothingRecorded: "noch nichts aufgezeichnet",
  noStepYet: "noch kein Schritt",
  nothingRunning: "Noch läuft nichts",
  nothingRunningText:
    "Sobald ein Plan freigegeben ist und seine Karten starten (oder ein Planer arbeitet), erscheint hier jede " +
    "Sitzung als Kachel — mit dem, was sie gerade tut.",
  branchTitle: "In ihrem Worktree ausgecheckt: {branch}",
  unreadTitle: "Ungelesene Nachrichten im Posteingang der Sitzung",
  watchTitle:
    "Sieh dieser Sitzung nur lesend in einem eigenen Pane zu: nichts, was du dort tippst, erreicht sie",
  watch: "Zusehen",
  terminalTitle:
    "Holt das Terminal dieser Sitzung nach vorn; es startet kein neues",
  terminal: "Terminal",
  waitingForYou: "Wartet auf dich: eine Frage oder eine Freigabe im Terminal.",
  asks: "Fragt: {question}",
  latestSteps: "Letzte Schritte",

  teamTools: "Team-Werkzeuge",
  entryPreparing: "Wird vorbereitet…",
  entryConnected: "Team-Werkzeuge verbunden",
  entryConnectedDetail:
    "Diese Sitzung erreicht die anderen Sitzungen und das Board über die Werkzeuge unten.",
  entryUnavailable: "Team-Werkzeuge nicht verfügbar",
  entryUnavailableDetail: "Die Sitzung läuft ohne sie.",
  entryOff: "Team-Werkzeuge aus",
  entryCommandTitle:
    "Das Programm, das der Harness als MCP-Server {server} startet",

  messages: "Nachrichten",
  noMessages: "Noch keine Nachrichten.",
  unread: "ungelesen",
  mailHeld: "zurückgehalten",
  mailUndeliverable: "nicht zustellbar",
  savedUnread: "Gespeichert, aber niemand liest es: {reason}",
  writePlaceholder: "Schreibe der Sitzung… (⌘↩ sendet)",
  send: "Senden",

  title: "Titel",
  description: "Beschreibung",
  acceptance: "Abnahmekriterien",
  acceptancePlaceholder: "Woran erkennst du, dass die Karte fertig ist?",
  role: "Rolle",
  level: "Stufe",
  needsFirst: "Braucht zuerst",
  noCandidates: "Der Plan hat keine andere Karte, auf die diese warten könnte.",
  save: "Speichern",
  addCard: "Karte hinzufügen",
  cancel: "Abbrechen",
  cardInProgress:
    "Die Karte ist schon in Arbeit oder fertig; hier lässt sie sich nicht mehr ändern.",
  noColumn: "Das Board hat keine Spalte dafür.",

  legend: "Farben",
  legendIdle: "bereit / wartet",
  legendActive: "in Arbeit",
  legendQuestion: "Frage",
  legendReview: "in Prüfung",
  legendDone: "geprüft / im Plan",
  legendFailed: "fehlgeschlagen",
  noPlanYet: "Noch kein Plan",
  noPlanYetText:
    "Lege einen unter Planung an; seine Karten erscheinen hier als Graph.",
  noCardsYet: "Der Plan hat noch keine Karten",
  noCardsYetText: "Der Planer legt sie als Vorschläge an.",
  graphLabel: "Graph des Plans {name}",
  startLabel: "Start des Plans",
  start: "Start",
  goalLabel: "Ziel des Plans: {label}",
  goal: "Ziel",
  reviewNodeLabel: "Prüfung von Karte {id}, {state}",
  cardNodeLabel: "Karte {id}, {state}",
  reviewTag: "PRÜFUNG",
  cardKind: "Karte",
  card: "Karte",
  roleLine: "Rolle {role}",
  needsFirstList: "braucht zuerst: {ids}",
  openSession: "Sitzung öffnen: {name}",
  gaveUp: "Passt zweimal nicht in den Plan; das Studio hat aufgegeben.",
  integrateAgain: "Erneut integrieren",
  redo: "Neu machen",
  history: "Verlauf",
  clickCard:
    "Klicke eine Karte an, um ihren Verlauf und ihre Sitzungen zu sehen.",
  cardIntegrated: "Karte #{id} ist in den Plan integriert.",
  cardStillConflicts:
    "Karte #{id} passt immer noch nicht in den Plan ({files}).",
  cardWorkerMoving:
    "Der Bearbeiter von Karte #{id} ist noch in Bewegung; versuche es gleich noch einmal.",
  cardRedone: "Karte #{id} wird auf dem neuen Stand des Plans neu gemacht.",

  plans: "Pläne",
  board: "Board",
  newPlan: "Neuer Plan…",
  name: "Name",
  planName: "Name des Plans",
  goalOfPlan: "Ziel des Plans",
  goalPlaceholder: "Was soll am Ende da sein?",
  create: "Anlegen",
  noPlanOnBoard: "Noch kein Plan auf diesem Board.",
  resetTitle:
    "Setzt Planung, Agenten und die Flow-Ansicht an ihre Plätze zurück",
  resetLayout: "Layout zurücksetzen",
  plan: "Plan",
  noBoard: "Es gibt noch kein Board. Lege eines im Kanban an.",
  pickPlan:
    "Wähle einen Plan oder lege einen an: du schreibst das Ziel, der Planer zerlegt es in Karten.",
  runsByItself: "läuft von selbst",
  closeTitle:
    "Plan schließen: nichts startet mehr daraus, und seine Linie wird entfernt",
  closeBlocked:
    "Eine Sitzung arbeitet noch an einer Karte des Plans: lass sie fertig werden oder gib sie erst frei",
  close: "Schließen",
  reallyDelete: "Wirklich löschen",
  delete: "Löschen",
  deleteTitle: "Plan löschen; seine Karten bleiben ohne Plan auf dem Board",
  deleteBlocked: "Der Plan läuft noch: übernimm oder schließe ihn zuerst",
  origin:
    "Das Ziel von Karte #{id} auf dem Board. Nach der Freigabe laufen seine Schritte von selbst, und die Karte folgt " +
    "ihnen — nach der Prüfung, wenn alles auf deine Übernahme wartet.",
  costLimit: "Kostengrenze (USD)",
  noLimit: "keine",
  costInvalid:
    "Die Kostengrenze ist ein positiver Betrag in USD, oder leer für keine.",
  suggestedGoal: "Vorgeschlagenes Ziel",
  sharpenedTitle: "Geschärftes Ziel vorgeschlagen",
  sharpenedText:
    "Aus dem Gespräch mit der Grill-Sitzung. Es ersetzt dein Ziel erst, wenn du es übernimmst.",
  takeOver: "Übernehmen",
  discard: "Verwerfen",
  plannerIs: "Planer:",
  grillIs: "Grill:",
  showPane: "Pane zeigen",
  plannerRole: "Rolle des Planers",
  grillRole: "Rolle fürs Grillen",
  grillAfter: "Danach grillen",
  needsGoal: "Der Planer braucht ein Ziel.",
  startPlanner: "Planer starten",
  grillNoEngine:
    "Wähle eine Engine für die Grill-Rolle, sonst startet das Grillen nicht.",
  grillAfterPlanner:
    "Das Grillen startet, sobald der Planer seine Karten vorgeschlagen hat und auf dich wartet.",
  grillSoon:
    "Das Grillen startet gleich: die Karten sind da, der Planer läuft nicht mehr.",
  grillAfterPlanning:
    "Nach dem Planen befragt dich eine Grill-Sitzung zu den Karten.",
  proposals: "Vorschläge",
  proposalsCount: "Vorschläge ({n})",
  plannerNothingYet: "Der Planer hat noch nichts vorgeschlagen.",
  noProposals: "Keine Vorschläge warten auf dein Ja.",
  changedTitle: "Der Planer hat diesen Vorschlag nach dem Anlegen geändert",
  changedByPlanner: "vom Planer geändert",
  roleOf: "Rolle von {title}",
  levelOf: "Stufe von {title}",
  accept: "Annehmen",
  edit: "Bearbeiten",
  reallyDiscard: "Wirklich verwerfen",
  addCardRunning: "Karte zum laufenden Plan hinzufügen",
  releasePlan: "Plan freigeben",
  runAutomatically: "Automatisch abarbeiten",
  autoStartText:
    "Karten starten von selbst, sobald ihre Vorgänger fertig sind, so viele gleichzeitig wie möglich; die Prüfung " +
    "läuft von selbst.",
  manualText:
    "Die Vorschläge wandern in die erste offene Spalte; jede Karte startest du selbst.",
  plannerDoneAfter: "Der Planer ist danach fertig.",
  autoMode: "Automodus",
  autoRightsOn:
    "Die Bearbeiter des Plans dürfen jeden Befehl ausführen und ohne Nachfrage bearbeiten. Ein Push, die CLI, die " +
    "Datenbank und Config-Dateien bleiben in ihren schlichten Formen gesperrt — Muster, keine Sandbox: schalte es " +
    "für Karten ein, die du gelesen hast. Reviewer dürfen ebenfalls jeden Befehl ausführen, bekommen aber keine " +
    "Bearbeitungswerkzeuge; den Plan zu übernehmen bleibt deine Sache.",
  autoRightsOff:
    "Die Bearbeiter des Plans fragen vor Befehlen, die ihre Rolle nicht erlaubt.",
  notGrilled:
    "Dieser Plan wurde nicht gegrillt: sein Ziel wurde noch nicht hinterfragt. Freigeben kannst du ihn trotzdem.",
  spend: "Ausgaben: {line}",
  paused: "Pausiert: {reason}.",
  continue: "Weiter",
  continueText:
    "Setzt die Grenze des Plans auf die bisherigen Ausgaben plus ein neues Budget.",
  dayCapBefore:
    "Tagesgrenze der Studio-Sitzungen erreicht: {reason}. Bis morgen startet nichts Neues; anheben:",
  dayCapAfter: "in der Config.",
  today: "Heute: {spent} von {cap}",
  autoModeRunning:
    "Alles außer einem Push (durch Muster gesperrt, keine Sandbox), ohne Nachfrage. Gilt für Sitzungen, die ab jetzt " +
    "starten; laufende behalten ihre Rechte — um eine zu stoppen, stoppe ihre Karte.",
  takeOverPlan: "Plan übernehmen",
  takeOverText:
    "Alle Karten liegen auf der Linie des Plans. Die Arbeit geht in den Haupt-Branch des Projekts, ihre Commits " +
    "bleiben getrennt, solange sich der Branch nicht bewegt hat. Nichts wird gepusht; ein Konflikt wird zurückgerollt.",
  conflictMain: "Konflikt mit dem Haupt-Branch, nichts wurde geändert: {files}",
  cardsOfPlan: "Karten des Plans",
  plannerReading: "Der Planer {name} liest das Projekt.",
  grillQuestioning: "{name} befragt dich jetzt im Terminal zu den Karten.",
  grillFailed:
    "Das Grillen ließ sich nicht starten; setze den Haken noch einmal.",
  autoModeGone:
    "Der Plan ist geschlossen oder weg; sein Automodus wurde nicht geändert.",
  autoOn:
    "Automodus an: Sitzungen, die ab jetzt starten, dürfen alles außer pushen.",
  autoOff: "Automodus aus: Sitzungen, die ab jetzt starten, fragen wieder.",
  noLongerDraft: "Der Plan ist kein Entwurf mehr.",
  releasedWorking: {
    one: "Plan freigegeben: an {n} Karte wird gearbeitet.",
    other: "Plan freigegeben: an {n} Karten wird gearbeitet.",
  },
  releasedWaiting: {
    one: "Plan freigegeben: {n} Karte wartet in der ersten offenen Spalte.",
    other: "Plan freigegeben: {n} Karten warten in der ersten offenen Spalte.",
  },
  takenOver: {
    one: "Plan „{name}“ wurde übernommen ({commit}): {n} Karte.",
    other: "Plan „{name}“ wurde übernommen ({commit}): {n} Karten.",
  },
  notCleaned: "Nicht aufgeräumt: {note}",
  resumed:
    "Plan „{name}“ läuft weiter: eine neue Grenze wurde auf die bisherigen Ausgaben gesetzt.",
  deleted:
    "Plan „{name}“ wurde gelöscht; seine Karten bleiben ohne Plan auf dem Board.",
  closeConfirm:
    "Plan „{name}“ schließen? Nichts startet mehr daraus, und seine Linie wird entfernt; Arbeit auf der Linie, die " +
    "nicht übernommen wurde, bleibt auf ihrem Branch.",
  closedNote: "Plan „{name}“ wurde geschlossen. {notes}",
  noLongerProposal: "Die Karte ist kein Vorschlag mehr und bleibt.",

  usageSteps: "Schritte",
  usageTokens: "Tokens",
  usageCost: "Kosten",
  unmeasuredNoSessionId:
    "Für diese Sitzung wurde keine Claude-Code-Sitzung aufgezeichnet (sie lief mit einem eigenen Befehl).",
  unmeasuredNoRecordYet:
    "Die Sitzung hat noch nichts aufgezeichnet — vielleicht ist sie gerade erst gestartet.",
  unmeasuredReadFailed:
    "Die Aufzeichnung der Sitzung konnte nicht gelesen werden.",
  unmeasuredNotMeasured: "Dieser Harness wird nicht gemessen.",
  unmeasuredLimit:
    "{why} Eine Grenze gilt erst, wenn die Sitzung gemessen werden kann.",
  notPushedAhead:
    "Noch nicht gepusht: ↑{ahead} nach {target}. Pushen im Git-Reiter des Studios.",
  theUpstream: "den Upstream",
  notPushedNoUpstream:
    "Noch nicht gepusht: Der Branch hat noch keinen Upstream. Pushen im Git-Reiter des Studios legt ihn an.",
});
