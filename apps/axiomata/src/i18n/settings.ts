/**
 * The texts of Settings (`shell/Settings.svelte` and its parts, `core/settingsTabs.ts`). Provider, harness, model and
 * theme names stay as they are (`docs/plans/i18n.md`, Q13). A text that wraps a `<code>` piece is split around it
 * (`…Before` / `…After`), so German may place the piece where its word order needs it.
 */

import { texts } from "./texts";

const en = {
  language: "Language",
  languageSystem: "System ({lang})",
  languageHint:
    "The app's buttons, notes and the handbook. Names like Studio, Canvas, Flow or Skills stay as they are; " +
    "the agents always work in English.",
  langDe: "Deutsch",
  langEn: "English",

  // The dialog and its tabs.
  title: "Settings",
  tabAppearance: "Appearance",
  tabWorkspace: "Workspace",
  tabProviders: "AI providers",
  tabStorage: "Storage",
  tabAbout: "About",

  // Appearance.
  theme: "Theme",
  typeface: "Typeface",
  fontSample: "Aa",
  typefaceHint: "For the whole interface. Code in the editor and the terminal keep their own fonts.",
  motion: "Motion",
  movement: "Movement",
  motionHint:
    "Full: the glow breathes, sparks fly, the Orbit turns. Calm: the glow holds still, nothing flies, the Orbit " +
    "still turns. Off: nothing moves by itself.",
  reduceMotionOn: "macOS “Reduce motion” is on, so motion is off.",
  light: "Light",
  pointerLight: "Light under the pointer",
  pointerLightHint: "(the hex lines glow where the mouse is)",
  dayLight: "Time-of-day light",
  dayLightHint: "(a faint light that moves with the sun)",
  ignition: "Start ignition",
  ignitionHint: "(the logo lights up when the app starts; motion Full only)",
  sound: "Sound",
  sounds: "Sounds",
  soundsHint: "(only while Axiomata is in the background)",
  canvas: "Canvas",
  showGrid: "Show dot grid",
  showGridHint: "(tiles snap to it either way)",
  magneticEdges: "Magnetic edges",
  magneticEdgesHint: "(tiles stick to their neighbours within 8 px)",
  orbitBrain: "Orbit & Second Brain",
  speed: "Speed",
  labels: "Labels",
  labelsHint: "(skills, areas and the hub)",
  fileNames: "File names",
  fileNamesHint: "(next to every note in the Second Brain)",
  shimmer: "Shimmer",
  shimmerHint: "(how the hex cells catch the light; still unless motion is Full)",
  layout: "Layout",
  layoutHint: "(the Second Brain: a ring of bundled links, countries or a honeycomb)",
  uiSize: "UI size",
  auto: "Auto",
  autoHint: "({percent}% on this display)",
  textAndControls: "Text and controls",
  uiSizeHint:
    "“Auto” follows the real pixel density of the display the window is on: on a dense monitor the interface " +
    "grows so it reads as well as on a MacBook. The fonts of the editor and the terminal are not affected.",
  keyboard: "Keyboard",
  shortcutsFollow: "Shortcuts follow",
  keyboardHint:
    "Mac: ⌘ is the main key. Windows & Linux: Ctrl is, and in a terminal an app shortcut takes Ctrl+Shift, " +
    "because Ctrl belongs to the shell there. “Auto” follows this machine; the other two let you try the other " +
    "keys here.",
  glass: "Glass",
  transparency: "Transparency",
  blur: "Blur",
  backgroundImage: "Background image",
  chooseAnother: "Choose another…",
  chooseImage: "Choose image…",
  remove: "Remove",
  imageNotTaken: "The image was not taken: {error}",
  imageNotRemoved: "The image could not be removed: {error}",
  imageDim: "Image dim",
  pixels: "{n} px",
  glassHint:
    "Transparency: how much of the ground shows through the surfaces — 0% solid, 100% clear, 50% the theme's own " +
    "look. Blur: how soft the ground is, and with it everything seen through the glass. The ground is the theme's " +
    "own or your image, blurred once, so a softer one costs nothing while you work. Image dim: how strongly the " +
    "theme's own ground veils your image — darker in a dark theme, lighter in a light one —, so text stays " +
    "readable. Menus and text overlays stay dense either way, and macOS “Reduce transparency” makes all of it " +
    "solid.",
  reduceTransparencyOn: "macOS “Reduce transparency” is on: the glass is solid now.",
  customCss: "Custom CSS",
  notLoaded: "Not loaded.",
  reloading: "Reloading…",
  reloadCss: "Reload custom CSS",
  copyTemplate: "Copy template",
  templateCopied: "Template copied — save it as ~/.axiomata/theme.css.",
  customAbsent: "No ~/.axiomata/theme.css.",
  customApplied: "Custom CSS applied.",
  customRejected: "Custom CSS rejected; built-in theme kept.",

  // The choices of Appearance's selects and buttons (`core/motion.ts`, `core/sounds.ts`, `core/brainView.ts`,
  // `core/keys.ts`); the ids they stand for stay as they are.
  motionFull: "Full",
  motionCalm: "Calm",
  motionOff: "Off",
  cueWaiting: "An agent waits for you",
  cueDone: "An agent finished its turn",
  cueRoutine: "A routine fired",
  shineCalm: "Calm",
  shineTwinkle: "Twinkle",
  shineMetal: "Metal",
  layoutBundle: "Bundle",
  layoutMap: "Map",
  layoutHex: "Hex",
  keyboardAuto: "Auto",
  keyboardMac: "Mac",
  keyboardOther: "Windows & Linux",

  // The themes' one-liners (`core/themes.ts`); their names stay as they are.
  blurbGraphite: "Near-black, orange accent, hex mesh",
  blurbPaper: "Warm cream, ink text, clay accent",
  blurbSteampunk: "Bronze and leather, brass accent",
  blurbForest: "Pine and moss, autumn-amber accent",
  blurbOcean: "Deep-sea navy, coral accent",
  blurbCatppuccinMocha: "Soft dark, mauve accent",
  blurbCatppuccinMacchiato: "Soft dark, a touch lighter",
  blurbCatppuccinFrappe: "Soft dim, mauve accent",
  blurbCatppuccinLatte: "Soft light, mauve accent",
  blurbCatppuccinEspresso: "Macchiato on near-black",
  blurbTokyoNight: "Deep indigo, blue accent",
  blurbGithubDark: "Familiar dark, blue accent",
  blurbGithubLight: "Familiar light, blue accent",

  // The harnesses' and web-search providers' one-liners (`core/harness.ts`, `core/webSettings.ts`).
  harnessClaudeCode: "Anthropic only, on the app's own login",
  harnessGlut: "Axiomata's own harness — OpenRouter or Ollama",
  webExaHint: "Works without a key (rate limited); a key raises the limit",
  webTinyfishHint: "Asked when Exa is limited or fails",
  webTavilyHint: "Asked last, only up to the monthly limit below (the account holds a card)",

  // Workspace.
  vault: "Vault",
  vaultLead: "Root folder of the Second Brain workspace. A change takes effect after a restart.",
  path: "Path",
  vaultPlaceholder: "/Users/…/Vault",
  saving: "Saving…",
  saveRestart: "Save & restart",
  vaultEmpty: "The vault path must not be empty.",
  vaultSavedRestart: "Vault path saved — restart Axiomata-OS to switch.",
  vaultSaved: "Vault path saved.",
  savingFailed: "Saving failed: {error}",

  // AI providers: the providers themselves.
  editProviders: "Edit providers",
  editProvidersLead:
    "A provider is where the model runs — its endpoint, key and models. Every provider is configured " +
    "independently here; which harness and provider serve the chat and the skills is set in the next section.",
  badgeChat: "Chat",
  badgeSkills: "Skills",
  blurbAnthropic: "Billed through the subscription — no key",
  blurbOpenRouter: "Anthropic-compatible endpoint",
  blurbOllama: "Local model on this machine",
  anthropicNoKey: "No key or base URL needed — Anthropic is billed through the subscription's CLI login.",
  baseUrl: "Base URL",
  apiKey: "API key",
  apiKeyOptional: "API key (optional)",
  keySaved: "•••••• saved — leave empty to keep",
  keyOllama: "A placeholder is enough for a local Ollama",
  keyRequired: "required",
  keyDeletedOnSave: "The key is deleted on save.",
  chatModel: "Chat model",
  skillsModel: "Skills/routines model",
  modelExample: "e.g. {model}",
  modelOverrideBefore: "A",
  modelOverrideAfter: "in the SKILL.md frontmatter still overrides the skills model.",
  unknownProvider: "Unknown provider “{id}” — no settings to edit. Pick one of the known providers above.",
  saveProvider: "Save provider",
  providersSaved: "Provider settings saved — they apply from the next agent call.",

  // AI providers: what a save refuses (Rust's `validate_for_save` is the authority).
  roleChat: "Chat",
  roleSkills: "Skills",
  fieldChatModel: "chat model",
  fieldSkillsModel: "skills model",
  noProviderSettings: "{role}: provider “{id}” has no settings.",
  needsKey: "{role} ({provider}) needs an API key.",
  modelEmpty: "{role} ({provider}): {field} must not be empty.",
  modelSpaces: "{role} ({provider}): {field} contains spaces.",
  modelBrackets: "{role} ({provider}): {field} has unbalanced brackets — “{value}”.",
  baseUrlHttps: "{role} ({provider}): Base URL must start with https://.",

  // AI providers: the model lists.
  catalogBuiltIn: "Built-in list — Anthropic has no model list to ask without an API key.",
  catalogNone: "No model list yet.",
  catalogFailed: "The last fetch failed: {error}",
  catalogFetched: {
    one: "{n} model, fetched {when}; refreshed at start and once a day.",
    other: "{n} models, fetched {when}; refreshed at start and once a day.",
  },
  pricesNotSaved: "The prices could not be saved: {error}",
  pricesUpdated: "Prices updated: {models}",
  modelListFailed: "Could not fetch the model list: {error}",

  // AI providers: harness and provider per role.
  harnessAssignment: "Harness & provider assignment",
  harnessLeadBefore:
    "The harness drives the agent — its tools, rights and loop; the provider serves the model. Each role picks " +
    "both, e.g. Claude Code for the chat and Glut with a local Ollama for the digests. A skill's own",
  harnessLeadAfter: "still wins over the harness set here.",
  forChat: "chat",
  forSkills: "skills & routines",
  harnessFor: "Harness for {role}",
  providerFor: "Provider for {role}",
  effortFor: "Effort for {role}",
  glutOwnEffort: "Glut's own setting",
  glutOwnEffortElse: "Glut's own setting, else {level}",
  effortNotListed: "{level} (not listed)",
  webSkillBefore: "A skill that reads the web (",
  webSkillAfter: ") uses Glut's web search — its keys and limits are in the Web search block below.",

  // AI providers: the app's own Claude Code sign-in.
  claudeSignIn: "Claude Code sign-in",
  signedInAs: "Signed in as {email}",
  unknownEmail: "unknown",
  ownLogin: "— the app's own login, apart from yours in",
  claudeUnreachable: "Claude Code cannot be asked: {error}",
  notSignedIn: "Not signed in — chat and skills on Claude Code fail until the app's Claude Code signs in.",
  waitingForBrowser: "Waiting for the browser…",
  signInAnthropic: "Sign in with Anthropic",
  signInCode: "Code (only if the browser page shows one)",
  send: "Send",
  signOut: "Sign out",
  signedIn: "Claude Code is signed in.",
  signInIncomplete: "The sign-in did not complete.",
  signInFailed: "Sign-in failed: {error}",
  codeNotTaken: "The code was not taken: {error}",
  signedOut: "Claude Code is signed out.",
  signOutFailed: "Sign-out failed: {error}",

  // AI providers: Glut's web search.
  webSearch: "Web search (Glut)",
  webSearchBefore: "Glut's",
  webSearchMiddle:
    "asks Exa first, then TinyFish, then Tavily — a provider without its key is skipped. Keys are stored in",
  webSearchAfter: "and never shown again.",
  webKey: "{name} key",
  optional: "optional",
  notSet: "not set",
  searchesPerDay: "Searches per day (all providers)",
  tavilyPerMonth: "Tavily searches per month",
  webLimitsHint: "0 turns the search (or Tavily) off. {n} Tavily searches a month are its free credits.",

  // AI providers: spend.
  spend: "Spend",
  spendToday: "{amount} today",
  spendCap: "/ {amount} limit",
  spendMonth: "· {amount} this month",
  roleToProvider: "{role} → {provider}",
  subscriptionBilled: "{role} → {provider} is billed through the subscription — no cost tracking.",
  dailyLimit: "Daily limit (USD, paid providers)",
  noLimit: "empty = no limit",
  dailyLimitHint: "Once today's total reaches the limit, the next agent call through a paid provider is refused.",
  saveAssignment: "Save assignment",

  // About.
  owner: "Owner",
  ownerUnset: "— (set `owner` in ~/.axiomata/config.toml)",
  workspace: "Workspace",
  version: "Version",
};

export const tSettings = texts("settings", en, {
  language: "Sprache",
  languageSystem: "System ({lang})",
  languageHint:
    "Knöpfe, Hinweise und das Handbuch der App. Namen wie Studio, Canvas, Flow oder Skills bleiben, wie sie sind; " +
    "die Agenten arbeiten immer auf Englisch.",
  langDe: "Deutsch",
  langEn: "English",

  title: "Einstellungen",
  tabAppearance: "Darstellung",
  tabWorkspace: "Workspace",
  tabProviders: "KI-Anbieter",
  tabStorage: "Speicher",
  tabAbout: "Über",

  theme: "Theme",
  typeface: "Schrift",
  fontSample: "Aa",
  typefaceHint: "Für die ganze Oberfläche. Code im Editor und das Terminal behalten ihre eigenen Schriften.",
  motion: "Bewegung",
  movement: "Bewegung",
  motionHint:
    "Voll: das Glühen atmet, Funken fliegen, der Orbit dreht sich. Ruhig: das Glühen steht still, nichts fliegt, " +
    "der Orbit dreht sich weiter. Aus: nichts bewegt sich von selbst.",
  reduceMotionOn: "„Bewegung reduzieren“ von macOS ist an, deshalb ist die Bewegung aus.",
  light: "Licht",
  pointerLight: "Licht unter dem Zeiger",
  pointerLightHint: "(die Hex-Linien glühen, wo die Maus ist)",
  dayLight: "Tageszeit-Licht",
  dayLightHint: "(ein schwaches Licht, das mit der Sonne wandert)",
  ignition: "Zündung beim Start",
  ignitionHint: "(das Logo leuchtet auf, wenn die App startet; nur bei Bewegung Voll)",
  sound: "Ton",
  sounds: "Töne",
  soundsHint: "(nur, während Axiomata im Hintergrund ist)",
  canvas: "Canvas",
  showGrid: "Punktraster zeigen",
  showGridHint: "(Kacheln rasten so oder so daran ein)",
  magneticEdges: "Magnetische Kanten",
  magneticEdgesHint: "(Kacheln haften bis 8 px an ihren Nachbarn)",
  orbitBrain: "Orbit & Second Brain",
  speed: "Tempo",
  labels: "Beschriftungen",
  labelsHint: "(Skills, Bereiche und die Mitte)",
  fileNames: "Dateinamen",
  fileNamesHint: "(neben jeder Notiz im Second Brain)",
  shimmer: "Schimmer",
  shimmerHint: "(wie die Hex-Zellen das Licht fangen; still, außer bei Bewegung Voll)",
  layout: "Anordnung",
  layoutHint: "(das Second Brain: ein Ring gebündelter Links, Länder oder eine Wabe)",
  uiSize: "UI-Größe",
  auto: "Auto",
  autoHint: "({percent} % auf diesem Bildschirm)",
  textAndControls: "Text und Bedienelemente",
  uiSizeHint:
    "„Auto“ folgt der echten Pixeldichte des Bildschirms, auf dem das Fenster liegt: auf einem dichten Monitor " +
    "wächst die Oberfläche, damit sie sich so gut liest wie auf einem MacBook. Die Schriften von Editor und " +
    "Terminal bleiben, wie sie sind.",
  keyboard: "Tastatur",
  shortcutsFollow: "Tastenkürzel nach",
  keyboardHint:
    "Mac: ⌘ ist die Haupttaste. Windows & Linux: Ctrl ist es, und im Terminal nimmt ein App-Kürzel Ctrl+Shift, " +
    "weil Ctrl dort der Shell gehört. „Auto“ folgt diesem Rechner; mit den anderen beiden kannst du die anderen " +
    "Tasten hier ausprobieren.",
  glass: "Glas",
  transparency: "Transparenz",
  blur: "Unschärfe",
  backgroundImage: "Hintergrundbild",
  chooseAnother: "Anderes wählen…",
  chooseImage: "Bild wählen…",
  remove: "Entfernen",
  imageNotTaken: "Das Bild wurde nicht übernommen: {error}",
  imageNotRemoved: "Das Bild ließ sich nicht entfernen: {error}",
  imageDim: "Bild abdunkeln",
  pixels: "{n} px",
  glassHint:
    "Transparenz: wie viel vom Grund durch die Flächen scheint — 0 % deckend, 100 % klar, 50 % der eigene Look " +
    "des Themes. Unschärfe: wie weich der Grund ist, und mit ihm alles, was man durchs Glas sieht. Der Grund ist " +
    "der des Themes oder dein Bild, einmal weichgezeichnet, also kostet ein weicherer nichts, während du arbeitest. " +
    "Bild abdunkeln: wie stark der Grund des Themes dein Bild verschleiert — dunkler in einem dunklen Theme, " +
    "heller in einem hellen —, damit Text lesbar bleibt. Menüs und Text-Overlays bleiben so oder so dicht, und " +
    "„Transparenz reduzieren“ von macOS macht alles deckend.",
  reduceTransparencyOn: "„Transparenz reduzieren“ von macOS ist an: das Glas ist jetzt deckend.",
  customCss: "Eigenes CSS",
  notLoaded: "Nicht geladen.",
  reloading: "Lädt neu…",
  reloadCss: "Eigenes CSS neu laden",
  copyTemplate: "Vorlage kopieren",
  templateCopied: "Vorlage kopiert — speichere sie als ~/.axiomata/theme.css.",
  customAbsent: "Keine ~/.axiomata/theme.css.",
  customApplied: "Eigenes CSS angewendet.",
  customRejected: "Eigenes CSS abgelehnt; das eingebaute Theme bleibt.",

  motionFull: "Voll",
  motionCalm: "Ruhig",
  motionOff: "Aus",
  cueWaiting: "Ein Agent wartet auf dich",
  cueDone: "Ein Agent hat seinen Schritt beendet",
  cueRoutine: "Eine der Routines wurde ausgelöst",
  shineCalm: "Ruhig",
  shineTwinkle: "Funkeln",
  shineMetal: "Metall",
  layoutBundle: "Bündel",
  layoutMap: "Karte",
  layoutHex: "Hex",
  keyboardAuto: "Auto",
  keyboardMac: "Mac",
  keyboardOther: "Windows & Linux",

  blurbGraphite: "Fast schwarz, orangener Akzent, Hex-Gitter",
  blurbPaper: "Warmes Creme, Tinte als Schrift, Lehm-Akzent",
  blurbSteampunk: "Bronze und Leder, Messing-Akzent",
  blurbForest: "Kiefer und Moos, Akzent in Herbstbernstein",
  blurbOcean: "Tiefseeblau, Korallen-Akzent",
  blurbCatppuccinMocha: "Sanft dunkel, Mauve-Akzent",
  blurbCatppuccinMacchiato: "Sanft dunkel, eine Spur heller",
  blurbCatppuccinFrappe: "Sanft gedämpft, Mauve-Akzent",
  blurbCatppuccinLatte: "Sanft hell, Mauve-Akzent",
  blurbCatppuccinEspresso: "Macchiato auf fast Schwarz",
  blurbTokyoNight: "Tiefes Indigo, blauer Akzent",
  blurbGithubDark: "Vertrautes Dunkel, blauer Akzent",
  blurbGithubLight: "Vertrautes Hell, blauer Akzent",

  harnessClaudeCode: "Nur Anthropic, mit der eigenen Anmeldung der App",
  harnessGlut: "Axiomatas eigener Harness — OpenRouter oder Ollama",
  webExaHint: "Geht auch ohne Schlüssel (begrenzt); ein Schlüssel hebt das Limit an",
  webTinyfishHint: "Wird gefragt, wenn Exa begrenzt ist oder scheitert",
  webTavilyHint: "Wird zuletzt gefragt, nur bis zum Monatslimit unten (auf dem Konto liegt eine Karte)",

  vault: "Vault",
  vaultLead: "Wurzelordner des Second-Brain-Workspace. Eine Änderung wirkt nach einem Neustart.",
  path: "Pfad",
  vaultPlaceholder: "/Users/…/Vault",
  saving: "Speichert…",
  saveRestart: "Speichern & neu starten",
  vaultEmpty: "Der Vault-Pfad darf nicht leer sein.",
  vaultSavedRestart: "Vault-Pfad gespeichert — starte Axiomata-OS neu, um zu wechseln.",
  vaultSaved: "Vault-Pfad gespeichert.",
  savingFailed: "Speichern fehlgeschlagen: {error}",

  editProviders: "Anbieter bearbeiten",
  editProvidersLead:
    "Ein Anbieter ist, wo das Modell läuft — sein Endpoint, sein Schlüssel und seine Modelle. Jeder Anbieter wird " +
    "hier für sich eingerichtet; welcher Harness und welcher Anbieter Chat und Skills bedienen, legst du im " +
    "nächsten Abschnitt fest.",
  badgeChat: "Chat",
  badgeSkills: "Skills",
  blurbAnthropic: "Über das Abo abgerechnet — kein Schlüssel",
  blurbOpenRouter: "Anthropic-kompatibler Endpoint",
  blurbOllama: "Lokales Modell auf diesem Rechner",
  anthropicNoKey:
    "Kein Schlüssel und keine Basis-URL nötig — Anthropic wird über den CLI-Login des Abos abgerechnet.",
  baseUrl: "Basis-URL",
  apiKey: "API-Schlüssel",
  apiKeyOptional: "API-Schlüssel (optional)",
  keySaved: "•••••• gespeichert — leer lassen, um ihn zu behalten",
  keyOllama: "Für ein lokales Ollama reicht ein Platzhalter",
  keyRequired: "erforderlich",
  keyDeletedOnSave: "Der Schlüssel wird beim Speichern gelöscht.",
  chatModel: "Chat-Modell",
  skillsModel: "Modell für Skills/Routines",
  modelExample: "z. B. {model}",
  modelOverrideBefore: "Ein",
  modelOverrideAfter: "im Frontmatter der SKILL.md hat weiterhin Vorrang vor dem Skills-Modell.",
  unknownProvider:
    "Unbekannter Anbieter „{id}“ — keine Einstellungen zum Bearbeiten. Wähle oben einen der bekannten Anbieter.",
  saveProvider: "Anbieter speichern",
  providersSaved: "Anbieter-Einstellungen gespeichert — sie gelten ab dem nächsten Agent-Aufruf.",

  roleChat: "Chat",
  roleSkills: "Skills",
  fieldChatModel: "Chat-Modell",
  fieldSkillsModel: "Skills-Modell",
  noProviderSettings: "{role}: Anbieter „{id}“ hat keine Einstellungen.",
  needsKey: "{role} ({provider}) braucht einen API-Schlüssel.",
  modelEmpty: "{role} ({provider}): {field} darf nicht leer sein.",
  modelSpaces: "{role} ({provider}): {field} enthält Leerzeichen.",
  modelBrackets: "{role} ({provider}): {field} hat unpaarige Klammern — „{value}“.",
  baseUrlHttps: "{role} ({provider}): die Basis-URL muss mit https:// beginnen.",

  catalogBuiltIn: "Eingebaute Liste — Anthropic hat ohne API-Schlüssel keine Modellliste zum Abfragen.",
  catalogNone: "Noch keine Modellliste.",
  catalogFailed: "Der letzte Abruf ist fehlgeschlagen: {error}",
  catalogFetched: {
    one: "{n} Modell, abgerufen {when}; aktualisiert beim Start und einmal am Tag.",
    other: "{n} Modelle, abgerufen {when}; aktualisiert beim Start und einmal am Tag.",
  },
  pricesNotSaved: "Die Preise ließen sich nicht speichern: {error}",
  pricesUpdated: "Preise aktualisiert: {models}",
  modelListFailed: "Die Modellliste ließ sich nicht abrufen: {error}",

  harnessAssignment: "Harness & Anbieter zuordnen",
  harnessLeadBefore:
    "Der Harness steuert den Agenten — seine Werkzeuge, Rechte und Schleife; der Anbieter liefert das Modell. Jede " +
    "Rolle wählt beides, z. B. Claude Code für den Chat und Glut mit einem lokalen Ollama für die Digests. Das " +
    "eigene",
  harnessLeadAfter: "eines Skills hat weiterhin Vorrang vor dem hier gesetzten Harness.",
  forChat: "Chat",
  forSkills: "Skills & Routines",
  harnessFor: "Harness für {role}",
  providerFor: "Anbieter für {role}",
  effortFor: "Effort für {role}",
  glutOwnEffort: "Einstellung von Glut",
  glutOwnEffortElse: "Einstellung von Glut, sonst {level}",
  effortNotListed: "{level} (nicht gelistet)",
  webSkillBefore: "Ein Skill, der im Web liest (",
  webSkillAfter: "), nutzt die Websuche von Glut — ihre Schlüssel und Grenzen stehen unten im Block Websuche.",

  claudeSignIn: "Claude-Code-Anmeldung",
  signedInAs: "Angemeldet als {email}",
  unknownEmail: "unbekannt",
  ownLogin: "— der eigene Login der App, getrennt von deinem in",
  claudeUnreachable: "Claude Code lässt sich nicht fragen: {error}",
  notSignedIn:
    "Nicht angemeldet — Chat und Skills auf Claude Code schlagen fehl, bis sich das Claude Code der App anmeldet.",
  waitingForBrowser: "Wartet auf den Browser…",
  signInAnthropic: "Mit Anthropic anmelden",
  signInCode: "Code (nur, wenn die Browserseite einen zeigt)",
  send: "Senden",
  signOut: "Abmelden",
  signedIn: "Claude Code ist angemeldet.",
  signInIncomplete: "Die Anmeldung wurde nicht abgeschlossen.",
  signInFailed: "Anmeldung fehlgeschlagen: {error}",
  codeNotTaken: "Der Code wurde nicht angenommen: {error}",
  signedOut: "Claude Code ist abgemeldet.",
  signOutFailed: "Abmelden fehlgeschlagen: {error}",

  webSearch: "Websuche (Glut)",
  webSearchBefore: "Glut fragt mit",
  webSearchMiddle:
    "zuerst Exa, dann TinyFish, dann Tavily — ein Anbieter ohne Schlüssel wird übersprungen. Die Schlüssel " +
    "liegen in",
  webSearchAfter: "und werden nie wieder angezeigt.",
  webKey: "{name}-Schlüssel",
  optional: "optional",
  notSet: "nicht gesetzt",
  searchesPerDay: "Suchen pro Tag (alle Anbieter)",
  tavilyPerMonth: "Tavily-Suchen pro Monat",
  webLimitsHint: "0 schaltet die Suche (oder Tavily) ab. {n} Tavily-Suchen im Monat sind ihr Gratis-Kontingent.",

  spend: "Ausgaben",
  spendToday: "{amount} heute",
  spendCap: "/ {amount} Limit",
  spendMonth: "· {amount} diesen Monat",
  roleToProvider: "{role} → {provider}",
  subscriptionBilled: "{role} → {provider} wird über das Abo abgerechnet — keine Kostenerfassung.",
  dailyLimit: "Tageslimit (USD, bezahlte Anbieter)",
  noLimit: "leer = kein Limit",
  dailyLimitHint:
    "Sobald die heutige Summe das Limit erreicht, wird der nächste Agent-Aufruf über einen bezahlten Anbieter " +
    "abgelehnt.",
  saveAssignment: "Zuordnung speichern",

  owner: "Owner",
  ownerUnset: "— (setze `owner` in ~/.axiomata/config.toml)",
  workspace: "Workspace",
  version: "Version",
});
