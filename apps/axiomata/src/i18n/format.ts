/** The words of `core/format.ts`: how long ago, how long until, which day. */

import { texts } from "./texts";

const en = {
  never: "never",
  justNow: "just now",
  minutesAgo: "{n} min ago",
  hoursAgo: "{n} h ago",
  daysAgo: { one: "1 d ago", other: "{n} d ago" },
  none: "—",
  due: "due",
  inUnderAMinute: "in <1 min",
  inMinutes: "in {n} min",
  inHours: "in {n} h",
  inDays: { one: "in 1 d", other: "in {n} d" },
  today: "Today",
  tomorrow: "Tomorrow",
};

export const tFormat = texts("format", en, {
  never: "nie",
  justNow: "gerade eben",
  minutesAgo: "vor {n} Min.",
  hoursAgo: "vor {n} Std.",
  daysAgo: { one: "vor 1 Tag", other: "vor {n} Tagen" },
  none: "—",
  due: "fällig",
  inUnderAMinute: "in <1 Min.",
  inMinutes: "in {n} Min.",
  inHours: "in {n} Std.",
  inDays: { one: "in 1 Tag", other: "in {n} Tagen" },
  today: "Heute",
  tomorrow: "Morgen",
});
