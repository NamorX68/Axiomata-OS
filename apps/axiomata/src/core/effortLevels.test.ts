import { describe, expect, it } from "vitest";

import { EFFORT_LEVELS } from "./backend";
import {
  defaultLabel,
  effortState,
  levelsFor,
  offeredLevels,
  reasoningOf,
  takesLevel,
  unlistedNote,
} from "./effortLevels";
import type { ModelCatalog, ModelReasoning } from "./modelCatalog";

const flash: ModelReasoning = { efforts: ["none", "low", "high", "max"], default_effort: "high", mandatory: false };

const catalog: ModelCatalog = {
  version: 1,
  providers: {
    open_router: {
      models: [
        { id: "deepseek/flash", name: "Flash", reasoning: flash },
        { id: "plain/model", name: "Plain" },
      ],
      fetched_at: null,
      error: null,
    },
  },
};

describe("effort levels", () => {
  it("finds a model's levels in the catalog and says nothing for an unknown model or provider", () => {
    expect(reasoningOf(catalog, "open_router", "deepseek/flash")).toEqual(flash);
    expect(reasoningOf(catalog, "open_router", "plain/model")).toBeUndefined();
    expect(reasoningOf(catalog, "ollama", "deepseek/flash")).toBeUndefined();
    expect(reasoningOf(null, "open_router", "deepseek/flash")).toBeUndefined();
  });

  it("offers the model's levels least first, every level when the catalog does not say", () => {
    expect(levelsFor(flash)).toEqual(["none", "low", "high", "max"]);
    expect(levelsFor(undefined)).toEqual(EFFORT_LEVELS);
    expect(takesLevel(flash, "medium")).toBe(false);
    expect(takesLevel(undefined, "medium")).toBe(true);
  });

  it("keeps a chosen level the model does not list, in its place, so a select can show it", () => {
    expect(offeredLevels(flash, "medium")).toEqual(["none", "low", "medium", "high", "max"]);
    expect(offeredLevels(flash, "high")).toEqual(["none", "low", "high", "max"]);
    expect(offeredLevels(flash, null)).toEqual(["none", "low", "high", "max"]);
  });

  it("names the model's own default where the catalog knows it", () => {
    expect(defaultLabel(flash)).toBe("default (high)");
    expect(defaultLabel(undefined)).toBe("default");
  });

  it("takes the conversation's level over the configured one over the defaults", () => {
    expect(effortState("max", "low", flash)).toMatchObject({ level: "max", source: "conversation" });
    expect(effortState("max", "low", flash).explanation).toContain("the configured low");
    expect(effortState(null, "low", flash)).toMatchObject({ level: "low", source: "settings" });
    const none = effortState(null, null, flash);
    expect(none).toMatchObject({ level: null, source: "default" });
    expect(none.explanation).toContain("model's default (high)");
    expect(effortState(null, null, undefined).explanation).toContain("provider's default");
  });

  it("notes a level the model does not list, and only such a level", () => {
    expect(unlistedNote(flash, "medium")).toMatch(/does not list medium/);
    expect(unlistedNote(flash, "high")).toBeNull();
    expect(unlistedNote(flash, null)).toBeNull();
    expect(unlistedNote(undefined, "medium")).toBeNull();
  });
});
