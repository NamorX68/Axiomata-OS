/**
 * The texts of Settings → Workspace → Tags (`shell/TagsSettings.svelte`), its run over the whole vault
 * (`shell/VaultTagging.svelte`) and their helpers (`core/tagProposals.ts`, `core/vaultTagging.ts`). Tags themselves
 * are data and are never translated; `TAGS.md`, Vault and skill names stay as they are (`docs/plans/i18n.md`, Q13).
 */

import { texts } from "./texts";

const en = {
  // The tag list.
  title: "Tags",
  lead:
    "The vault's tag list, kept in {file}. Agents tag notes only from it; a tag they want that is not on it " +
    "waits here for you. Aim for some 30–60 tags, 1–4 per note.",
  proposedByAgents: "Proposed by agents",
  descriptionPlaceholder: "A line of description",
  descriptionOf: "Description of #{tag}",
  descriptionOfNew: "Description of the new tag",
  notes: { one: "1 note", other: "{n} notes" },
  accept: "Accept",
  turnDown: "Turn down",
  readError: "Could not read the tag list: {error}",
  reading: "Reading…",
  onTheList: "On the list ({n})",
  noTags: "No tags yet.",
  newNameOf: "New name of #{tag}",
  usesIn: { one: "{uses} uses in 1 note", other: "{uses} uses in {n} notes" },
  rename: "Rename",
  countUses: "Count uses",
  cancel: "Cancel",
  renameTag: "Rename #{tag}",
  takeOff: "Take #{tag} off the list",
  newTagPlaceholder: "#new-tag",
  newTag: "New tag",
  add: "Add",

  // What saving and answering say.
  saveFailed: "Could not save the tag list: {error}",
  added: "#{tag} added",
  descriptionSaved: "#{tag}: description saved",
  confirmRemove: "Take #{tag} off the list? Notes that carry it keep it; agents no longer choose it.",
  removed: "#{tag} is off the list",
  countFailed: "Could not count the uses: {error}",
  renamed: {
    one: "#{from} is #{to} now — {links} uses in 1 note rewritten",
    other: "#{from} is #{to} now — {links} uses in {n} notes rewritten",
  },
  renameFailed: "Renaming failed: {error}",
  acceptedOn: { one: "#{tag} is on the list and set on 1 note", other: "#{tag} is on the list and set on {n} notes" },
  notSet: "#{tag} not set: {failure}",
  turnedDown: "#{tag} turned down",
  answerFailed: "Could not answer the proposal: {error}",

  // The badge (`core/tagProposals.ts`).
  waiting: { one: "A tag proposal waits for you", other: "{n} tag proposals wait for you" },

  // What is wrong with a typed tag (`core/tagProposals.ts`).
  problemEmpty: "Type a tag.",
  problemLength: "At most 64 characters.",
  problemChars: "Letters, digits, _ - and / only.",
  problemDigits: "Not only digits.",
  problemStart: "Start with a letter or digit.",
  problemSlash: "A / only between two parts.",
  problemTaken: "#{tag} is on the list already.",

  // The run over the whole vault.
  vaultTitle: "Tag the whole vault",
  vaultHint: "Once, for the notes there already are. Each step says first what it reads and costs.",
  stepList: "1 · Propose a tag list",
  stepNotes: "2 · Plan tags and links for every note",
  running: "running",
  proposing: "Proposing…",
  propose: "Propose",
  planning: "Planning…",
  planRest: "Plan the rest",
  plan: "Plan",
  stale: "A step is marked as running, but not from this window (the app was closed during it).",
  endIt: "End it",
  preview: { one: "Preview (1 note)", other: "Preview ({n} notes)" },
  proposedChip: "Proposed to you when the plan is written; set once you accept it",
  leftOut: "left out: {reason}",
  nothing: "nothing",
  showAll: "Show all {n}",
  writeInto: { one: "Write into 1 note", other: "Write into {n} notes" },
  discard: "Discard",
  confirmList: "Propose a tag list? The agent reads the notes and proposes 30–60 tags; you accept or turn down each.",
  confirmNotes:
    "Plan tags for every note? The agent reads each note; nothing is written until you look at the preview.",
  runStatus: "vault-tagging: {status}",
  runFailed: "The vault run failed: {error}",
  confirmApply: {
    one: "Write the planned tags and links into 1 note? Only its tags: and ## Related change.",
    other: "Write the planned tags and links into {n} notes? Only their tags: and ## Related change.",
  },
  written: { one: "Written into 1 note", other: "Written into {n} notes" },
  writtenUnchanged: {
    one: "Written into 1 note, {unchanged} unchanged",
    other: "Written into {n} notes, {unchanged} unchanged",
  },
  notWritten: {
    one: "1 note not written (kept in the preview): {first}",
    other: "{n} notes not written (kept in the preview): {first}",
  },
  writeFailed: "Writing failed: {error}",
  confirmDiscard: "Throw the preview away? No note was changed.",

  // The estimate of a step (`core/vaultTagging.ts`).
  estimateTokens: "about {input} tokens in, {output} out",
  estimateCost: "about {cost}",
  estimateNoPrice: "no price for {model} in your price table",
  estimateNoPriceSkill: "no price for the skill model in your price table",
};

export const tTags = texts("tags", en, {
  title: "Tags",
  lead:
    "Die Tag-Liste des Vault, abgelegt in {file}. Agenten taggen Notizen nur aus ihr; ein Tag, das sie wollen und " +
    "das nicht darauf steht, wartet hier auf dich. Ziel: etwa 30–60 Tags, 1–4 pro Notiz.",
  proposedByAgents: "Von Agenten vorgeschlagen",
  descriptionPlaceholder: "Eine Zeile Beschreibung",
  descriptionOf: "Beschreibung von #{tag}",
  descriptionOfNew: "Beschreibung des neuen Tags",
  notes: { one: "1 Notiz", other: "{n} Notizen" },
  accept: "Annehmen",
  turnDown: "Ablehnen",
  readError: "Die Tag-Liste ließ sich nicht lesen: {error}",
  reading: "Lese…",
  onTheList: "Auf der Liste ({n})",
  noTags: "Noch keine Tags.",
  newNameOf: "Neuer Name von #{tag}",
  usesIn: { one: "{uses}× in 1 Notiz", other: "{uses}× in {n} Notizen" },
  rename: "Umbenennen",
  countUses: "Stellen zählen",
  cancel: "Abbrechen",
  renameTag: "#{tag} umbenennen",
  takeOff: "#{tag} von der Liste nehmen",
  newTagPlaceholder: "#neues-tag",
  newTag: "Neues Tag",
  add: "Hinzufügen",

  saveFailed: "Die Tag-Liste ließ sich nicht speichern: {error}",
  added: "#{tag} hinzugefügt",
  descriptionSaved: "#{tag}: Beschreibung gespeichert",
  confirmRemove:
    "#{tag} von der Liste nehmen? Notizen, die es tragen, behalten es; Agenten wählen es nicht mehr.",
  removed: "#{tag} ist nicht mehr auf der Liste",
  countFailed: "Die Stellen ließen sich nicht zählen: {error}",
  renamed: {
    one: "#{from} heißt jetzt #{to} — {links} Stellen in 1 Notiz umgeschrieben",
    other: "#{from} heißt jetzt #{to} — {links} Stellen in {n} Notizen umgeschrieben",
  },
  renameFailed: "Umbenennen fehlgeschlagen: {error}",
  acceptedOn: {
    one: "#{tag} steht auf der Liste und ist an 1 Notiz gesetzt",
    other: "#{tag} steht auf der Liste und ist an {n} Notizen gesetzt",
  },
  notSet: "#{tag} nicht gesetzt: {failure}",
  turnedDown: "#{tag} abgelehnt",
  answerFailed: "Der Vorschlag ließ sich nicht beantworten: {error}",

  waiting: { one: "Ein Tag-Vorschlag wartet auf dich", other: "{n} Tag-Vorschläge warten auf dich" },

  problemEmpty: "Gib ein Tag ein.",
  problemLength: "Höchstens 64 Zeichen.",
  problemChars: "Nur Buchstaben, Ziffern, _ - und /.",
  problemDigits: "Nicht nur Ziffern.",
  problemStart: "Fange mit einem Buchstaben oder einer Ziffer an.",
  problemSlash: "Ein / nur zwischen zwei Teilen.",
  problemTaken: "#{tag} steht schon auf der Liste.",

  vaultTitle: "Den ganzen Vault taggen",
  vaultHint: "Einmal, für die Notizen, die es schon gibt. Jeder Schritt sagt vorher, was er liest und kostet.",
  stepList: "1 · Eine Tag-Liste vorschlagen",
  stepNotes: "2 · Tags und Links für jede Notiz planen",
  running: "läuft",
  proposing: "Schlage vor…",
  propose: "Vorschlagen",
  planning: "Plane…",
  planRest: "Den Rest planen",
  plan: "Planen",
  stale:
    "Ein Schritt ist als laufend markiert, aber nicht aus diesem Fenster (die App wurde währenddessen geschlossen).",
  endIt: "Beenden",
  preview: { one: "Vorschau (1 Notiz)", other: "Vorschau ({n} Notizen)" },
  proposedChip: "Wird dir vorgeschlagen, wenn der Plan geschrieben wird; gesetzt, sobald du es annimmst",
  leftOut: "ausgelassen: {reason}",
  nothing: "nichts",
  showAll: "Alle {n} zeigen",
  writeInto: { one: "In 1 Notiz schreiben", other: "In {n} Notizen schreiben" },
  discard: "Verwerfen",
  confirmList:
    "Eine Tag-Liste vorschlagen? Der Agent liest die Notizen und schlägt 30–60 Tags vor; du nimmst jedes an " +
    "oder lehnst es ab.",
  confirmNotes:
    "Tags für jede Notiz planen? Der Agent liest jede Notiz; nichts wird geschrieben, bevor du dir die Vorschau " +
    "angesehen hast.",
  runStatus: "vault-tagging: {status}",
  runFailed: "Der Vault-Lauf ist fehlgeschlagen: {error}",
  confirmApply: {
    one: "Die geplanten Tags und Links in 1 Notiz schreiben? Nur ihr tags: und ## Related ändern sich.",
    other: "Die geplanten Tags und Links in {n} Notizen schreiben? Nur ihr tags: und ## Related ändern sich.",
  },
  written: { one: "In 1 Notiz geschrieben", other: "In {n} Notizen geschrieben" },
  writtenUnchanged: {
    one: "In 1 Notiz geschrieben, {unchanged} unverändert",
    other: "In {n} Notizen geschrieben, {unchanged} unverändert",
  },
  notWritten: {
    one: "1 Notiz nicht geschrieben (bleibt in der Vorschau): {first}",
    other: "{n} Notizen nicht geschrieben (bleiben in der Vorschau): {first}",
  },
  writeFailed: "Schreiben fehlgeschlagen: {error}",
  confirmDiscard: "Die Vorschau verwerfen? Keine Notiz wurde geändert.",

  estimateTokens: "etwa {input} Tokens rein, {output} raus",
  estimateCost: "etwa {cost}",
  estimateNoPrice: "kein Preis für {model} in deiner Preistabelle",
  estimateNoPriceSkill: "kein Preis für das Skills-Modell in deiner Preistabelle",
});
