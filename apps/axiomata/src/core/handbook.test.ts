import { beforeAll, describe, expect, it } from "vitest";

import { registerBuiltins } from "../modules";
import { listAllRingEligibleBuiltins } from "./apps";
import { PAGE_FOR_PLACE, pageForRingEntry, pageForSettingsTab } from "./handbook";
import { SETTINGS_TABS } from "./settingsTabs";

// The pages as the build embeds them: the same folder `axiomata_core::handbook` reads with `include_str!`.
const FILES = import.meta.glob("../../../../docs/handbook/*.md", { query: "?raw", import: "default", eager: true });
const PAGES = new Set(
  Object.keys(FILES)
    .map((path) => path.split("/").pop()!.replace(/\.md$/, ""))
    .filter((name) => name !== "README"),
);

beforeAll(() => registerBuiltins());

describe("the handbook covers every place of the app", () => {
  it("finds the handbook's pages", () => {
    expect(PAGES.has("index")).toBe(true);
    expect(PAGES.size).toBeGreaterThanOrEqual(20);
  });

  it("maps every App Ring entry to a page", () => {
    const missing = listAllRingEligibleBuiltins()
      .map((entry) => entry.type)
      .filter((type) => !pageForRingEntry(type));
    expect(missing, "ring entries without a handbook page in PAGE_FOR_PLACE").toEqual([]);
  });

  it("maps every Settings tab to a page", () => {
    const missing = SETTINGS_TABS.map((tab) => tab.id).filter((id) => !pageForSettingsTab(id));
    expect(missing, "settings tabs without a handbook page in PAGE_FOR_PLACE").toEqual([]);
  });

  it("names only pages that exist", () => {
    const unknown = Object.entries(PAGE_FOR_PLACE).filter(([, page]) => !PAGES.has(page));
    expect(unknown).toEqual([]);
  });

  it("keeps no entry for a place that is gone", () => {
    const places = new Set([
      ...listAllRingEligibleBuiltins().map((entry) => `ring:${entry.type}`),
      ...SETTINGS_TABS.map((tab) => `settings:${tab.id}`),
    ]);
    expect(Object.keys(PAGE_FOR_PLACE).filter((place) => !places.has(place))).toEqual([]);
  });
});
