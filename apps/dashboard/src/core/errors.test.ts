import { describe, expect, it } from "vitest";

import { messageOf } from "./errors";

describe("messageOf", () => {
  it("returns a string error as-is", () => {
    expect(messageOf("plain string failure")).toBe("plain string failure");
  });

  it("reads an Error's own message, including a subclass", () => {
    expect(messageOf(new Error("bad thing happened"))).toBe("bad thing happened");
    expect(messageOf(new TypeError("wrong type"))).toBe("wrong type");
  });

  it("reads the message field off a file-service-shaped error ({ kind, message })", () => {
    expect(messageOf({ kind: "NotFound", message: "no such file" })).toBe("no such file");
  });

  it("falls back to String() for an object with no message field", () => {
    expect(messageOf({ kind: "NotFound" })).toBe("[object Object]");
  });

  it("falls back to String() for null, undefined and primitives", () => {
    expect(messageOf(null)).toBe("null");
    expect(messageOf(undefined)).toBe("undefined");
    expect(messageOf(42)).toBe("42");
  });
});
