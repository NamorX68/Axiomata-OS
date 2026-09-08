# Plan: Settings dialog overhaul — vault path + model providers

Status: **complete** (2026-09-08). All phases (0 spike, 1 schema, 2 runtime, 3 Tauri
commands + `RwLock<Config>`, 4 Settings UI — Vault + Provider sections, 5 docs) landed. See
`docs/architecture.md` §5 "Model providers" for the as-built description; the phase-outcome
notes below record the decisions taken along the way. Kept in-repo as the rationale record.

Written 2026-09-07 in a planning session; followed the owner's usual stepwise workflow —
each checkpoint confirmed before the next.

## Goal (owner's ask, verbatim intent)

1. The vault directory (`config.workspace_root`) must be changeable from the Settings dialog.
2. Introduce a **provider** concept for model selection. Execution stays on the **Claude Code
   agent** (`claude -p`) — this is not about adding a second agent backend.
3. Offer three providers in v1: **Anthropic** (as today), **OpenRouter**, **Ollama**. **LM
   Studio is deferred** (owner, 2026-09-07) — not dropped, just out of scope for the first cut;
   see the note at the end of Phase 1 for how to keep it a cheap follow-up rather than a rework.
4. Every provider offers **two models**: one for chat, one for skills/routines. Anthropic needs
   no base URL or key (stays subscription-billed via the CLI's own login, as today).

## Scoping note — two unrelated "Ollama" concepts

`AgentBackend::Ollama` (`agents/ollama.rs`) already exists today: a raw, tool-free HTTP
completion call, selectable per skill via `SKILL.md`'s `backend: ollama`. **This plan does not
touch it.** The new "Ollama provider" is a different thing entirely: a way to point the
*Claude Code CLI itself* (still `AgentBackend::ClaudeCode`, still with full tool use / MCP / the
agent loop) at a different upstream model API, the same way `ANTHROPIC_BASE_URL` already lets it
talk to Bedrock or a proxy. Keep the naming distinction visible in code comments and the UI copy
so the next reader doesn't conflate them.

## How provider routing actually works (already half-built)

`config.agents.claude_env: BTreeMap<String, String>` and `skills::runner::claude_env()`
(`crates/axiomata-core/src/skills/runner.rs:294-328`) already forward an allow-listed set of env
vars (`ANTHROPIC_*`, `CLAUDE_CODE_*`, `AWS_*`, proxy vars) to every `claude -p` child process.
This is exactly the mechanism needed: set `ANTHROPIC_BASE_URL` (+ an API key env var) to redirect
the same CLI binary at a different endpoint that speaks the Anthropic **Messages API** shape.
Today this map is raw and has no UI — this plan turns it into a structured, UI-editable
per-provider config, computed into the same env-forwarding path at spawn time.

## Phase 0 — verification spike (do this before writing any schema/UI code)

The plan above only works if each non-Anthropic provider actually exposes an
Anthropic-Messages-API-compatible endpoint (`/v1/messages`-shaped), not just an
OpenAI-compatible `/v1/chat/completions`. Confirm per provider before committing to the v1
(three-way) UI:

- **OpenRouter**: reasonably likely to just work — they have publicized Claude Code
  compatibility. Verify the exact base URL, and whether the CLI expects the key as
  `ANTHROPIC_API_KEY` or `ANTHROPIC_AUTH_TOKEN` (`claude_code.rs` / Claude Code's own docs
  should say which env var it reads).
- **Ollama**: the owner reports `ollama launch claude` exists — a subcommand that starts the
  Claude Code CLI pointed at Ollama's own default model, with an optional model argument to
  override it. That command is almost certainly a thin wrapper: it sets the same
  `ANTHROPIC_BASE_URL` (+ possibly a placeholder API key, since local Ollama needs no real auth)
  and execs `claude`, exactly the mechanism this plan already relies on. **Don't shell out to
  `ollama launch claude` itself** — our app already spawns `claude` directly with full control
  over args (`claude_code.rs`), and shelling out to another wrapper binary would fight that. The
  actual spike work is narrower than before: find out what URL (and, if any, what dummy key /
  header) that wrapper sets — `ollama launch claude --help`, Ollama's own docs/changelog, or
  inspecting the child process's env (`ps eww $(pgrep claude)` while it's running, or a verbose
  flag if one exists) — and replicate exactly that in `claude_env()`. Also confirm the model
  override path (CLI flag vs. env var) so it lines up with per-task (chat/skill) model selection.
- **LM Studio**: out of scope for v1 (see Goal §3) — no spike needed now. When it's picked back
  up, the same two questions apply: does it expose an Anthropic Messages-API-shaped endpoint
  natively, or only OpenAI-compatible chat completions (in which case it needs the same
  translating-proxy treatment as a fallback Ollama would have needed).

Outcome of this phase decides the real v1 provider list. If a v1 provider needs a proxy after
all, either punt it too or fold "launch/manage a local translator" into this plan explicitly —
don't discover it mid-implementation.

### Phase 0 outcome (2026-09-08) — both confirmed, v1 list unchanged

- **OpenRouter**: confirmed, native — not a translating proxy. OpenRouter's "Anthropic Skin"
  speaks the real `/v1/messages` shape directly; thinking blocks and native tool use survive the
  round trip. `ANTHROPIC_BASE_URL = https://openrouter.ai/api` (note: **not** `/api/v1` — that's
  the separate OpenAI-compatible endpoint, a documented common mistake). Auth goes in
  `ANTHROPIC_AUTH_TOKEN` (bearer) with `ANTHROPIC_API_KEY` explicitly set to `""` — the CLI reads
  `ANTHROPIC_API_KEY` as an `x-api-key` header for direct-Anthropic auth, which conflicts with
  the bearer token if both are set non-empty. So `ProviderSettings.api_key` for OpenRouter must
  be written into `ANTHROPIC_AUTH_TOKEN`, not `ANTHROPIC_API_KEY` — Phase 1's field naming should
  make this per-provider env mapping explicit rather than assuming one key field ⇒ one env var
  uniformly. Source: [OpenRouter's Claude Code integration guide](https://openrouter.ai/docs/cookbook/coding-agents/claude-code-integration).
- **Ollama**: confirmed, and simpler than the plan assumed. Ollama **v0.14+** natively serves the
  Anthropic Messages API shape at `/v1/messages` on its normal port — `ollama launch claude` is
  just a convenience wrapper around three env vars, not something we need to reverse-engineer via
  `ps eww`/a wrapper spike as §Phase 0 originally proposed:
  `ANTHROPIC_BASE_URL=http://localhost:11434`, `ANTHROPIC_AUTH_TOKEN=ollama` (literal placeholder
  string, not a real credential), `ANTHROPIC_API_KEY=""`. No real key ever required for local
  Ollama. Source: [Ollama's own Claude Code integration doc](https://docs.ollama.com/integrations/claude-code).
- **Model selection, both providers**: works through the **existing `--model` CLI flag** that
  `claude_code.rs` already passes today — it's Claude Code's own flag, sent as the `model` field
  in the Messages API request body, which both providers route on their end. No provider-specific
  model env var needed anywhere. (OpenRouter also *supports* an alternate
  `ANTHROPIC_DEFAULT_SONNET_MODEL`-style env-var path, but the CLI flag already in use covers it —
  no reason to add a second mechanism.) This simplifies Phase 2: `default_chat_model()` /
  `default_skill_model()` just feed the existing `--model` plumbing, same as today's single
  `default_claude_model()` does — no new per-provider model env-var branch needed.
- **Env allowlist already sufficient**: `CLAUDE_ENV_ALLOWED_PREFIXES` in
  `skills/runner.rs:299-309` already allow-lists the whole `ANTHROPIC_` prefix, which covers
  `ANTHROPIC_AUTH_TOKEN` alongside `ANTHROPIC_BASE_URL`/`ANTHROPIC_API_KEY`. No allowlist change
  needed in Phase 2.
- **LM Studio**: still out of scope, no spike done, per Goal §3 — unchanged.
- **Conclusion**: v1 provider list stays **Anthropic | OpenRouter | Ollama** as planned. Proceed
  to Phase 1 with one schema adjustment: `ProviderSettings` needs to know *which* env var its key
  goes into (`ANTHROPIC_API_KEY` vs `ANTHROPIC_AUTH_TOKEN`), since it differs across Anthropic
  (API key, direct) vs. OpenRouter/Ollama (auth token, bearer) — see Phase 1 note below.

## Phase 1 — config schema (backend only, no UI yet)

In `crates/axiomata-core/src/config.rs`:

- Add a `ProviderId` enum: `Anthropic | OpenRouter | Ollama` for v1 (serde
  `rename_all = "snake_case"` or similar), with `Anthropic` as the default. Design the enum and
  the `BTreeMap<ProviderId, ProviderSettings>` below so a fourth variant (`LmStudio`, once its
  Phase-0 spike is done) is a one-line addition later, not a schema rework — don't hardcode
  "three providers" anywhere outside this enum (loop over `ProviderId::ALL` or a similar
  exhaustive list for UI rendering and migration code, rather than enumerating three call sites
  by hand).
- Add a `ProviderSettings` struct per provider: `base_url: Option<String>`,
  `api_key: Option<String>` (empty/`None` for Anthropic and optionally for local providers that
  need none), `chat_model: String`, `skill_model: String`.
  - **Phase-0 finding**: Anthropic wants its key in `ANTHROPIC_API_KEY` (direct, `x-api-key`
    header), but OpenRouter/Ollama want theirs in `ANTHROPIC_AUTH_TOKEN` (bearer) *with*
    `ANTHROPIC_API_KEY` explicitly emptied — the CLI otherwise prefers the direct key and the two
    conflict. Don't hardcode this as one `if provider == Anthropic` branch in Phase 2; give
    `ProviderId` (or `ProviderSettings::default_for`) a fixed `auth_env_var()` — `ANTHROPIC_API_KEY`
    for Anthropic, `ANTHROPIC_AUTH_TOKEN` for the other two — so a future LM Studio/self-hosted
    entry just declares which kind it is rather than adding another branch.
- Store all four under `agents.providers: BTreeMap<ProviderId, ProviderSettings>` plus
  `agents.active_provider: ProviderId` — keep each provider's own remembered fields even while
  inactive, so switching back and forth in the UI doesn't lose what was typed.
- Prefill sensible defaults per provider (base URL placeholders, empty models) in
  `ProviderSettings::default_for(id)` or similar, mirroring `default_claude_model()`'s pattern.
- **Migration**: existing installs have a flat `agents.claude_model` / no `providers` block.
  `#[serde(default)]` covers the missing-field case structurally, but write an explicit
  migration step in `Config::load()` (or a `#[serde(deserialize_with)]`/post-load fixup) that,
  when `providers` is absent/empty, seeds the Anthropic entry's `chat_model` *and* `skill_model`
  from the old flat `claude_model` value — so upgrading doesn't silently reset anyone's model
  choice. Add a test mirroring the existing `load_defaults_when_missing_then_round_trips_through_save`
  test, but starting from an old-shaped TOML string.
- Keep `agents.claude_env` as-is, as a power-user escape hatch (e.g. Bedrock, which doesn't fit
  the four-provider model at all) layered *on top of* the provider-derived env — decide and
  document precedence (recommendation: provider-derived vars first, then `claude_env` entries
  can still add to or override them).
- The `config.rs:38` comment about Haiku being a *temporary* global default becomes obsolete
  once chat/skill models are independently configurable per provider — remove it here and let
  the owner pick real defaults per model field instead (this directly resolves the open item
  from the earlier chat-vs-skill-model conversation).

## Phase 2 — runtime plumbing (backend only, with tests)

- `crates/axiomata-core/src/agents/mod.rs`: replace `default_claude_model()` with
  `default_chat_model(config)` and `default_skill_model(config)`, both reading
  `config.agents.providers[active_provider]`. `chat()` uses the chat variant.
- `crates/axiomata-core/src/skills/runner.rs`: its fallback (when a skill has no own `model:`
  frontmatter) switches to `default_skill_model()`. Per-skill frontmatter still wins — unchanged
  precedence, just a different fallback source.
- `claude_env()` (`skills/runner.rs:313`) gains provider-derived `ANTHROPIC_BASE_URL` /
  `ANTHROPIC_API_KEY` (or whatever env var Phase 0 confirms) built from the active
  `ProviderSettings`, merged with the existing allow-listed `claude_env` map per the precedence
  decided in Phase 1. Anthropic provider ⇒ no base URL/key ⇒ empty env, byte-identical to
  today's behavior (subscription auth via the CLI's own login) — this must not regress.
- Unit tests: env derivation per provider (Anthropic → empty; OpenRouter → base_url + key
  present; Ollama → base_url present, key optional/absent), and model-resolution precedence
  (skill frontmatter > provider `skill_model`; chat always uses provider `chat_model`).
- `rust-test-engineer` fires automatically per the repo's `CLAUDE.md` on every new/changed `fn`
  here — let it run rather than skipping.

## Phase 3 — Tauri commands + the "config is never mutated at runtime" invariant

`AxiomataCore.config: Config` is currently unlocked by design (`lib.rs:33-37`: "read constantly
and effectively never mutated at runtime, so it needs no lock"). This plan breaks that
invariant — the settings dialog needs to write config at runtime. This is the one unavoidable
structural change:

- Wrap `config` in a `Mutex<Config>` (or `RwLock<Config>`, given many more reads than writes) in
  `AxiomataCore`. Grep every `state.config.*` / `core.config.*` call site first (roughly a dozen
  across `commands.rs` and the core crate) and switch each to take the lock — keep locks short
  and never held across an `.await`, matching the existing pattern already used for `db`.
- Add `get_config` — returns the full editable config to the frontend (workspace root, owner,
  provider settings incl. keys). Same trust boundary as today's plaintext `config.toml`; nothing
  new is exposed that the user couldn't already read from disk.
- Add `save_config` — validates, writes via the existing `Config::save()` (already does the
  `0o600` permission fix-up, appropriate for now storing an OpenRouter key too), and updates the
  live in-memory config under the new lock.
- **Workspace-root is a special case.** Live-swapping it has a wide blast radius (memory router,
  the particle graph, dashboard content, notes — everything that walks `config.workspace_root`
  today assumes it's fixed for the process lifetime). Recommendation for v1: **write the new
  root to `config.toml` immediately, but require an explicit app restart** to actually start
  using it, rather than attempting a live workspace swap. Surface this plainly in the UI (e.g. a
  "Restart Axiomata-OS to switch workspace" prompt/button) instead of quietly no-op'ing or, worse,
  half-applying it. Provider/model changes, by contrast, can apply live — nothing else caches
  them beyond the next `claude -p` spawn.

### Phase 3 outcome (2026-09-08) — done, plus one scope addition beyond the text above

Implemented as `Arc<RwLock<Config>>` (not `Mutex`, per the "many more reads than writes" note).
Every read site clones the `Config` out from under the lock in its own statement
(`read_config(&state.config)` in `commands.rs`, mirrored in `axiomata-cli/src/main.rs`) rather
than holding the guard — never across an `.await`. `get_config`/`save_config` landed as specified;
`save_config`'s workspace-root special case is its own pure `apply_config_update()` helper (no
`State` dependency) so it's unit-tested directly rather than only reachable through a Tauri
harness.

**Scope addition, confirmed with the owner first (not silently assumed):** the routine
scheduler (`routines::scheduler::{spawn,spawn_with_interval,serve,serve_with_interval}`) held its
own frozen `Config` snapshot from `bootstrap.rs`'s `core.config.clone()` at startup — a plain
value clone, decoupled from `AxiomataCore.config` even after this phase's `RwLock` wrap. Left
as-is, "provider/model changes apply live" would have been false for anything routine-fired until
an app restart, contradicting this section's own claim above. Fixed: those four functions now
take `Arc<RwLock<Config>>` (the *same* shared lock as `AxiomataCore.config` — `bootstrap.rs` hands
it over via `Arc::clone`, not a value clone), and `serve_with_interval`'s loop reads+clones a
fresh snapshot right before each `tick()` call (the guard is a temporary, dropped before the
`.await`). `tick()` itself and everything it calls (`fire_one`, `execute_target`, …) needed no
changes — only the four outer loop-owning functions and one scheduler test's `spawn_with_interval`
call site.

## Phase 4 — Settings UI (`apps/dashboard/src/shell/Settings.svelte`)

- **Vault section**: current path (read-only display, matches today's "About" `Workspace` row),
  a folder picker (check whether the Tauri dialog plugin is already a dependency; add it if not)
  or a plain path field, and a "Save & Restart" action reflecting the Phase 3 decision.
- **Provider section**: a 3-way picker for v1 (Anthropic / OpenRouter / Ollama), rendered by
  looping over `ProviderId::ALL` rather than hand, so adding LM Studio later is a UI no-op. Per
  selected provider: base URL field (prefilled default, editable; hidden for Anthropic), API key
  field (masked; required for OpenRouter, optional/hidden for Ollama, absent entirely for
  Anthropic with a short note that it's billed through the subscription, no key needed — ties
  back to the earlier "20€-Abo vs. Credits" conversation), and two model text fields (Chat model
  / Skills model). Persist each provider's fields independently (per the `BTreeMap` schema) so
  switching the active provider in the UI doesn't discard what was typed for the others.
- Match the dialog's existing visual language (`--ax-*` tokens only, per `CLAUDE.md`'s theming
  rule) and its existing mixed instant-apply/explicit-save pattern — theme applies instantly,
  this is form-shaped data, so an explicit "Save" per section fits better than global auto-save.
- Add/extend the corresponding TS types in `core/backend.ts` (mirroring `AppInfo`) for the new
  `get_config`/`save_config` payloads.

## Phase 5 — docs

- Update `docs/architecture.md` §5 (and wherever agent backends are described, e.g. near the
  existing `AgentBackend` walkthrough) once this lands — required by this repo's own maintenance
  note, not optional.
- Update `CLAUDE.md`'s "Model" bullet (currently describes the single global
  `config.agents.claude_model` / temporary-Haiku situation) to describe the new per-provider,
  per-task (chat vs. skills) model config instead.

## Suggested checkpoint order

1. Phase 0 spike (no code) — confirms the real v1 provider list.
2. Phase 1 — config schema + migration test.
3. Phase 2 — runtime plumbing + tests.
4. Phase 3 — `Mutex`-wrap config, Tauri commands, workspace-restart flow.
5. Phase 4 — Vault UI section.
6. Phase 4 — Provider UI section.
7. Phase 5 — docs.

Confirm each checkpoint with the owner before starting the next, per the project's usual
stepwise workflow.
