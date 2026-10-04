import { describe, expect, it } from "vitest";

import { FILE_ICON_KEYS } from "../ui/icons/fileIcons";
import { LUCIDE } from "../ui/icons/lucide";
import { FILE_ICON_STYLES, fileIconDrawing, fileIconKey } from "./fileIcons";

describe("file icons (LK4, K5)", () => {
  it("knows a file by its name first, then its extension, then its language", () => {
    expect(fileIconKey("Cargo.toml")).toBe("cargo");
    expect(fileIconKey("crates/x/Cargo.lock")).toBe("lock");
    expect(fileIconKey("package.json")).toBe("npm");
    expect(fileIconKey(".gitignore")).toBe("git");
    expect(fileIconKey("Dockerfile")).toBe("docker");
    expect(fileIconKey("LICENSE")).toBe("license");
    expect(fileIconKey("README.md")).toBe("readme");
    expect(fileIconKey(".env.local")).toBe("env");
    expect(fileIconKey("logo.svg")).toBe("image");
    expect(fileIconKey("App.tsx")).toBe("react");
    expect(fileIconKey("notes.txt")).toBe("text");
    expect(fileIconKey("main.rs")).toBe("rust");
    expect(fileIconKey("other.toml")).toBe("toml");
    expect(fileIconKey("Idee.md")).toBe("markdown");
    expect(fileIconKey("data.bin")).toBe("file");
  });

  it("has a drawing for every kind in every style, the JetBrains variant following the theme", () => {
    for (const style of FILE_ICON_STYLES) {
      for (const key of FILE_ICON_KEYS) {
        const drawing = fileIconDrawing(style, key, false);
        if (style === "none") expect(drawing).toBeNull();
        else if (drawing?.kind === "lucide") expect(LUCIDE[drawing.name], `${style}/${key}`).toBeTruthy();
        else expect(drawing?.body, `${style}/${key}`).toBeTruthy();
      }
    }
    const dark = fileIconDrawing("jetbrains", "json", false);
    const light = fileIconDrawing("jetbrains", "json", true);
    expect(dark && light && dark.kind === "glyph" && light.kind === "glyph" && dark.body !== light.body).toBe(true);
  });
});
