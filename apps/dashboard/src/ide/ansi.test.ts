import { describe, expect, it } from "vitest";

import { isFullScreen, stripAnsi } from "./ansi";

describe("console output", () => {
  it("drops colours and cursor moves, keeps the text", () => {
    expect(stripAnsi("\u001b[38;2;254;166;43mhello\u001b[0m \u001b[21;80Hworld")).toBe("hello world");
    expect(stripAnsi("\u001b]0;title\u0007plain")).toBe("plain");
  });

  it("knows a full-screen program by its switch to the alternate screen", () => {
    expect(isFullScreen("\u001b[?1049h\u001b[2J")).toBe(true);
    expect(isFullScreen("\u001b[31mred\u001b[0m")).toBe(false);
  });
});
