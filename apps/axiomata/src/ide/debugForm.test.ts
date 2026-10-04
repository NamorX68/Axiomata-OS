import { describe, expect, it } from "vitest";

import { formOf, splitArgs, toNewConfig } from "./debugForm";

describe("debug form", () => {
  it("splits arguments, keeping quoted words together", () => {
    expect(splitArgs(`--port 8000 "two words" 'x y'`)).toEqual(["--port", "8000", "two words", "x y"]);
    expect(splitArgs("   ")).toEqual([]);
  });

  it("tells a file from a module", () => {
    expect(toNewConfig({ name: " A ", target: "src/ocht/main.py", args: "", cwd: "" })).toEqual({
      name: "A", program: "src/ocht/main.py", module: null, args: [], cwd: null,
    });
    expect(toNewConfig({ name: "B", target: "ocht.cli", args: "-v", cwd: "backend" })).toEqual({
      name: "B", program: null, module: "ocht.cli", args: ["-v"], cwd: "backend",
    });
  });

  it("round-trips a saved configuration into the form", () => {
    const form = formOf({
      name: "App", language: "python", program: null, module: "ocht", args: ["--name", "a b"], cwd: null,
      env: [], just_my_code: true, detected: false,
    });
    expect(form).toEqual({ name: "App", target: "ocht", args: `--name "a b"`, cwd: "" });
  });
});
