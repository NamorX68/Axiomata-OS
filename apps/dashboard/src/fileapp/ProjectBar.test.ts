import { mount, flushSync, unmount } from "svelte";
import { describe, expect, it, vi } from "vitest";

import ProjectBar from "./ProjectBar.svelte";

describe("ProjectBar", () => {
  it("keeps the menu open and shows the form after a click on New project…", async () => {
    const onNew = vi.fn();
    const target = document.body.appendChild(document.createElement("div"));
    const app = mount(ProjectBar, {
      target,
      props: { projects: [], current: null, onPick: vi.fn(), onOpenFolder: vi.fn(), onNew, onClose: vi.fn() },
    });
    flushSync();
    (target.querySelector(".current") as HTMLElement).click();
    flushSync();
    const add = [...target.querySelectorAll("button.action")].find((b) => b.textContent?.includes("New project")) as HTMLElement;
    // A real click lets Svelte's queued update run between two listeners of the same event
    // (a microtask checkpoint), so the clicked button is already gone when the window listener looks.
    const flushBetweenListeners = () => flushSync();
    document.body.addEventListener("click", flushBetweenListeners);
    add.click();
    document.body.removeEventListener("click", flushBetweenListeners);
    flushSync();
    expect(target.querySelector(".new-form")).not.toBeNull();
    const input = target.querySelector(".new-form input[type=text]") as HTMLInputElement;
    input.value = "demo";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    flushSync();
    (target.querySelector(".new-form button.primary") as HTMLElement).click();
    flushSync();
    expect(onNew).toHaveBeenCalledWith("demo", true);
    unmount(app);
  });

  it("says what is wrong with an empty name instead of doing nothing", () => {
    const onNew = vi.fn();
    const target = document.body.appendChild(document.createElement("div"));
    const app = mount(ProjectBar, {
      target,
      props: { projects: [], current: null, onPick: vi.fn(), onOpenFolder: vi.fn(), onNew, onClose: vi.fn() },
    });
    flushSync();
    (target.querySelector(".current") as HTMLElement).click();
    flushSync();
    ([...target.querySelectorAll("button.action")].find((b) => b.textContent?.includes("New project")) as HTMLElement).click();
    flushSync();
    (target.querySelector(".new-form button.primary") as HTMLElement).click();
    flushSync();
    expect(target.querySelector(".problem")?.textContent).toMatch(/Enter a folder name/);
    expect(onNew).not.toHaveBeenCalled();
    unmount(app);
  });
});
