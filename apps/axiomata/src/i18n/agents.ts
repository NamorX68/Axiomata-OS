/**
 * The texts of the Studio's agents, models and roles: the Agents tab (`ide/AgentsPanel.svelte`,
 * `ide/AgentStart.svelte`), the Models & roles pane (`ide/panes/RosterPane.svelte`, `ide/ModelsSection.svelte`,
 * `ide/RolesSection.svelte`, `ide/EnginesSection.svelte` and its notices), an agent's pane
 * (`ide/panes/AgentPane.svelte`) and the helpers whose words these show (`ide/agentStatus.ts`, `ide/models.ts`,
 * `ide/agentsMigration.ts`, `ide/rosterLook.ts`, `ide/agents.ts`, `ide/agentRemoval.ts`). Harness, model and role
 * names, the tiers Light/Medium/Heavy and a session's name ("Glut Medium Allrounder (Sonnet 5.5)") are names and data
 * and stay as they are (`docs/plans/i18n.md`, Q13).
 */

import { texts, type Shape } from "./texts";

const en = {
  // An agent's status (`agentStatus.ts`): the word beside its dot and the tooltip.
  statusStarting: "starting",
  statusStartingTitle: "Started — the harness has not reported yet",
  statusIdle: "idle",
  statusIdleTitle: "Finished its turn, ready for the next prompt",
  statusWorking: "working",
  statusWorkingTitle: "In the middle of a turn",
  statusWaiting: "waiting",
  statusWaitingTitle: "Waiting for you — a permission prompt or a question",
  statusEnded: "ended",
  statusEndedTitle: "The harness has exited",
  statusNone: "no status",
  statusNoneTitle:
    "No status channel (own command). For Claude Code, add {flag} to the command.",
  statusClosed: "closed",
  statusClosedTitle: "No pane is open for this agent",

  // Shared words.
  agents: "Agents",
  name: "Name",
  role: "Role",
  engine: "Engine",
  tier: "Tier",
  model: "Model",
  save: "Save",
  saving: "Saving…",
  cancel: "Cancel",
  loading: "Loading…",
  advanced: "Advanced",
  modelsAndRoles: "Models & roles",
  defaultModel: "default model",
  checkId:
    "ID: lower-case letters and digits, with - or _ between them (up to 48 characters).",
  checkLabelEmpty: "Label must not be empty.",
  checkLabelLong: "Label: up to 80 characters.",
  checkEnv: "Environment: “{line}” is not KEY=value.",
  checkName:
    "Name: lower-case letters and digits, with - or _ between them (up to 48 characters).",
  checkKind: "Kind: one lower-case word, e.g. implement, review, plan.",
  checkDescription: "Description: one line, up to 200 characters.",
  checkNoEngine: "There is no engine “{id}”.",
  checkFallbackIsDefault: "“{id}” is already the default engine.",
  checkCardKind: "Card kind “{kind}”: one lower-case word.",
  checkCost: "Cost limit: a positive number or empty.",
  checkTokens: "Token limit: a positive whole number or empty.",
  checkSteps: "Step limit: a positive whole number or empty.",
  local: "local",
  pricePerMOut: "{price} / M out",
  subscription: "subscription",
  metered: "metered",
  tierHintLight: "Small, well-specified changes",
  tierHintMedium: "The normal case",
  tierHintHeavy: "Large or delicate changes; where escalation ends",
  billingMetered: "Per token",
  billingMeteredHint: "Limits in dollars",
  billingSubscription: "Subscription",
  billingSubscriptionHint: "Limits in tokens (Claude Code on the account)",

  // The Agents tab (`AgentsPanel.svelte`).
  masterPanes: "Master panes",
  masterOpencodeTitle:
    "Opencode as you run it yourself — your settings, your models, in the project folder; no team tools",
  masterTitle:
    "{harness} as you use it outside — your settings and rules, in the project folder, with the team tools. " +
    "Click again while it is open for one more beside it",
  removeMaster: "Remove the {harness} master",
  removeMasterAsk:
    "Remove the {harness} master? Panes already running it stay open.",
  favorites: "Favorites",
  startNamed: "Start {name}",
  unavailable: "unavailable",
  tierDefault: "tier default",
  removeFavoriteNamed: "Remove {name} from the favorites",
  branchTitle: "Checked out in its worktree",
  stepsDone: "{done} of {total} steps done",
  lastState: "last",
  removeAgentFavorite: "Remove this agent from the favorites",
  keepAgentFavorite: "Keep this agent as a favorite",
  editNamed: "Edit {name}",
  removeNamed: "Remove {name}",
  removeAgentAsk:
    "Remove the agent “{name}”? Its worktree and branch go with it; panes already running it stay open.",
  unsavedWorkQuestion: "Unsaved work in the worktree — delete anyway?",
  sessions: "Sessions",
  waitingForYou: "Waiting for you: take over or discard",
  openProjectForAgents: "Open a project to work with agents.",
  noSession: "No session runs or waits for you.",
  modelsAndRolesMore: "Models & roles…",
  editAgent: "Edit agent",
  pickEngine: "Engine…",
  editHint:
    "Another role or engine for this session only. Models, engines and roles are made and changed under",

  // "Start an agent" (`AgentStart.svelte`).
  startAgent: "Start an agent",
  enginesUnreadable: "Could not load the engines: {error}",
  tierDefaultEngine: "Default of the tier",
  noEngineOfTier: "no engine of this tier can run",
  standard: "standard",
  start: "Start",
  starting: "Starting…",
  removeChoiceFavorite: "Remove this choice from the favorites",
  keepChoiceFavorite: "Keep this choice as a favorite",
  pickRoleHint: "Pick a role, then its tier and model below.",

  // The Models & roles pane (`panes/RosterPane.svelte`).
  modelsOrRoles: "Models or roles",
  models: "Models",
  roles: "Roles",
  chainModel: "Model",
  chainModelText:
    "= what thinks, at a tier (light · medium · heavy), on the harness that runs it ·",
  chainRole: "Role",
  chainRoleText:
    "= what it is for (instructions, rights, limits, a default tier) ·",
  chainAgent: "Agent",
  chainAgentText:
    "= a role at a tier on an engine, started as a session with its own branch.",

  // The Models tab (`ModelsSection.svelte`, `models.ts`).
  modelsUnreadable: "Could not load the models: {error}",
  researching: "Researching model tiers…",
  researchingBefore: "The",
  researchingAfter:
    "skill looks the models up on rating sites and proposes a tier for each. Its proposals appear here as " +
    "“not reviewed”; your own tier always wins.",
  defaultEnginePerTier: "Default engine per tier",
  defaultEngineHint:
    "What a session of a tier starts on. Without a choice the cheapest engine of the tier runs; when one cannot " +
    "run, the next of the same tier takes over.",
  cheapestNamed: "Cheapest: {engine}",
  cheapestOfTier: "Cheapest of the tier",
  notOfThisTier: "{engine} — not of this tier",
  priceLimits: "Price limits",
  priceLimitsHint:
    "The tier a model's output price proposes until research or you say otherwise (USD per million output tokens).",
  lightBelow: "Light below",
  heavyAbove: "Heavy above",
  saveLimits: "Save limits",
  proposeAllAgain: "Propose all again",
  modelsHint:
    "Each model that is on is an engine of its harness. Its tier comes from you, else from research, else from its " +
    "price; a model without any is no agent yet.",
  modelsOn: "{on} of {total} on",
  filterModels: "Filter {n} models…",
  switchOff: "Switch off",
  switchOn: "Switch on",
  modelOn: "{name} on",
  tierOf: "Tier of {name}",
  yourTierDrop: "Your tier — click to drop it",
  setYourTier: "Set {tier} as your tier",
  waitsForReview: "Waits for your review",
  proposeAgainFor: "Propose again for {name}",
  acceptTier: "Accept {tier}",
  noModelMatches: "No model matches “{query}”.",
  ownEngines: "Own engines",
  ownEnginesHint:
    "An engine of your own: a command or an environment the automatic ones do not have. Automatic engines are not " +
    "edited here — switch their model on or off above.",
  originYours: "Yours",
  originYoursProposal: "Yours · the proposal says {tier}",
  originResearch: "Proposal · not reviewed",
  originPrice: "From the price",
  originUnrated: "Unrated · no agent until it has a tier",
  noPrice: "no price",
  contextK: "{n}K context",
  contextM: "{n}M context",
  limitsNotPrices: "Both limits are prices in USD per million output tokens.",
  limitsOrder: "“Light below” must not be above “heavy above”.",
  researchNone: "No model is switched on to research.",
  researchOne: "Researching the model's tier…",
  researchMany: "Researching the tiers of {n} models…",

  // The Roles tab (`RolesSection.svelte`, `MigrationNotice.svelte`).
  rolesUnreadable: "Could not load the roles: {error}",
  roleExists: "There is already a role “{name}”.",
  deleteRoleAsk: "Delete the role “{name}”? Its AGENT.md file is removed.",
  addRole: "Add role…",
  noRoles: "No roles yet.",
  defaultTierNamed: "default tier {tier}",
  deleteNamed: "Delete {name}",
  skippedRole: "Skipped:",
  allRoles: "← All roles",
  newRole: "New role",
  roleNamePlaceholder: "e.g. Reviewer light",
  idAndFile: "ID and file name:",
  description: "Description",
  kindOfWork: "Kind of work",
  defaultTier: "Default tier",
  instructions: "Working instructions (Markdown)",
  rights: "Rights (harness rules, one per line)",
  sessionLimits: "Limits of a session (empty = the tier's default)",
  costLimit: "Cost limit (USD)",
  tokenLimit: "Token limit",
  stepLimit: "Step limit",
  emptyIsDefault: "empty = default",
  createsCards: "Creates cards without asking (kinds, comma-separated)",
  createsPlaceholder: "e.g. test, doc",
  saveRole: "Save role",
  rolesHintBefore:
    "A role says what an agent is for: kind of work, default tier, rights, limits and the working instructions — " +
    "no engine: the tier's engine runs (Models tab). It lives as a file at",
  rolesHintAfter:
    "and can never start a program itself. Pick one on the left to change it, or add a new one.",
  migrationFailed: "The move to roles at tiers did not run",
  migrationBackupBefore: "The originals are in",
  migrationBackupAfter: "; delete it in Settings → Storage.",
  migrationLeft: "Left as they were: {items}.",
  migrationDefaults: "Default engines per tier: {items}.",
  ok: "OK",
  migMoved: { one: "{n} copied role moved", other: "{n} copied roles moved" },
  migAdded: "{names} added",
  migEngines: {
    one: "{n} engine now automatic",
    other: "{n} engines now automatic",
  },
  migSessions: {
    one: "{n} session moved along",
    other: "{n} sessions moved along",
  },
  migTidied: "Roles and engines tidied: {parts}.",
  migAlreadyTidy: "Roles and engines were already tidy.",

  // A project's own roles (`ProjectRolesNotice.svelte`).
  projectRolesError: "Project roles: {error}",
  limitTokens: "{n} tokens",
  limitSteps: "{n} steps",
  factDescription: "description: {text}",
  factKindTier: "kind: {kind}, tier: {tier}",
  factNamesEngine:
    "names an engine ({engines}) — ignored: the engine of the tier runs",
  factCreates: "creates cards without asking: {kinds}",
  factRights:
    "rights, granted without asking in sessions started for a card: {rights}",
  factLimits: "limits: {limits}",
  factNoLimits: "limits: none of its own",
  projectRolesApply: "Roles from this project apply (confirmed): {roles}.",
  noneReadable: "none that could be read",
  allowRoles: "Allow these roles",
  projectRolesReview: "Project roles need your review",
  projectRolesBefore: "This project has its own agent roles in",
  projectRolesAfter:
    "Their instructions are what an agent will obey — and the rights they list are granted without asking in a " +
    "session started for a card, where nobody watches each step — so they stay off until you have read them.",
  review: "Review…",
  cannotConfirm: "They cannot be confirmed: {reason}.",
  skippedNamed: "Skipped",
  replacesYours: {
    one: "Replaces your own role: {names}.",
    other: "Replaces your own roles: {names}.",
  },
  namesEngine: {
    one:
      "{names} names an engine. A role chooses no engine any more, so it is ignored: " +
      "the engine of the role's tier runs.",
    other:
      "{names} name an engine. A role chooses no engine any more, so it is ignored: " +
      "the engine of the role's tier runs.",
  },

  // Engines (`EnginesSection.svelte`, `agents.ts`).
  claudeCodeHint: "Anthropic's CLI",
  glutHint: "Axiomata's own harness: sandbox, network gate, refusal limits",
  agentCount: { one: "{n} agent", other: "{n} agents" },
  engineExists: "There is already an engine “{id}”.",
  removeEngineAsk: "Remove the engine “{label}”?",
  engines: "Engines",
  addEngine: "Add engine…",
  noOwnEngines:
    "No own engines. The models that are on already make one engine each.",
  itsModelsTier: "{tier} (its model's)",
  unrated: "unrated",
  allEngines: "← All engines",
  newEngine: "New engine",
  label: "Label",
  labelPlaceholder: "e.g. Claude Code · Opus",
  harness: "Harness",
  billing: "Billing",
  tierOfModel: "The tier of its model",
  modelPlaceholder:
    "empty = the harness picks; Glut: openrouter/… or ollama/… picks the provider",
  idFixed: "ID (fixed)",
  id: "ID",
  command: "Command (empty = the harness default)",
  environment: "Environment",
  perLine: "per line",
  saveEngine: "Save engine",
  enginesHint:
    "An own engine is harness + model + environment of your choosing — the part that costs money and can be " +
    "unreachable — with a tier of its own if you give it one. Pick one on the left to change it, or add a new one.",

  // An agent's pane (`panes/AgentPane.svelte`).
  sideTerminal: "Terminal",
  sidePlan: "Plan",
  sideDiffs: "Diffs",
  sideInbox: "Inbox",
  notInsideFolder: "“{path}” is not inside this agent's folder.",
  asksPermission: "{name} asks for a permission",
  preparingWorktree: "Preparing this agent's worktree…",
  tasks: "Tasks",
  tasksDone: "{done} / {total} done",
  fromEarlierSession:
    "From an earlier session — the agent has not written new tasks since it started.",
  noTasksYet:
    "No tasks yet — they appear once the agent starts working on the plan below.",
  planModeDoc: "Plan (plan mode) · {name}",
  earlierSession: "earlier session",
  noPlanYet: "No plan yet. It appears here as soon as the agent writes one.",
  agentViews: "{name} views",
  sharedFolderTitle:
    "The project is not a git repository, so agents share its folder",
  sharedFolder: "shared folder",
  restartAgent: "Restart the agent",

  // The role picker of a planner or grilling start (`panes/SessionPicker.svelte`).
  engineFor: "Engine for “{role}”",
  savedInRole: "(saved in the role)",
  engineForLabel: "Engine for {role}",
  engineBelongsToRole:
    "The engine belongs to the role; change it under Models & roles",
  runsOn: "runs on {engine}",
};

const de: Shape<typeof en> = {
  statusStarting: "startet",
  statusStartingTitle: "Gestartet — der Harness hat sich noch nicht gemeldet",
  statusIdle: "bereit",
  statusIdleTitle: "Schritt fertig, bereit für den nächsten Prompt",
  statusWorking: "arbeitet",
  statusWorkingTitle: "Mitten in einem Schritt",
  statusWaiting: "wartet",
  statusWaitingTitle: "Wartet auf dich — eine Freigabe oder eine Frage",
  statusEnded: "beendet",
  statusEndedTitle: "Der Harness hat sich beendet",
  statusNone: "kein Status",
  statusNoneTitle:
    "Kein Statuskanal (eigener Befehl). Für Claude Code ergänze {flag} im Befehl.",
  statusClosed: "geschlossen",
  statusClosedTitle: "Für diesen Agenten ist kein Pane offen",

  agents: "Agenten",
  name: "Name",
  role: "Rolle",
  engine: "Engine",
  tier: "Stufe",
  model: "Modell",
  save: "Speichern",
  saving: "Speichere…",
  cancel: "Abbrechen",
  loading: "Lade…",
  advanced: "Erweitert",
  modelsAndRoles: "Modelle & Rollen",
  defaultModel: "Standardmodell",
  checkId:
    "ID: Kleinbuchstaben und Ziffern, mit - oder _ dazwischen (bis 48 Zeichen).",
  checkLabelEmpty: "Die Bezeichnung darf nicht leer sein.",
  checkLabelLong: "Bezeichnung: bis 80 Zeichen.",
  checkEnv: "Umgebung: „{line}“ ist nicht KEY=value.",
  checkName:
    "Name: Kleinbuchstaben und Ziffern, mit - oder _ dazwischen (bis 48 Zeichen).",
  checkKind: "Art: ein kleingeschriebenes Wort, z. B. implement, review, plan.",
  checkDescription: "Beschreibung: eine Zeile, bis 200 Zeichen.",
  checkNoEngine: "Es gibt keine Engine „{id}“.",
  checkFallbackIsDefault: "„{id}“ ist schon die Standard-Engine.",
  checkCardKind: "Kartenart „{kind}“: ein kleingeschriebenes Wort.",
  checkCost: "Kostengrenze: eine positive Zahl oder leer.",
  checkTokens: "Token-Grenze: eine positive ganze Zahl oder leer.",
  checkSteps: "Schrittgrenze: eine positive ganze Zahl oder leer.",
  local: "lokal",
  pricePerMOut: "{price} / M Ausgabe",
  subscription: "Abo",
  metered: "nach Verbrauch",
  tierHintLight: "Kleine, klar umrissene Änderungen",
  tierHintMedium: "Der Normalfall",
  tierHintHeavy: "Große oder heikle Änderungen; hier endet die Eskalation",
  billingMetered: "Pro Token",
  billingMeteredHint: "Grenzen in Dollar",
  billingSubscription: "Abo",
  billingSubscriptionHint: "Grenzen in Tokens (Claude Code über das Konto)",

  masterPanes: "Master-Panes",
  masterOpencodeTitle:
    "Opencode, wie du es selbst startest — deine Einstellungen, deine Modelle, im Projektordner; ohne Team-Werkzeuge",
  masterTitle:
    "{harness}, wie du es außerhalb nutzt — deine Einstellungen und Regeln, im Projektordner, mit den " +
    "Team-Werkzeugen. Klicke erneut, während es offen ist, für ein weiteres daneben",
  removeMaster: "{harness}-Master entfernen",
  removeMasterAsk:
    "{harness}-Master entfernen? Panes, in denen er schon läuft, bleiben offen.",
  favorites: "Favoriten",
  startNamed: "{name} starten",
  unavailable: "nicht verfügbar",
  tierDefault: "Standard der Stufe",
  removeFavoriteNamed: "{name} aus den Favoriten entfernen",
  branchTitle: "In seinem Worktree ausgecheckt",
  stepsDone: "{done} von {total} Schritten erledigt",
  lastState: "zuletzt",
  removeAgentFavorite: "Diesen Agenten aus den Favoriten entfernen",
  keepAgentFavorite: "Diesen Agenten als Favoriten behalten",
  editNamed: "{name} bearbeiten",
  removeNamed: "{name} entfernen",
  removeAgentAsk:
    "Den Agenten „{name}“ entfernen? Sein Worktree und Branch gehen mit; " +
    "Panes, in denen er schon läuft, bleiben offen.",
  unsavedWorkQuestion: "Ungespeicherte Arbeit im Worktree — trotzdem löschen?",
  sessions: "Sitzungen",
  waitingForYou: "Wartet auf dich: übernehmen oder verwerfen",
  openProjectForAgents: "Öffne ein Projekt, um mit Agenten zu arbeiten.",
  noSession: "Keine Sitzung läuft oder wartet auf dich.",
  modelsAndRolesMore: "Modelle & Rollen…",
  editAgent: "Agenten bearbeiten",
  pickEngine: "Engine…",
  editHint:
    "Eine andere Rolle oder Engine nur für diese Sitzung. Modelle, Engines und Rollen legst du an und änderst du unter",

  startAgent: "Agenten starten",
  enginesUnreadable: "Die Engines ließen sich nicht laden: {error}",
  tierDefaultEngine: "Standard der Stufe",
  noEngineOfTier: "keine Engine dieser Stufe kann laufen",
  standard: "Standard",
  start: "Starten",
  starting: "Starte…",
  removeChoiceFavorite: "Diese Wahl aus den Favoriten entfernen",
  keepChoiceFavorite: "Diese Wahl als Favoriten behalten",
  pickRoleHint: "Wähle eine Rolle, darunter dann ihre Stufe und ihr Modell.",

  modelsOrRoles: "Modelle oder Rollen",
  models: "Modelle",
  roles: "Rollen",
  chainModel: "Modell",
  chainModelText:
    "= was denkt, auf einer Stufe (light · medium · heavy), im Harness, der es ausführt ·",
  chainRole: "Rolle",
  chainRoleText:
    "= wofür es da ist (Anweisungen, Rechte, Grenzen, eine Standardstufe) ·",
  chainAgent: "Agent",
  chainAgentText:
    "= eine Rolle auf einer Stufe auf einer Engine, gestartet als Sitzung mit eigenem Branch.",

  modelsUnreadable: "Die Modelle ließen sich nicht laden: {error}",
  researching: "Stufen der Modelle werden recherchiert…",
  researchingBefore: "Der Skill",
  researchingAfter:
    "schlägt die Modelle auf Bewertungsseiten nach und schlägt für jedes eine Stufe vor. Seine Vorschläge erscheinen " +
    "hier als „nicht geprüft“; deine eigene Stufe gewinnt immer.",
  defaultEnginePerTier: "Standard-Engine je Stufe",
  defaultEngineHint:
    "Worauf eine Sitzung einer Stufe startet. Ohne Wahl läuft die günstigste Engine der Stufe; kann eine nicht " +
    "laufen, übernimmt die nächste derselben Stufe.",
  cheapestNamed: "Günstigste: {engine}",
  cheapestOfTier: "Günstigste der Stufe",
  notOfThisTier: "{engine} — nicht von dieser Stufe",
  priceLimits: "Preisgrenzen",
  priceLimitsHint:
    "Die Stufe, die der Ausgabepreis eines Modells vorschlägt, bis Recherche oder du etwas anderes sagen " +
    "(USD pro Million Ausgabe-Tokens).",
  lightBelow: "Light unter",
  heavyAbove: "Heavy über",
  saveLimits: "Grenzen speichern",
  proposeAllAgain: "Alle neu vorschlagen",
  modelsHint:
    "Jedes eingeschaltete Modell ist eine Engine seines Harness. Seine Stufe kommt von dir, sonst aus der " +
    "Recherche, sonst aus seinem Preis; ein Modell ohne Stufe ist noch kein Agent.",
  modelsOn: "{on} von {total} an",
  filterModels: "{n} Modelle filtern…",
  switchOff: "Ausschalten",
  switchOn: "Einschalten",
  modelOn: "{name} an",
  tierOf: "Stufe von {name}",
  yourTierDrop: "Deine Stufe — klicke, um sie zu entfernen",
  setYourTier: "{tier} als deine Stufe setzen",
  waitsForReview: "Wartet auf deine Prüfung",
  proposeAgainFor: "Für {name} neu vorschlagen",
  acceptTier: "{tier} übernehmen",
  noModelMatches: "Kein Modell passt zu „{query}“.",
  ownEngines: "Eigene Engines",
  ownEnginesHint:
    "Eine eigene Engine: ein Befehl oder eine Umgebung, die die automatischen nicht haben. Automatische Engines " +
    "bearbeitest du nicht hier — schalte ihr Modell oben an oder aus.",
  originYours: "Deine",
  originYoursProposal: "Deine · der Vorschlag sagt {tier}",
  originResearch: "Vorschlag · nicht geprüft",
  originPrice: "Aus dem Preis",
  originUnrated: "Ohne Stufe · kein Agent, bis es eine Stufe hat",
  noPrice: "kein Preis",
  contextK: "{n}K Kontext",
  contextM: "{n}M Kontext",
  limitsNotPrices:
    "Beide Grenzen sind Preise in USD pro Million Ausgabe-Tokens.",
  limitsOrder: "„Light unter“ darf nicht über „Heavy über“ liegen.",
  researchNone: "Kein Modell ist zum Recherchieren eingeschaltet.",
  researchOne: "Die Stufe des Modells wird recherchiert…",
  researchMany: "Die Stufen von {n} Modellen werden recherchiert…",

  rolesUnreadable: "Die Rollen ließen sich nicht laden: {error}",
  roleExists: "Es gibt schon eine Rolle „{name}“.",
  deleteRoleAsk:
    "Die Rolle „{name}“ löschen? Ihre AGENT.md-Datei wird entfernt.",
  addRole: "Rolle hinzufügen…",
  noRoles: "Noch keine Rollen.",
  defaultTierNamed: "Standardstufe {tier}",
  deleteNamed: "{name} löschen",
  skippedRole: "Übersprungen:",
  allRoles: "← Alle Rollen",
  newRole: "Neue Rolle",
  roleNamePlaceholder: "z. B. Reviewer light",
  idAndFile: "ID und Dateiname:",
  description: "Beschreibung",
  kindOfWork: "Art der Arbeit",
  defaultTier: "Standardstufe",
  instructions: "Arbeitsanweisungen (Markdown)",
  rights: "Rechte (Harness-Regeln, eine pro Zeile)",
  sessionLimits: "Grenzen einer Sitzung (leer = Standard der Stufe)",
  costLimit: "Kostengrenze (USD)",
  tokenLimit: "Token-Grenze",
  stepLimit: "Schrittgrenze",
  emptyIsDefault: "leer = Standard",
  createsCards: "Legt Karten an, ohne zu fragen (Arten, durch Komma getrennt)",
  createsPlaceholder: "z. B. test, doc",
  saveRole: "Rolle speichern",
  rolesHintBefore:
    "Eine Rolle sagt, wofür ein Agent da ist: Art der Arbeit, Standardstufe, Rechte, Grenzen und die " +
    "Arbeitsanweisungen — keine Engine: die Engine der Stufe läuft (Reiter Modelle). Sie liegt als Datei unter",
  rolesHintAfter:
    "und kann selbst nie ein Programm starten. Wähle links eine aus, um sie zu ändern, oder füge eine neue hinzu.",
  migrationFailed: "Die Umstellung auf Rollen mit Stufen ist nicht gelaufen",
  migrationBackupBefore: "Die Originale liegen in",
  migrationBackupAfter: "; lösche den Ordner unter Einstellungen → Speicher.",
  migrationLeft: "Unverändert gelassen: {items}.",
  migrationDefaults: "Standard-Engines je Stufe: {items}.",
  ok: "OK",
  migMoved: {
    one: "{n} kopierte Rolle verschoben",
    other: "{n} kopierte Rollen verschoben",
  },
  migAdded: "{names} hinzugefügt",
  migEngines: {
    one: "{n} Engine jetzt automatisch",
    other: "{n} Engines jetzt automatisch",
  },
  migSessions: {
    one: "{n} Sitzung mitgezogen",
    other: "{n} Sitzungen mitgezogen",
  },
  migTidied: "Rollen und Engines aufgeräumt: {parts}.",
  migAlreadyTidy: "Rollen und Engines waren schon aufgeräumt.",

  projectRolesError: "Projektrollen: {error}",
  limitTokens: "{n} Tokens",
  limitSteps: "{n} Schritte",
  factDescription: "Beschreibung: {text}",
  factKindTier: "Art: {kind}, Stufe: {tier}",
  factNamesEngine:
    "nennt eine Engine ({engines}) — ignoriert: die Engine der Stufe läuft",
  factCreates: "legt Karten an, ohne zu fragen: {kinds}",
  factRights:
    "Rechte, ohne Nachfrage gewährt in Sitzungen, die für eine Karte gestartet sind: {rights}",
  factLimits: "Grenzen: {limits}",
  factNoLimits: "Grenzen: keine eigenen",
  projectRolesApply: "Rollen aus diesem Projekt gelten (bestätigt): {roles}.",
  noneReadable: "keine lesbaren",
  allowRoles: "Diese Rollen erlauben",
  projectRolesReview: "Projektrollen brauchen deine Prüfung",
  projectRolesBefore: "Dieses Projekt hat eigene Agentenrollen in",
  projectRolesAfter:
    "Ihre Anweisungen befolgt ein Agent — und die Rechte, die sie nennen, gelten ohne Nachfrage in einer Sitzung, " +
    "die für eine Karte gestartet ist, wo niemand jeden Schritt sieht — " +
    "darum bleiben sie aus, bis du sie gelesen hast.",
  review: "Prüfen…",
  cannotConfirm: "Sie lassen sich nicht bestätigen: {reason}.",
  skippedNamed: "Übersprungen",
  replacesYours: {
    one: "Ersetzt deine eigene Rolle: {names}.",
    other: "Ersetzt deine eigenen Rollen: {names}.",
  },
  namesEngine: {
    one:
      "{names} nennt eine Engine. Eine Rolle wählt keine Engine mehr, darum wird sie ignoriert: die Engine der " +
      "Stufe der Rolle läuft.",
    other:
      "{names} nennen eine Engine. Eine Rolle wählt keine Engine mehr, darum wird sie ignoriert: die Engine der " +
      "Stufe der Rolle läuft.",
  },

  claudeCodeHint: "Die CLI von Anthropic",
  glutHint:
    "Axiomatas eigener Harness: Sandbox, Netz-Schranke, Grenzen für Ablehnungen",
  agentCount: { one: "{n} Agent", other: "{n} Agenten" },
  engineExists: "Es gibt schon eine Engine „{id}“.",
  removeEngineAsk: "Die Engine „{label}“ entfernen?",
  engines: "Engines",
  addEngine: "Engine hinzufügen…",
  noOwnEngines:
    "Keine eigenen Engines. Die eingeschalteten Modelle ergeben schon je eine Engine.",
  itsModelsTier: "{tier} (die seines Modells)",
  unrated: "ohne Stufe",
  allEngines: "← Alle Engines",
  newEngine: "Neue Engine",
  label: "Bezeichnung",
  labelPlaceholder: "z. B. Claude Code · Opus",
  harness: "Harness",
  billing: "Abrechnung",
  tierOfModel: "Die Stufe seines Modells",
  modelPlaceholder:
    "leer = der Harness wählt; Glut: openrouter/… oder ollama/… wählt den Anbieter",
  idFixed: "ID (fest)",
  id: "ID",
  command: "Befehl (leer = Standard des Harness)",
  environment: "Umgebung",
  perLine: "pro Zeile",
  saveEngine: "Engine speichern",
  enginesHint:
    "Eine eigene Engine ist Harness + Modell + Umgebung nach deiner Wahl — der Teil, der Geld kostet und " +
    "unerreichbar sein kann — mit einer eigenen Stufe, wenn du ihr eine gibst. Wähle links eine aus, um sie zu " +
    "ändern, oder füge eine neue hinzu.",

  sideTerminal: "Terminal",
  sidePlan: "Plan",
  sideDiffs: "Diffs",
  sideInbox: "Posteingang",
  notInsideFolder: "„{path}“ liegt nicht im Ordner dieses Agenten.",
  asksPermission: "{name} bittet um eine Freigabe",
  preparingWorktree: "Der Worktree dieses Agenten wird vorbereitet…",
  tasks: "Aufgaben",
  tasksDone: "{done} / {total} erledigt",
  fromEarlierSession:
    "Aus einer früheren Sitzung — der Agent hat seit seinem Start keine neuen Aufgaben geschrieben.",
  noTasksYet:
    "Noch keine Aufgaben — sie erscheinen, sobald der Agent am Plan unten arbeitet.",
  planModeDoc: "Plan (Plan-Modus) · {name}",
  earlierSession: "frühere Sitzung",
  noPlanYet:
    "Noch kein Plan. Er erscheint hier, sobald der Agent einen schreibt.",
  agentViews: "Ansichten von {name}",
  sharedFolderTitle:
    "Das Projekt ist kein Git-Repository, darum teilen sich die Agenten seinen Ordner",
  sharedFolder: "geteilter Ordner",
  restartAgent: "Agenten neu starten",

  engineFor: "Engine für „{role}“",
  savedInRole: "(wird in der Rolle gespeichert)",
  engineForLabel: "Engine für {role}",
  engineBelongsToRole:
    "Die Engine gehört zur Rolle; ändere sie unter Modelle & Rollen",
  runsOn: "läuft auf {engine}",
};

/** The lookup of the agents' texts. */
export const tAgents = texts("agents", en, de);
