# HS6 — skills: build instructions

Self-contained instructions for building step HS6 of the harness (`docs/plans/harness.md`, card #37). Written
2026-10-13 for another model to build without the conversation. Build HS4 and HS5 first (`harness-hs4.md`,
`harness-hs5.md`); HS6 starts from their commits. Owner decisions (2026-10-13) are marked **[owner]**, points the
planner fixed are **[default]** — build them as written, do not redesign.

## Where and how

As in HS4 ("Where and how"): worktree `.claude/worktrees/harness`, branch `worktree-harness`, **no `axiomata-core`
dependency** (the harness must not know `~/.axiomata`), gate
`cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, one commit
`feat(harness): skills, step HS6 (#37)`, no push. Frontmatter is YAML: use the workspace's `gray_matter`
(`gray_matter.workspace = true`; see how `crates/axiomata-roster` uses it). Real examples to test against (read only,
never write): `~/.axiomata/skills/*/SKILL.md`, e.g. `mail-digest`.

## Decisions

- **K1 [owner] — two ways to use a skill.**
  1. *A skill as a run:* `axh run --skill <name> [-- "extra prompt"]` and `axiomata-cli harness run --skill <name>`.
     The SKILL.md body becomes the run's instructions (appended to the small system prompt, `prompt.rs`, like the
     caller's instructions of H6); the prompt is the extra text, or `"Run the skill."` when there is none.
  2. *Like Claude Code:* in every run the model sees each available skill's `name` and `description` in the
     description of one tool, `skill`; calling `skill {"name": "…"}` returns that SKILL.md's body (and the list of
     files next to it, see K4) as the tool result, and the model follows it.
- **K2 [owner] — the caller says where skills are.** The harness itself knows `~/.axh/skills/` (the owner's) and
  `<project>/.axh/skills/` (only in a trusted folder, as the project's allow rules). A caller adds more folders:
  `RunConfig`/the builder gets `skill_dirs: Vec<PathBuf>`, `axh` a repeatable `--skill-dir <path>`. Axiomata's CLI
  passes `~/.axiomata/skills` (it knows where that is; the harness does not). Each skill is a folder
  `<dir>/<name>/SKILL.md`. On equal names: project beats caller folders beat the owner's `~/.axh/skills` **[default]**
  — the more specific place wins, as with commands.
- **K3 [default] — frontmatter.** Read `name`, `description` (both required — a skill without them is skipped with a
  note on stderr, never fails the run), `model`, `allowed_tools` / `allowed-tools` (a list or one space- or
  comma-separated string of rules, e.g. Axiomata's `allowed_tools: mcp__apple-mail__search_emails …`), `timeout_secs`,
  `prepend_files`. Every other key (`backend`, `output`, `license`, `metadata`, …) is ignored: skill files come from
  other tools too. `name` must equal the folder name and match `[a-z0-9-]{1,64}`; otherwise skipped with a note.
- **K4 [default] — files next to a skill.** A skill folder may hold more files (scripts, references). The `skill`
  tool's result ends with a list of them as paths relative to the skill folder, and the model reads them with
  `read_file`. For that, the file tools get a second read-only root per skill folder **outside** the working folder:
  name them `skill:<name>/<rel>` in `read_file`, `list_dir`, `grep`, `glob` (go through `axiomata_files::Root::dir`
  with `LinkPolicy::Contained`, as the working folder does). Never writable. The `Read` rules judge them as
  `skill:<name>/<rel>`.
- **K5 [default] — what a skill's frontmatter does in a run.**
  - `allowed_tools` *narrows* the run like a command's (`Gate.restrict` from HS4), only for a skill **run** (K1.1);
    a skill loaded through the `skill` tool (K1.2) does not change the rules (Claude Code's behaviour for
    model-invoked skills is the same: the existing permissions decide).
  - `model` overrides the run's model in a skill run; ignored when loaded through the tool.
  - `timeout_secs` becomes the run's overall timeout in a skill run (capped by the existing maximum).
  - `prepend_files: ["rel/path.md"]` (Axiomata's bridge, `AGENTS.md` "Backend ids"): in a skill run, each file
    relative to the **working folder** is inlined before the body as `## Context file: <path>` + content; a missing
    file is skipped; an absolute path or one with `..` fails the run with a clear error. Read through
    `axiomata_files` (contained), capped at 256 KiB per file (named const).
- **K6 [owner] — Axiomata's core is not touched in HS6.** `backend: axh` for skills, runs in the `runs` table and
  cost metering are step HS6b, planned separately. HS6 only adds `--skill`/`--skill-dir` to the CLI's existing
  `harness run`.

## What to build

1. **`src/skills/mod.rs`** — `Skill { name, description, dir: PathBuf, body: String, model, allowed_tools:
   Vec<Rule>, timeout: Option<Duration>, prepend_files: Vec<String> }`; `Catalog::load(home, project, trusted,
   extra_dirs) -> (Catalog, Vec<String> /* notes */)`; `Catalog::get(name)`, `Catalog::list()`. Limits (named consts):
   200 skills, 256 KiB per SKILL.md; a broken skill is a note, not an error.
2. **`src/skills/tool.rs`** — the `skill` tool (K1.2): `spec()` lists `name: description` of every skill in its
   description (cut each description at 300 characters, the whole list at 8 000 — named consts; beyond that: "…and N
   more, call skill with list=true"), parameters `{"name": string}` and optional `{"list": bool}`; `access()` is
   `Access::Own` (loading text is not an action — what the model then does is judged as usual). Not offered when the
   catalog is empty.
3. **File tools** — the `skill:<name>/` roots of K4 in `src/tools/files.rs`; protected-name and rule checks unchanged.
4. **Skill run** — in `bin/axh.rs` and `crates/axiomata-cli/src/harness_cmd.rs`: `--skill <name>` and repeatable
   `--skill-dir`; resolve, build the instructions (prepended files + body), apply K5, run. Unknown skill → error
   listing the known names. The CLI passes `~/.axiomata/skills` (from `axiomata_core`'s config dir — the CLI may
   depend on core, the harness may not) plus any `--skill-dir`.
5. **Event** — `Event::Skill { name, how: "run" | "loaded" }`, emitted when a skill run starts and when the `skill`
   tool loads one.
6. **Docs** in the same commit — `docs/plans/harness.md`: row HS6 ✅, row **HS6b** "`backend: axh` in the core: skill
   runs through the own harness, `runs` table, spend metering — planned on its own", a section "HS6 — skills
   (decided 2026-10-13)" with K1–K6 and "HS6 — what exists"; status line in `docs/architecture.md`; `AGENTS.md`
   `HS1–HS6 built, HS6b–HS8 open`.

## Tests (inline, all must pass)

- catalog: owner, caller folder and trusted project merged with project > caller > owner; untrusted project
  skipped; missing `description` → note, skipped; `name` ≠ folder → skipped; 257 KiB file → skipped.
- frontmatter: Axiomata's `mail-digest` style (string `allowed_tools`, `prepend_files` list, `backend`, `output`,
  `timeout_secs`) parses — copy that header into a test fixture, do not read the real file in a test; Claude style
  (`allowed-tools: [Read, "Bash(git status:*)"]`, multi-line `description: >`).
- skill run: body appended to the system prompt; `prepend_files` inlined in order, missing skipped, `../x` → error;
  `allowed_tools` narrows (a `bash` call refused when only `Read` is allowed); `model` overrides.
- `skill` tool: lists name+description; loads a body; ends with the folder's other files; unknown name → tool error;
  long list cut with "…and N more"; `list=true` returns all names.
- file roots: `read_file skill:demo/ref.md` works; `write_file skill:demo/x` refused; `skill:demo/../../etc/passwd`
  refused; a link out of the skill folder refused.
- event: `Event::Skill` serializes as `{"type":"skill","name":…,"how":"run"}`.

## Out of scope (do not build)

The core side (`backend: axh`, `runs`, spend — HS6b), skills from plugins or marketplaces, running a skill's scripts
automatically, hooks inside skills (`hooks:` frontmatter), `context: fork`/subagent skills (HS8), editing or
creating skills. Each is a later step.
