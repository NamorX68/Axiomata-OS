/**
 * The texts of the Second Brain and the Orbit (`shell/SecondBrainView.svelte`, `modules/second-brain.svelte`,
 * `modules/TimeTravel.svelte`, `modules/memory-status*.svelte`, `graph/Legend.svelte`, `graph/GlyphPicker.svelte`) and
 * of what the graph draws on its canvas (`graph/render.ts`). "Second Brain", "Orbit", "Vault", area, file, tag and note
 * names are names or data and stay as they are (`docs/plans/i18n.md`, Q13).
 */

import { texts } from "./texts";

const en = {
  secondBrain: "Second Brain",

  // The view's controls.
  layout: "Layout",
  groupBy: "Group by",
  areas: "Areas",
  folders: "Folders",
  findPlaceholder: "Find notes or #tag",
  findLabel: "Find notes by name or #tag",
  matches: { one: "1 match", other: "{n} matches" },
  replayTitle: "Replay the vault's life from its first note, in twenty seconds",
  replayButton: "▶ How your vault grew",
  resetViewTitle: "Reset zoom and pan",
  resetView: "Reset view",
  reloadTitle: "Reload the graph from the workspace",
  reload: "Reload",
  helpTitle: "What do the options mean?",
  help: "Help",

  // Counts under the controls and under the Orbit.
  notes: { one: "1 note", other: "{n} notes" },
  files: { one: "1 file", other: "{n} files" },
  areaCount: { one: "1 area", other: "{n} areas" },
  folderCount: { one: "1 folder", other: "{n} folders" },
  links: { one: "1 link", other: "{n} links" },
  truncated: "truncated",

  // The help list beside the layout buttons.
  helpBundle:
    "Every note on one ring, an arc per area, the newest first, clockwise. Links run through the middle, bundled " +
    "along the folders, in the colour of the note they start from; the links of the last {days} days glow. Point at " +
    "a note or an area to see its links. Skills ring it outside.",
  helpMap:
    "A map of the vault: every note a field, every area a country, the oldest notes at its heart. Links are roads " +
    "(zoom in); close up every field shows its name. Skills ring the map.",
  helpHex:
    "A honeycomb: every note its own hex cell, packed tight, each area in its own wedge, the newest nearest the " +
    "middle. Skills and areas sit inside it.",
  helpAreas: "One segment per top-level vault folder.",
  helpFolders:
    "One segment per deepest folder, e.g. Learning/Rust/lessons. A finer split of large areas; in Bundle the links " +
    "bundle along the folder tree.",

  // "How your vault grew".
  growthTitle: "How your vault grew",
  pause: "Pause",
  play: "Play",
  growthMoment: "Moment of the replay",
  endReplay: "End the replay",
  endReplayTitle: "End (Esc)",
  growthAt: "{day} · {notes}",

  // The detail panel.
  kind_file: "file",
  kind_hub: "hub",
  kind_area: "area",
  kind_skill: "skill",
  kind_app: "app",
  deselect: "Deselect",
  folder: "Folder",
  size: "Size",
  modified: "Modified",
  path: "Path",
  linksTerm: "Links",
  tags: "Tags",
  linksInOut: "{out} out · {in} in",
  showTagged: "Show the notes tagged #{tag}",
  open: "Open",
  copyPath: "Copy path",
  flyTo: "Fly to",
  deleting: "Deleting…",
  reallyDelete: "Really delete",
  cancel: "Cancel",
  delete: "Delete",
  notesTerm: "Notes",
  foldersTerm: "Folders",
  notesInArea: "Notes in this area ({n})",
  filterPlaceholder: "Filter…",
  filterLabel: "Filter notes",
  runSkill: "▶ Run",
  linksTo: "Links to ({n})",
  linkedFrom: "Linked from ({n})",
  noLinks: "No links yet.",
  skillRan: "/{name}: {status} ({ms} ms)",
  fileDeleted: "File deleted.",
  deleteFailed: "Deleting failed: {error}",
  pathCopied: "Path copied.",

  // The Orbit tile.
  clickToOpen: "CLICK TO OPEN SECOND BRAIN",
  addApp: "Add app",

  // The time travel.
  today: "today",
  stop: "Stop",
  growVault: "Grow the vault",
  timeTravel: "Time travel",
  backToToday: "Back to today",
  timeTravelOpen: "Time travel: how the vault grew",

  // The legend.
  legendHub: "AGENTS.md (hub)",
  legendArea: "Area (folder)",
  legendNote: "Note / page",
  legendSkill: "Skill",
  legendLink: "Link, in its note's area colour",
  legendFreshLink: "Link of the last {days} days",

  // The canvas: a note's age badge and the honeycomb's ring captions.
  ageNew: "NEW",
  ageDays: "{n}D",
  ageMonths: "{n}M",
  ageYears: "{n}Y",
  captionSkills: "SKILLS",
  captionMemory: "MEMORY",

  // The memory status tile.
  loading: "Loading…",
  stale: "stale",
  fresh: "fresh",
  filesWord: { one: "file", other: "files" },
  synced: "synced {when}",
  syncing: "Syncing…",
  sync: "Sync",
  inSync: "In sync · {files}",
  wrote: "Wrote {n} · {unchanged} unchanged",
  failedCount: "{n} failed",
  syncFailed: "Sync failed: {error}",
  compact: "Compact (badge line only)",

  // The icon picker: the glyphs' names, read out for each cell.
  chooseIcon: "Choose an icon",
  glyph_group: "Group",
  glyph_folder: "Folder",
  glyph_code: "Code",
  glyph_terminal: "Terminal",
  glyph_ide: "IDE",
  glyph_chip: "Chip",
  glyph_graphic: "Graphics",
  glyph_calculator: "Calculator",
  glyph_browser: "Browser",
  glyph_book: "Book",
  glyph_briefcase: "Briefcase",
  glyph_camera: "Camera",
  glyph_photos: "Photos",
  glyph_music: "Music",
  glyph_mail: "Mail",
  glyph_messenger: "Messenger",
  glyph_writer: "Writing",
  glyph_journal: "Journal",
  glyph_people: "People",
  glyph_user: "Person",
  glyph_wrench: "Wrench",
  glyph_tray: "Tray",
  glyph_check: "Check",
  glyph_calendar: "Calendar",
  glyph_list: "List",
};

const de = {
  secondBrain: "Second Brain",

  layout: "Anordnung",
  groupBy: "Gruppieren nach",
  areas: "Bereichen",
  folders: "Ordnern",
  findPlaceholder: "Notizen oder #Tag finden",
  findLabel: "Notizen nach Name oder #Tag finden",
  matches: { one: "1 Treffer", other: "{n} Treffer" },
  replayTitle:
    "Das Leben des Vault ab seiner ersten Notiz abspielen, in zwanzig Sekunden",
  replayButton: "▶ Wie dein Vault wuchs",
  resetViewTitle: "Zoom und Verschiebung zurücksetzen",
  resetView: "Ansicht zurücksetzen",
  reloadTitle: "Den Graphen neu aus dem Workspace laden",
  reload: "Neu laden",
  helpTitle: "Was bedeuten die Optionen?",
  help: "Hilfe",

  notes: { one: "1 Notiz", other: "{n} Notizen" },
  files: { one: "1 Datei", other: "{n} Dateien" },
  areaCount: { one: "1 Bereich", other: "{n} Bereiche" },
  folderCount: { one: "1 Ordner", other: "{n} Ordner" },
  links: { one: "1 Link", other: "{n} Links" },
  truncated: "gekürzt",

  helpBundle:
    "Jede Notiz auf einem Ring, ein Bogen je Bereich, die neueste zuerst, im Uhrzeigersinn. Links laufen durch die " +
    "Mitte, gebündelt entlang der Ordner, in der Farbe der Notiz, von der sie ausgehen; die Links der letzten " +
    "{days} Tage glühen. Zeige auf eine Notiz oder einen Bereich, um seine Links zu sehen. Skills umringen ihn " +
    "außen.",
  helpMap:
    "Eine Karte des Vault: jede Notiz ein Feld, jeder Bereich ein Land, die ältesten Notizen in seinem Kern. Links " +
    "sind Straßen (zoome hinein); aus der Nähe zeigt jedes Feld seinen Namen. Skills umringen die Karte.",
  helpHex:
    "Eine Wabe: jede Notiz eine eigene Hex-Zelle, dicht gepackt, jeder Bereich in seinem eigenen Keil, die neuesten " +
    "nah der Mitte. Skills und Bereiche liegen darin.",
  helpAreas: "Ein Segment je Ordner der obersten Vault-Ebene.",
  helpFolders:
    "Ein Segment je tiefstem Ordner, z. B. Learning/Rust/lessons. Eine feinere Teilung großer Bereiche; im Bündel " +
    "bündeln sich die Links entlang des Ordnerbaums.",

  growthTitle: "Wie dein Vault wuchs",
  pause: "Pause",
  play: "Abspielen",
  growthMoment: "Moment der Wiedergabe",
  endReplay: "Wiedergabe beenden",
  endReplayTitle: "Beenden (Esc)",
  growthAt: "{day} · {notes}",

  kind_file: "Datei",
  kind_hub: "Hub",
  kind_area: "Bereich",
  kind_skill: "Skill",
  kind_app: "App",
  deselect: "Auswahl aufheben",
  folder: "Ordner",
  size: "Größe",
  modified: "Geändert",
  path: "Pfad",
  linksTerm: "Links",
  tags: "Tags",
  linksInOut: "{out} ausgehend · {in} eingehend",
  showTagged: "Die Notizen mit #{tag} zeigen",
  open: "Öffnen",
  copyPath: "Pfad kopieren",
  flyTo: "Hinfliegen",
  deleting: "Lösche…",
  reallyDelete: "Wirklich löschen",
  cancel: "Abbrechen",
  delete: "Löschen",
  notesTerm: "Notizen",
  foldersTerm: "Ordner",
  notesInArea: "Notizen in diesem Bereich ({n})",
  filterPlaceholder: "Filtern…",
  filterLabel: "Notizen filtern",
  runSkill: "▶ Ausführen",
  linksTo: "Verlinkt auf ({n})",
  linkedFrom: "Verlinkt von ({n})",
  noLinks: "Noch keine Links.",
  skillRan: "/{name}: {status} ({ms} ms)",
  fileDeleted: "Datei gelöscht.",
  deleteFailed: "Löschen fehlgeschlagen: {error}",
  pathCopied: "Pfad kopiert.",

  clickToOpen: "KLICKE, UM DAS SECOND BRAIN ZU ÖFFNEN",
  addApp: "App hinzufügen",

  today: "heute",
  stop: "Stopp",
  growVault: "Den Vault wachsen lassen",
  timeTravel: "Zeitreise",
  backToToday: "Zurück zu heute",
  timeTravelOpen: "Zeitreise: wie der Vault wuchs",

  legendHub: "AGENTS.md (Hub)",
  legendArea: "Bereich (Ordner)",
  legendNote: "Notiz / Seite",
  legendSkill: "Skill",
  legendLink: "Link, in der Bereichsfarbe seiner Notiz",
  legendFreshLink: "Link der letzten {days} Tage",

  ageNew: "NEU",
  ageDays: "{n} T.",
  ageMonths: "{n}M",
  ageYears: "{n}J",
  captionSkills: "SKILLS",
  captionMemory: "GEDÄCHTNIS",

  loading: "Lade…",
  stale: "veraltet",
  fresh: "aktuell",
  filesWord: { one: "Datei", other: "Dateien" },
  synced: "synchronisiert {when}",
  syncing: "Synchronisiere…",
  sync: "Synchronisieren",
  inSync: "Synchron · {files}",
  wrote: "{n} geschrieben · {unchanged} unverändert",
  failedCount: "{n} fehlgeschlagen",
  syncFailed: "Synchronisieren fehlgeschlagen: {error}",
  compact: "Kompakt (nur die Statuszeile)",

  chooseIcon: "Wähle ein Symbol",
  glyph_group: "Gruppe",
  glyph_folder: "Ordner",
  glyph_code: "Code",
  glyph_terminal: "Terminal",
  glyph_ide: "IDE",
  glyph_chip: "Chip",
  glyph_graphic: "Grafik",
  glyph_calculator: "Rechner",
  glyph_browser: "Browser",
  glyph_book: "Buch",
  glyph_briefcase: "Aktentasche",
  glyph_camera: "Kamera",
  glyph_photos: "Fotos",
  glyph_music: "Musik",
  glyph_mail: "Mail",
  glyph_messenger: "Messenger",
  glyph_writer: "Schreiben",
  glyph_journal: "Tagebuch",
  glyph_people: "Leute",
  glyph_user: "Person",
  glyph_wrench: "Schraubenschlüssel",
  glyph_tray: "Ablage",
  glyph_check: "Haken",
  glyph_calendar: "Kalender",
  glyph_list: "Liste",
};

export const tBrain = texts("brain", en, de);

/** A key of this catalog. */
export type BrainKey = keyof typeof en;

/** The name of an icon (`graph/render.ts` `APP_GROUP_GLYPHS`) to read out; an id without a name reads as itself. */
export function glyphName(glyph: string): string {
  const key = `glyph_${glyph}`;
  return Object.prototype.hasOwnProperty.call(en, key)
    ? tBrain(key as BrainKey)
    : glyph;
}

/** The shown name of a graph node's kind. */
export function kindName(
  kind: "hub" | "area" | "file" | "skill" | "app",
): string {
  return tBrain(`kind_${kind}`);
}
