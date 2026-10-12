// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { portal } from "./portal";

describe("the portal action", () => {
  it("moves the element under body and removes it again", () => {
    const host = document.createElement("div");
    const node = document.createElement("span");
    host.appendChild(node);
    const action = portal(node);
    expect(node.parentElement).toBe(document.body);
    action.destroy();
    expect(node.isConnected).toBe(false);
  });
});
