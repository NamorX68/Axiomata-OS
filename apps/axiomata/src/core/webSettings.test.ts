import { describe, expect, it } from "vitest";

import type { WebSettingsView } from "./backend";
import { WEB_KEYS, webKeyUpdate, webUpdate } from "./webSettings";

const VIEW: WebSettingsView = {
  has_exa_key: false,
  has_tinyfish_key: true,
  has_tavily_key: true,
  daily_search_limit: 50,
  tavily_monthly_limit: 1000,
};

describe("web search settings", () => {
  it("asks Exa first, TinyFish second and Tavily last", () => {
    expect(WEB_KEYS.map((k) => k.id)).toEqual(["exa", "tinyfish", "tavily"]);
    expect(WEB_KEYS.map((k) => k.stored(VIEW))).toEqual([false, true, true]);
  });

  it("keeps an untouched key, clears an emptied one and sets a typed one", () => {
    expect(webKeyUpdate({}, "tavily")).toEqual({ kind: "keep" });
    expect(webKeyUpdate({ tavily: "  " }, "tavily")).toEqual({ kind: "clear" });
    expect(webKeyUpdate({ tinyfish: " tf-1 " }, "tinyfish")).toEqual({ kind: "set", value: "tf-1" });
  });

  it("sends the limits as whole non-negative numbers", () => {
    const update = webUpdate({ ...VIEW, daily_search_limit: 12.7, tavily_monthly_limit: -3 }, { exa: "e" });
    expect(update.daily_search_limit).toBe(12);
    expect(update.tavily_monthly_limit).toBe(0);
    expect(update.exa_api_key).toEqual({ kind: "set", value: "e" });
    expect(update.tavily_api_key).toEqual({ kind: "keep" });
  });

  it("takes an emptied limit field as the default, never as 0", () => {
    const emptied = { ...VIEW, daily_search_limit: null, tavily_monthly_limit: Number.NaN } as unknown as WebSettingsView;
    const update = webUpdate(emptied, {});
    expect(update.daily_search_limit).toBe(50);
    expect(update.tavily_monthly_limit).toBe(1000);
    expect(webUpdate({ ...VIEW, daily_search_limit: 0 }, {}).daily_search_limit).toBe(0);
  });
});
