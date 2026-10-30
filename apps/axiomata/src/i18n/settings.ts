/** The texts of Settings (`shell/Settings.svelte` and its parts). */

import { texts } from "./texts";

const en = {
  language: "Language",
  languageSystem: "System ({lang})",
  languageHint:
    "The app's buttons, notes and the handbook. Names like Studio, Canvas, Flow or Skills stay as they are; " +
    "the agents always work in English.",
  langDe: "Deutsch",
  langEn: "English",
};

export const tSettings = texts("settings", en, {
  language: "Sprache",
  languageSystem: "System ({lang})",
  languageHint:
    "Knöpfe, Hinweise und das Handbuch der App. Namen wie Studio, Canvas, Flow oder Skills bleiben, wie sie sind; " +
    "die Agenten arbeiten immer auf Englisch.",
  langDe: "Deutsch",
  langEn: "English",
});
