import { get } from "svelte/store";
import { describe, expect, it, vi } from "vitest";

import type { DebugEvent, StackFrame } from "./debugBackend";
import { createDebugController, IDLE, MAX_OUTPUT_LINES, reduce, type Backend } from "./debugSession";

const frame = (id: number, line: number, name = "f"): StackFrame => ({ id, name, path: "/p/a.py", line, column: 1 });

function fakeBackend(): { backend: Backend; emit: (e: DebugEvent) => void; calls: string[] } {
  let sink: (e: DebugEvent) => void = () => {};
  const calls: string[] = [];
  const backend: Backend = {
    start: vi.fn(async (_r, _t, _b, _term, _args, onEvent) => {
      sink = onEvent;
      calls.push("start");
    }),
    stop: vi.fn(async () => void calls.push("stop")),
    control: vi.fn(async (a) => void calls.push(`control:${a}`)),
    stack: vi.fn(async () => [frame(10, 7, "add"), frame(11, 12, "<module>")]),
    scopes: vi.fn(async () => [
      { name: "Locals", variables_reference: 100, expensive: false },
      { name: "Globals", variables_reference: 200, expensive: true },
    ]),
    variables: vi.fn(async (ref) => (ref === 100 ? [{ name: "a", value: "2", type_name: "int", variables_reference: 0 }] : [])),
    evaluate: vi.fn(async (expr) => ({ name: expr, value: "5", type_name: null, variables_reference: 0 })),
    setBreakpoints: vi.fn(async () => []),
  };
  return { backend, emit: (e) => sink(e), calls };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("debug state", () => {
  it("watch expressions are evaluated in the shown frame at a stop, and survive the next session", async () => {
    const { backend, emit } = fakeBackend();
    const c = createDebugController(backend);
    c.setWatches(["a + b", "bad("]);
    vi.mocked(backend.evaluate).mockImplementation(async (expr) => {
      if (expr === "bad(") throw new Error("SyntaxError");
      return { name: expr, value: "5", type_name: null, variables_reference: 0 };
    });
    await c.start("project:1", { kind: "current_file", rel: "a.py" }, [], "x");
    emit({ event: "stopped", thread_id: 1, reason: "breakpoint", text: null });
    await tick();
    await tick();
    expect(get(c.state).watches).toEqual([
      { expr: "a + b", value: "5", error: null },
      { expr: "bad(", value: null, error: "SyntaxError" },
    ]);
    expect(backend.evaluate).toHaveBeenCalledWith("a + b", 10, "watch");
    // A new session keeps the expressions and clears the old answers.
    await c.start("project:1", { kind: "current_file", rel: "a.py" }, [], "x");
    expect(get(c.state).watches.map((w) => [w.expr, w.value])).toEqual([["a + b", null], ["bad(", null]]);
    c.removeWatch(0);
    expect(get(c.state).watches.map((w) => w.expr)).toEqual(["bad("]);
  });

  it("a stop in code outside the project shows the first frame that is the project's own", async () => {
    const { backend, emit } = fakeBackend();
    backend.stack = vi.fn(async () => [
      { id: 1, name: "rust_panic", path: "/rustc/abc/library/std/src/panicking.rs", line: 9, column: 1 },
      { id: 2, name: "check", path: "/p/main.rs", line: 3, column: 1 },
    ]);
    const onStop = vi.fn();
    const c = createDebugController(backend, { onStop, isUserFrame: (f) => f.path?.startsWith("/p/") ?? false });
    await c.start("project:1", { kind: "named", name: "x" }, [], "x");
    emit({ event: "stopped", thread_id: 1, reason: "breakpoint", text: null });
    await tick();
    await tick();
    expect(onStop).toHaveBeenCalledTimes(1);
    expect(onStop.mock.calls[0][0].id).toBe(2);
    expect(get(c.state).frameId).toBe(2);
  });

  it("the arguments typed for a run go to the backend", async () => {
    const { backend } = fakeBackend();
    const c = createDebugController(backend);
    await c.start("project:1", { kind: "named", name: "x" }, [], "x", false, ["--only", "adds"]);
    expect(vi.mocked(backend.start).mock.calls[0][4]).toEqual(["--only", "adds"]);
  });

  it("a program the adapter wants in a terminal goes to the host, and changes nothing in the state", async () => {
    const { backend, emit } = fakeBackend();
    const onTerminal = vi.fn();
    const c = createDebugController(backend, { onTerminal });
    await c.start("project:1", { kind: "named", name: "App" }, [], "App", true);
    const before = get(c.state);
    emit({ event: "run_in_terminal", title: "Python Debug Console", line: "cd '/p' && 'python' 'x.py'" });
    expect(onTerminal).toHaveBeenCalledWith("Python Debug Console", "cd '/p' && 'python' 'x.py'");
    expect(get(c.state)).toEqual(before);
    expect(vi.mocked(backend.start).mock.calls[0][3]).toBe(true);
  });

  it("a stop shows the stack, the scopes and the locals, and tells the editor where", async () => {
    const { backend, emit } = fakeBackend();
    const onStop = vi.fn();
    const c = createDebugController(backend, { onStop });
    await c.start("project:1", { kind: "current_file", rel: "a.py" }, [], "Current file");
    expect(get(c.state).phase).toBe("running");

    emit({ event: "stopped", thread_id: 1, reason: "breakpoint", text: null });
    await tick();
    await tick();
    const s = get(c.state);
    expect(s.phase).toBe("stopped");
    expect(s.frames.map((f) => f.name)).toEqual(["add", "<module>"]);
    expect(s.frameId).toBe(10);
    expect(s.scopes[0].variables?.[0].name).toBe("a");
    expect(s.scopes[1].variables).toBeNull(); // the expensive scope waits
    expect(onStop).toHaveBeenCalledWith(expect.objectContaining({ id: 10, line: 7 }));
  });

  it("continuing clears the stop, and a slow stack of the old stop never lands", async () => {
    const { backend, emit } = fakeBackend();
    let release: (f: StackFrame[]) => void = () => {};
    backend.stack = vi.fn(() => new Promise<StackFrame[]>((r) => (release = r)));
    const c = createDebugController(backend);
    await c.start("project:1", { kind: "named", name: "x" }, [], "x");
    emit({ event: "stopped", thread_id: 1, reason: "breakpoint", text: null });
    emit({ event: "continued", thread_id: 1 });
    release([frame(1, 1)]);
    await tick();
    expect(get(c.state).phase).toBe("running");
    expect(get(c.state).frames).toEqual([]);
  });

  it("a breakpoint hit during start keeps the stopped state", async () => {
    const { backend } = fakeBackend();
    let sink: (e: DebugEvent) => void = () => {};
    backend.start = vi.fn(async (_r, _t, _b, _term, _args, onEvent) => {
      sink = onEvent;
      sink({ event: "stopped", thread_id: 1, reason: "breakpoint", text: null });
    });
    const c = createDebugController(backend);
    await c.start("project:1", { kind: "named", name: "x" }, [], "x");
    expect(get(c.state).phase).toBe("stopped");
  });

  it("stepping only while stopped; pause any time", async () => {
    const { backend, emit, calls } = fakeBackend();
    const c = createDebugController(backend);
    await c.start("project:1", { kind: "named", name: "x" }, [], "x");
    await c.control("next");
    expect(calls).not.toContain("control:next");
    await c.control("pause");
    emit({ event: "stopped", thread_id: 1, reason: "pause", text: null });
    await c.control("next");
    expect(calls).toEqual(["start", "control:pause", "control:next"]);
  });

  it("evaluating in the console answers in the output", async () => {
    const { backend, emit } = fakeBackend();
    const c = createDebugController(backend);
    await c.start("project:1", { kind: "named", name: "x" }, [], "x");
    emit({ event: "stopped", thread_id: 1, reason: "breakpoint", text: null });
    await tick();
    await tick();
    await c.evaluate("a + b");
    expect(get(c.state).output.map((l) => l.text)).toEqual(["> a + b\n", "5\n"]);
    expect(backend.evaluate).toHaveBeenCalledWith("a + b", 10);
  });

  it("a failed start ends the session with the reason", async () => {
    const { backend } = fakeBackend();
    backend.start = vi.fn(async () => {
      throw { kind: "NoAdapter", message: "install debugpy" };
    });
    const onError = vi.fn();
    const c = createDebugController(backend, { onError });
    await c.start("project:1", { kind: "named", name: "x" }, [], "x");
    expect(get(c.state)).toMatchObject({ phase: "ended", error: "install debugpy" });
    expect(onError).toHaveBeenCalledWith("install debugpy");
  });
});

describe("reduce", () => {
  it("joins output chunks into lines and keeps the categories apart", () => {
    let s = reduce(IDLE, { event: "output", category: "stdout", text: "hel" });
    s = reduce(s, { event: "output", category: "stdout", text: "lo\nwor" });
    s = reduce(s, { event: "output", category: "stderr", text: "oops\n" });
    s = reduce(s, { event: "output", category: "stdout", text: "ld\n" });
    expect(s.output).toEqual([
      { category: "stdout", text: "hello\n" },
      { category: "stdout", text: "wor" },
      { category: "stderr", text: "oops\n" },
      { category: "stdout", text: "ld\n" },
    ]);
  });

  it("keeps the output bounded", () => {
    let s = IDLE;
    for (let i = 0; i < MAX_OUTPUT_LINES + 50; i++) s = reduce(s, { event: "output", category: "stdout", text: `${i}\n` });
    expect(s.output).toHaveLength(MAX_OUTPUT_LINES);
    expect(s.output[s.output.length - 1].text).toBe(`${MAX_OUTPUT_LINES + 49}\n`);
  });

  it("the end: terminated, and an adapter that closes with something to say", () => {
    const live = { ...IDLE, phase: "running" as const };
    expect(reduce(live, { event: "terminated" }).phase).toBe("ended");
    expect(reduce(live, { event: "closed", stderr: "No module named debugpy\n" }).error).toBe("No module named debugpy");
    const ended = reduce(live, { event: "terminated" });
    expect(reduce(ended, { event: "closed", stderr: "late noise" }).error).toBeNull();
  });
});
