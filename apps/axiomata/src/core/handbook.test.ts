import { beforeAll, describe, expect, it } from "vitest";

import { registerBuiltins } from "../modules";
import { listAllRingEligibleBuiltins } from "./apps";
import {
  PAGE_FOR_PLACE,
  pageForRingEntry,
  pageForSettingsTab,
  pageForTopic,
  pageOfLink,
  withPageLinks,
} from "./handbook";
import { renderMarkdown } from "./markdown";
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

describe("links and topics", () => {
  const entries = [
    { name: "studio", title: "Studio", summary: "" },
    { name: "second-brain", title: "Second Brain", summary: "" },
    { name: "spotlight", title: "Spotlight", summary: "" },
    { name: "kosten", title: "Kosten", summary: "" },
  ];
  const known = new Set(entries.map((entry) => entry.name));

  it("reads a link to another page", () => {
    expect(pageOfLink("studio.md", known)).toBe("studio");
    expect(pageOfLink("./studio.md#handgriffe", known)).toBe("studio");
    expect(pageOfLink("nope.md", known)).toBeUndefined();
    expect(pageOfLink("../studio.md", known)).toBeUndefined();
    expect(pageOfLink("https://example.com/studio.md", known)).toBeUndefined();
  });

  it("keeps a page link through the sanitiser, bare or not", () => {
    const html = renderMarkdown(withPageLinks("[A](studio.md) [B](./kosten.md#x) [C](https://x.org/a.md)"));
    const hrefs = [...new DOMParser().parseFromString(html, "text/html").querySelectorAll("a")].map(
      (a) => a.getAttribute("href") ?? "",
    );
    expect(hrefs.map((href) => pageOfLink(href, known))).toEqual(["studio", "kosten", undefined]);
  });

  it("finds the page a topic means", () => {
    expect(pageForTopic("studio", entries)).toBe("studio");
    expect(pageForTopic("Second Brain", entries)).toBe("second-brain");
    expect(pageForTopic("kos", entries)).toBe("kosten");
    expect(pageForTopic("s", entries), "several start so").toBeUndefined();
    expect(pageForTopic("  ", entries)).toBeUndefined();
  });
});
