/**
 * Refusals of the core in the owner's language (`docs/plans/i18n.md`, Q11): a coded error from Rust
 * (`axiomata_core::error::coded`) names its code and values; `core/errors.ts` `messageOf` writes it from here. A code
 * this catalog does not know shows the English sentence Rust sent with it, so a new refusal never shows a raw code.
 * The keys are the codes; a value names the code's values as `{name}`, with the names the Rust call gives them.
 * The values arrive as strings, so a text here never counts (no `{ one, other }`).
 */

import { texts } from "./texts";

const en = {
  // Ids that name nothing.
  "project.missing": "No project {id}.",
  "card.missing": "No card {id}.",
  "plan.missing": "No plan {id}.",
  "session.missing": "No session {id}.",

  // Engines (`roster.rs`, `card_session.rs`, `agent_start.rs`, `model_tiers.rs`).
  "engine.unknown": "There is no engine “{id}”.",
  "engine.unknownAddInStudio":
    "There is no engine “{id}”; engines are added in the Studio settings.",
  "engine.unknownSwitchOn":
    "There is no engine “{id}”; switch its model on in the Studio's Models settings, or add an own engine.",
  "engine.gone": "There is no engine “{id}” any more; pick another one.",
  "engine.unusable": "The engine “{id}” cannot run now: {why}",
  "engine.ownCommand":
    "The engine “{id}” runs a command of its own, which Axiomata does not touch; an unattended session needs the " +
    "generated command.",
  "engine.noneForTier":
    "No engine of the tier {tier} can run now; switch a model of that tier on in the Studio's Models tab.",
  "engine.noneForTierUnattended":
    "No engine of the tier {tier} can run an unattended session; switch a model of that tier on or set the tier's " +
    "default engine in the Studio's Models tab, or pick an engine when you start it.",
  "engine.automatic":
    "“{id}” is an automatic engine, made from a model of the catalog; it cannot be edited or removed — switch its " +
    "model on or off in the Studio's Models settings, or give an own engine another id.",
  "engine.inUseBySessions":
    "The engine “{id}” cannot be removed: agent sessions run on it ({n}).",
  "engine.inUseByRole":
    "The engine “{id}” cannot be removed: the role “{role}” uses it.",
  "model.badKey":
    "“{key}” is no model key (anthropic/…, openrouter/… or ollama/…).",

  // Roles.
  "role.unknown": "There is no role “{name}” for this project.",
  "role.unknownOfKind":
    "There is no role “{name}” of kind {kind} for this project.",
  "role.noneOfKind":
    "There is no role of kind {kind} for this project; the {default} role is seeded at the next start of the app, " +
    "or add one in Models & roles.",
  "role.doesNotWork":
    "The role “{name}” does not work cards (it reviews or plans); pick a role of kind implement for this card.",
  "role.isMaster":
    "“{name}” is the project's master session, opened from the Agents panel, not a role.",
  "role.reservedForMaster":
    "“{name}” is reserved for the project's master session.",
  "role.defaultUndeletable":
    "The default role “{name}” cannot be deleted (new sessions start as it); edit it instead.",
  "role.inUse":
    "The role “{name}” cannot be deleted: agent sessions play it ({n}).",

  // Sessions and favorites.
  "agent.invalidActor": "“{name}” makes no valid actor.",
  "agent.nameExhausted":
    "“{name}” has been started {n} times; remove its old sessions first.",
  "agent.masterKeepsEngine":
    "The project's master session keeps its engine and role.",
  "favorite.full": "At most {n} favorites; remove one first.",
  "message.empty": "The message is empty.",

  // Starting a card.
  "card.projectNotRepo":
    "An unattended session needs its own worktree, and this project is not a git repository.",
  "card.noCli":
    "A session working on a card needs the team tools, and no `axiomata-cli` was found next to the app (build the " +
    "workspace, or set AXIOMATA_CLI).",
  "card.otherProject":
    "Card #{card} belongs to plan #{plan}, which runs in project {project}; start it there.",
  "card.hasPredecessors":
    "Card #{card} builds on card #{dep}; only a plan that runs by itself starts cards on the work of their " +
    "predecessors.",
  "card.predecessorNotOnLine":
    "Card #{card} builds on card #{dep}, whose work is not on the plan's line yet.",
  "card.workerGone": "The session that worked on the card is gone.",
  "card.sessionGone": "The session that works on the card is gone.",

  // Taking a card over, integrating it.
  "card.takeOverNotSignedOff":
    "Only a card the reviewer signed off can be taken over.",
  "card.takenOverAlready": "The card was taken over already.",
  "card.takeOverPlanCard":
    "This card belongs to a plan that runs by itself: its work is integrated into the plan's line, and the plan is " +
    "taken over as a whole.",
  "card.reviewerGone":
    "The reviewer that signed the card off is gone, so there is no way to tell that the work is what it saw; send " +
    "the card back for a new review.",
  "card.changedAfterReview":
    "The work changed after it was reviewed; send the card back for a new review instead of taking it over.",
  "card.doesNotFitLine":
    "The card does not fit the line, and could not be put back.",
  "card.integrateNotSignedOff":
    "Only a card the reviewer signed off can be integrated.",
  "card.integratedAlready": "The card was integrated or taken over already.",
  "card.noRunningPlan": "The card belongs to no plan that runs by itself.",
  "card.notGivenUp":
    "Only a card the studio gave up integrating can be done again by hand.",
  "card.couldNotPutBack": "The card could not be put back to be done again.",
  "card.notMarkedIntegrated":
    "The work is on the line, but the card could not be marked as integrated.",

  // Reviews.
  "review.noRole":
    "There is no role of kind `review` for this project; add one in the Studio.",
  "review.sameEngine":
    "The reviewer must run on another engine than the session that worked on the card.",
  "review.noEngine":
    "The reviewer needs an engine of the tier {tier} or above other than the worker's, and none can run; switch " +
    "another model of that tier on in the Studio's Models tab, or pick one.",
  "review.notWaiting": "The card is not waiting for a review.",
  "review.alreadyRunning": "The card is being reviewed already, by {name}.",
  "review.projectNotRepo":
    "A reviewer needs a worktree of its own, and this project is not a git repository.",
  "review.noCli":
    "A reviewer needs the team tools, and no `axiomata-cli` was found next to the app (build the workspace, or set " +
    "AXIOMATA_CLI).",
  "review.touchesAgentConfig":
    "The work changes files agents take their configuration from ({files}), and a reviewer started on that checkout " +
    "would obey what the worker wrote there. Review it yourself, or start the review by hand and accept it.",

  // Plans and planners.
  "plan.projectNotRepo":
    "A plan's steps run unattended in worktrees of their own, and this project is not a git repository.",
  "plan.projectGone": "The plan's project is gone.",
  "plan.lineMoved":
    "The line of plan #{plan} is not where the studio left it: something else moved or made the branch {branch}; " +
    "look at it before anything is merged into it.",
  "plan.noLine": "Only a plan that runs by itself has a line to take over.",
  "plan.cardNotOnLine":
    "Card #{card} is not on the line yet ({state}); a plan is taken over when all its cards are.",
  "plan.nothingOnLine":
    "No card of the plan is on its line: there is nothing to take over.",
  "plan.noLimitReached":
    "Plan #{plan} has not reached a limit; there is nothing to go on from.",
  "plan.sessionExists":
    "Plan #{plan} has such a session already, {name}; open its pane in the Studio.",
  "plan.notDraft": "Plan #{plan} is {status}: only a draft is planned.",
  "plan.noLongerDraft": "Plan #{plan} is not a draft any more.",
  "planner.projectNotRepo":
    "A planner needs a worktree of its own, and this project is not a git repository.",
  "planner.noCli":
    "A planner needs the team tools, and no `axiomata-cli` was found next to the app (build the workspace, or set " +
    "AXIOMATA_CLI).",
};

export const tErrors = texts("errors", en, {
  "project.missing": "Es gibt kein Projekt {id}.",
  "card.missing": "Es gibt keine Karte {id}.",
  "plan.missing": "Es gibt keinen Plan {id}.",
  "session.missing": "Es gibt keine Sitzung {id}.",

  "engine.unknown": "Es gibt keine Engine „{id}“.",
  "engine.unknownAddInStudio":
    "Es gibt keine Engine „{id}“; Engines fügst du in den Einstellungen des Studios hinzu.",
  "engine.unknownSwitchOn":
    "Es gibt keine Engine „{id}“; schalte ihr Modell unter „Modelle & Rollen“ ein, oder füge eine " +
    "eigene Engine hinzu.",
  "engine.gone": "Die Engine „{id}“ gibt es nicht mehr; wähle eine andere.",
  "engine.unusable": "Die Engine „{id}“ kann gerade nicht laufen: {why}",
  "engine.ownCommand":
    "Die Engine „{id}“ startet einen eigenen Befehl, den Axiomata nicht anfasst; eine Sitzung ohne Aufsicht braucht " +
    "den erzeugten Befehl.",
  "engine.noneForTier":
    "Keine Engine der Stufe {tier} kann gerade laufen; schalte im Reiter „Modelle“ unter „Modelle & Rollen“ ein Modell dieser " +
    "Stufe ein.",
  "engine.noneForTierUnattended":
    "Keine Engine der Stufe {tier} kann eine Sitzung ohne Aufsicht führen; schalte im Reiter „Modelle“ unter „Modelle & Rollen“ ein " +
    "Modell dieser Stufe ein oder lege die Standard-Engine der Stufe fest, oder wähle beim Start eine Engine.",
  "engine.automatic":
    "„{id}“ ist eine automatische Engine, aus einem Modell des Katalogs gemacht; sie lässt sich weder bearbeiten " +
    "noch entfernen — schalte ihr Modell unter „Modelle & Rollen“ ein oder aus, oder gib einer eigenen " +
    "Engine eine andere Id.",
  "engine.inUseBySessions":
    "Die Engine „{id}“ lässt sich nicht entfernen: Agenten-Sitzungen laufen darauf ({n}).",
  "engine.inUseByRole":
    "Die Engine „{id}“ lässt sich nicht entfernen: die Rolle „{role}“ nutzt sie.",
  "model.badKey":
    "„{key}“ ist kein Modellschlüssel (anthropic/…, openrouter/… oder ollama/…).",

  "role.unknown": "Es gibt keine Rolle „{name}“ für dieses Projekt.",
  "role.unknownOfKind":
    "Es gibt keine Rolle „{name}“ der Art {kind} für dieses Projekt.",
  "role.noneOfKind":
    "Es gibt keine Rolle der Art {kind} für dieses Projekt; die Rolle {default} wird beim nächsten Start der App " +
    "angelegt, oder füge eine unter „Modelle & Rollen“ hinzu.",
  "role.doesNotWork":
    "Die Rolle „{name}“ arbeitet keine Karten ab (sie prüft oder plant); wähle für diese Karte eine Rolle der Art " +
    "implement.",
  "role.isMaster":
    "„{name}“ ist die Master-Sitzung des Projekts, geöffnet aus dem Agents-Panel, keine Rolle.",
  "role.reservedForMaster":
    "„{name}“ ist der Master-Sitzung des Projekts vorbehalten.",
  "role.defaultUndeletable":
    "Die Standardrolle „{name}“ lässt sich nicht löschen (neue Sitzungen starten mit ihr); bearbeite sie stattdessen.",
  "role.inUse":
    "Die Rolle „{name}“ lässt sich nicht löschen: Agenten-Sitzungen spielen sie ({n}).",

  "agent.invalidActor": "„{name}“ ergibt keinen gültigen Akteur.",
  "agent.nameExhausted":
    "„{name}“ wurde schon {n}-mal gestartet; entferne zuerst seine alten Sitzungen.",
  "agent.masterKeepsEngine":
    "Die Master-Sitzung des Projekts behält ihre Engine und Rolle.",
  "favorite.full": "Höchstens {n} Favoriten; entferne zuerst einen.",
  "message.empty": "Die Nachricht ist leer.",

  "card.projectNotRepo":
    "Eine Sitzung ohne Aufsicht braucht einen eigenen Worktree, und dieses Projekt ist kein Git-Repository.",
  "card.noCli":
    "Eine Sitzung an einer Karte braucht die Team-Werkzeuge, und neben der App wurde kein `axiomata-cli` gefunden " +
    "(bau den Workspace, oder setze AXIOMATA_CLI).",
  "card.otherProject":
    "Karte #{card} gehört zu Plan #{plan}, der im Projekt {project} läuft; starte sie dort.",
  "card.hasPredecessors":
    "Karte #{card} baut auf Karte #{dep} auf; nur ein Plan, der von selbst läuft, startet Karten auf der Arbeit " +
    "ihrer Vorgänger.",
  "card.predecessorNotOnLine":
    "Karte #{card} baut auf Karte #{dep} auf, deren Arbeit noch nicht auf der Linie des Plans liegt.",
  "card.workerGone":
    "Die Sitzung, die an der Karte gearbeitet hat, gibt es nicht mehr.",
  "card.sessionGone":
    "Die Sitzung, die an der Karte arbeitet, gibt es nicht mehr.",

  "card.takeOverNotSignedOff":
    "Übernehmen lässt sich nur eine Karte, die der Reviewer abgenommen hat.",
  "card.takenOverAlready": "Die Karte wurde schon übernommen.",
  "card.takeOverPlanCard":
    "Diese Karte gehört zu einem Plan, der von selbst läuft: ihre Arbeit wird in die Linie des Plans integriert, und " +
    "der Plan wird als Ganzes übernommen.",
  "card.reviewerGone":
    "Den Reviewer, der die Karte abgenommen hat, gibt es nicht mehr, also lässt sich nicht sagen, ob die Arbeit die " +
    "ist, die er gesehen hat; gib die Karte für ein neues Review zurück.",
  "card.changedAfterReview":
    "Die Arbeit hat sich nach dem Review geändert; gib die Karte für ein neues Review zurück, statt sie zu übernehmen.",
  "card.doesNotFitLine":
    "Die Karte passt nicht auf die Linie und ließ sich nicht zurücklegen.",
  "card.integrateNotSignedOff":
    "Integrieren lässt sich nur eine Karte, die der Reviewer abgenommen hat.",
  "card.integratedAlready": "Die Karte wurde schon integriert oder übernommen.",
  "card.noRunningPlan":
    "Die Karte gehört zu keinem Plan, der von selbst läuft.",
  "card.notGivenUp":
    "Von Hand neu machen lässt sich nur eine Karte, bei der das Studio das Integrieren aufgegeben hat.",
  "card.couldNotPutBack":
    "Die Karte ließ sich nicht zurücklegen, um sie neu zu machen.",
  "card.notMarkedIntegrated":
    "Die Arbeit liegt auf der Linie, aber die Karte ließ sich nicht als integriert markieren.",

  "review.noRole":
    "Es gibt keine Rolle der Art `review` für dieses Projekt; füge eine im Studio hinzu.",
  "review.sameEngine":
    "Der Reviewer muss auf einer anderen Engine laufen als die Sitzung, die an der Karte gearbeitet hat.",
  "review.noEngine":
    "Der Reviewer braucht eine Engine der Stufe {tier} oder höher, die nicht die der arbeitenden Sitzung ist, und " +
    "keine kann laufen; schalte im Reiter „Modelle“ unter „Modelle & Rollen“ ein weiteres Modell dieser Stufe ein, oder wähle eine.",
  "review.notWaiting": "Die Karte wartet nicht auf ein Review.",
  "review.alreadyRunning": "Die Karte wird schon geprüft, von {name}.",
  "review.projectNotRepo":
    "Ein Reviewer braucht einen eigenen Worktree, und dieses Projekt ist kein Git-Repository.",
  "review.noCli":
    "Ein Reviewer braucht die Team-Werkzeuge, und neben der App wurde kein `axiomata-cli` gefunden (bau den " +
    "Workspace, oder setze AXIOMATA_CLI).",
  "review.touchesAgentConfig":
    "Die Arbeit ändert Dateien, aus denen Agenten ihre Konfiguration lesen ({files}), und ein Reviewer, der auf " +
    "diesem Stand startet, würde befolgen, was dort steht. Prüf sie selbst, oder starte das Review von Hand und " +
    "nimm das in Kauf.",

  "plan.projectNotRepo":
    "Die Schritte eines Plans laufen ohne Aufsicht in eigenen Worktrees, und dieses Projekt ist kein Git-Repository.",
  "plan.projectGone": "Das Projekt des Plans gibt es nicht mehr.",
  "plan.lineMoved":
    "Die Linie von Plan #{plan} ist nicht dort, wo das Studio sie gelassen hat: etwas anderes hat den Branch " +
    "{branch} bewegt oder angelegt; sieh sie dir an, bevor etwas hineingemergt wird.",
  "plan.noLine":
    "Nur ein Plan, der von selbst läuft, hat eine Linie zum Übernehmen.",
  "plan.cardNotOnLine":
    "Karte #{card} liegt noch nicht auf der Linie ({state}); ein Plan wird übernommen, wenn alle seine Karten dort " +
    "liegen.",
  "plan.nothingOnLine":
    "Keine Karte des Plans liegt auf seiner Linie: es gibt nichts zu übernehmen.",
  "plan.noLimitReached":
    "Plan #{plan} hat keine Grenze erreicht; es gibt nichts, wovon es weitergehen könnte.",
  "plan.sessionExists":
    "Plan #{plan} hat schon so eine Sitzung, {name}; öffne ihr Pane im Studio.",
  "plan.notDraft": "Plan #{plan} ist {status}: geplant wird nur ein Entwurf.",
  "plan.noLongerDraft": "Plan #{plan} ist kein Entwurf mehr.",
  "planner.projectNotRepo":
    "Ein Planer braucht einen eigenen Worktree, und dieses Projekt ist kein Git-Repository.",
  "planner.noCli":
    "Ein Planer braucht die Team-Werkzeuge, und neben der App wurde kein `axiomata-cli` gefunden (bau den " +
    "Workspace, oder setze AXIOMATA_CLI).",
});

/** Whether `code` has a text here. */
export function knownCode(code: string): code is keyof typeof en {
  return Object.prototype.hasOwnProperty.call(en, code);
}
