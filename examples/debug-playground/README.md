# Debug playground

Small programs in five languages for trying the Studio's debugger. Every folder is its own project:
**open the folder as a project** (project menu → open folder), open the source file, click a line number to set a
breakpoint, and press **Debug** in the Debug view (the bug icon in the rail).

Each program does the same: a `Counter`, a Fibonacci loop, a Collatz loop, and a small map/dictionary.
Comments in the code mark good places for a breakpoint.

| Folder | What to pick in the Debug view | Needs |
|---|---|---|
| `python/` | **demo** (the entry point in `pyproject.toml`), **main.py**, or **Current file** | Python; `uv` (fetches debugpy) or debugpy in a `.venv` |
| `rust/` | **cargo: debug-demo** or **cargo: tiny** | Rust (`cargo`) and `lldb-dap` |
| `c/` | open `main.c`, pick **Current file** | a C compiler (Xcode command line tools) and `lldb-dap` |
| `cpp/` | **cmake: demo** | `cmake`, a C++ compiler and `lldb-dap` |
| `swift/` | **swift: demo** | Swift (Xcode) and `lldb-dap` |

`lldb-dap` comes with Xcode 16+ command line tools, or `brew install llvm`.

### The Python folder in detail

- `main.py` — the basic walk-through; `helpers.py` is stepped into with **F4** (the debugger moves between files).
- `crash.py` — open it, pick **Current file**: an uncaught `ValueError` stops the debugger on the line that raised it.
- `interactive.py` — reads with `input()`: pick **Current file** and tick **Run in a terminal**, type in the “Debug” pane.
- `pyproject.toml` — declares the `demo` entry point, which the Debug view lists on its own (no file to write).

## Things to try

1. Breakpoint in `add` → Debug → it stops; look at **Variables** and the **Call stack**.
2. **F2** step over, **F3** step out, **F4** step into, **F1** continue (F5 and F10 work too; F11 is "show desktop" on macOS).
3. Breakpoint inside the Fibonacci loop → **F1** several times, watch `a`, `b` change.
4. In the console (while stopped) evaluate an expression, e.g. `counter` or `a + b`.
5. Insert a few lines above a breakpoint: it should move with its code.
6. Tick **Run in a terminal** and start again: the program runs in the “Debug” pane instead.
7. Break the build on purpose (a typo in the Rust, C or C++ file): the build error appears in the panel.

## Opening the whole folder

You can also open `debug-playground` itself as the project: the Debug view then lists the Rust, CMake and Swift
projects beneath it (`cargo: tiny (rust)`, `cmake: demo (cpp)`, `swift: demo (swift)`), and **Current file** works
for the `.rs`, `.swift`, `.c` and `.cpp` files in them.
