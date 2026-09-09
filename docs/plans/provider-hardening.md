# Plan: model-provider hardening

Status: **in progress**. Checkpoints 0–6 landed (0–3 on 2026-09-08, 4–6 on 2026-09-09);
CP7–8 planned. Follow the owner's usual stepwise workflow — confirm each checkpoint before
starting the next.

This is the safety follow-up to `settings-provider-overhaul.md` (that plan is "complete" as a
feature, but shipping it uncovered real holes). Keep both files: the overhaul explains *how
providers work*, this one explains *why the guardrails exist*.

## The incident (2026-09-08)

The owner switched the active provider to OpenRouter through the new Settings dialog. Over the
day this ran up **~$9.71 on OpenRouter** (credits), the bulk of it **Claude Sonnet 5 via
"Claude Platform on AWS"** and DeepSeek V4 Flash, ~10.2M tokens. Nobody noticed until the
OpenRouter dashboard was checked by hand. Three independent faults combined:

1. **`valid_model_name` rejected `/`.** Every OpenRouter model id is `vendor/model`
   (`z-ai/glm-5.3-flash`, `deepseek/deepseek-v4-flash-0731`). `valid_model_name` allowed
   `- _ . : [ ]` but not `/`, so *every* OpenRouter model failed validation.
2. **`spawn_and_collect` silently dropped an invalid `--model`.** The code was
   `if let Some(model) = request.model.as_deref().filter(|m| valid_model_name(m)) { … }` — a
   rejected model just meant no `--model` flag. The Claude Code CLI then fell back to its own
   built-in default (`claude-sonnet-5`). With `ANTHROPIC_BASE_URL` pointing at OpenRouter,
   that default was billed at full Anthropic-Sonnet rates through the proxy. A later
   one-character config typo (`z-ai/glm-5.3-flash[1m)` — `)` for `]`) kept the same silent
   path alive even after fault 1 was fixed.
3. **The connector modules make a real agent turn per refresh.** Mail / Calendar / Reminders
   digests are skills (`*-digest`), and every dashboard refresh runs one as a `claude -p`
   agent loop — 6 KB system prompt + full MCP tool dumps (whole inbox / calendar / reminder
   lists) + a multi-turn loop, 30–300 s each, resending the growing context every iteration.
   35 such runs on the incident day. With a paid provider active and faults 1–2 in play, each
   of those turns went to Sonnet-via-OpenRouter. Nothing caps or surfaces this.

Contributing UI fault, same day: the Settings **provider form bound `<input>`s to a `$derived`
value** (`activeSettings.base_url` / `.api_key` / …). In Svelte 5 a `bind:value` write to a
property of a `$derived` is discarded on the next recompute, so editing one provider's fields
progressively **wiped `[agents.providers.open_router]` in `config.toml`** — `api_key` first,
then everything — which is why "switching back to OpenRouter" then silently did nothing.

## Already done (checkpoints 0–1, landed 2026-09-08)

- **`valid_model_name` allows `/`** (`agents/claude_code.rs`) — `vendor/model` ids validate.
  Leading byte still forced alphanumeric, so `/etc/passwd`-shaped values stay rejected. Tests
  updated.
- **`claude_env()` emits `ANTHROPIC_API_KEY=""`** for a non-Anthropic provider that has a
  `base_url` (`skills/runner.rs`) — neutralises an inherited/subscription key that the CLI
  would otherwise prefer as `x-api-key` over the bearer token. Tests updated.
- **`spawn_and_collect` no longer silently drops a bad model.** New `resolve_model_arg(model,
  redirected)` (`agents/claude_code.rs`), called *before* the concurrency permit and timer:
  - valid model → passed through (trimmed);
  - present but malformed → `AxiomataError::InvalidAgentModel`, **no spawn**;
  - absent **and** `ANTHROPIC_BASE_URL` set for the run → `InvalidAgentModel`, **no spawn**
    (the CLI's built-in default must never be billed through a paid proxy);
  - absent and not redirected (Anthropic via the CLI's own login) → fine, CLI default.
  New `AxiomataError::InvalidAgentModel { reason }`. Unit-tested, including the exact `[1m)`
  typo from the incident.
- **Settings provider form binds concrete `$state` paths**
  (`config.agents.providers[pid].<field>`), not the `activeSettings` `$derived` — stops the
  config-eating data loss.

Current safe state: `config.toml` has `active_provider = "anthropic"` and an empty
`[agents.providers.open_router]`; nothing can route to a paid provider until the owner
re-enters those fields.

## Checkpoint 2 — validate provider config at the save boundary — **done 2026-09-08**

Landed as `Config::validate_for_save(&self) -> Result<(), String>` (`config.rs`), called by
`apply_config_update` (`commands.rs`) *before* the write lock is taken. The CLI still has no
config-write path, so there is nothing else to wire yet; `init()`'s first-run `config.save()`
only ever writes a freshly-loaded default (always Anthropic-active, always valid) and is left
ungated. Rules implemented exactly as listed below, plus a dependency-free `validate_base_url`
helper (no `url` crate pulled in — it only has to catch blank / scheme-less / remote-`http`
mistakes). `Settings.svelte` keeps a `providerLooksSane()` fast-path affordance (empty /
whitespace / unbalanced-bracket model id, non-https base URL) but the backend is the
authority. Tests: one per rule in `config.rs` (`validate_for_save_*`), a `validate_base_url`
matrix, a save→reload round-trip for a valid OpenRouter config, and the exact `[1m)` incident
typo. The pre-existing `apply_config_update_applies_provider_and_env_settings_live_too` test
now sets concrete Ollama models (non-Anthropic ⇒ models required).

Frontend-only validation is not enough — `saveVault()` also persists the whole config, the
CLI writes config too, and a future caller might. Add a domain method on the core type:

- `Config::validate_for_save(&self) -> Result<(), String>` in `config.rs`, called by
  `apply_config_update` (`commands.rs`) **and** any CLI path that writes config. Rules:
  - `active_provider` must have an entry in `providers`;
  - the active provider's `chat_model` and `skill_model` must each be non-empty and pass
    `valid_model_name` **when the provider is non-Anthropic** (Anthropic may leave them blank
    to use the CLI default — that's free);
  - a non-Anthropic active provider must have a non-empty `base_url` that parses as a URL and
    is `https`, except `http://localhost` / `127.0.0.1` / `[::1]` (Ollama);
  - if the active provider's `auth_env_var()` is `ANTHROPIC_AUTH_TOKEN` and it is not the
    Ollama placeholder case, `api_key` must be non-empty;
  - `workspace_root` non-empty (already checked — fold in here).
- Keep the frontend check in `Settings.svelte` as a fast-path affordance (bracket balance,
  obvious typos) but the backend is the authority.
- Tests: one per rule, plus a round-trip that a valid three-provider config still saves.

## Checkpoint 3 — `.env_clear()` + allowlist on the `claude` spawn — **done 2026-09-08**

Landed in `agents/claude_code.rs`: `spawn_and_collect` now calls `command.env_clear()` and
repopulates from `child_env(request_env)` — an allowlisted subset of this process's
environment (`INHERITED_ENV_ALLOWLIST` = `PATH HOME USER SHELL TERM LANG TMPDIR`;
`INHERITED_ENV_PREFIXES` = `LC_ XDG_ SSL_CERT_ __CF`), then `request.env` layered on top so a
provider's `ANTHROPIC_BASE_URL` / token still wins and a caller can deliberately override an
inherited key. An ambient `ANTHROPIC_*` / `CLAUDE_CODE_*` / `AWS_*` export from the launching
shell no longer reaches the child at all. The env assembly is the pure, tested
`child_env_from(ambient, request_env)`; `child_env()` is the thin `std::env::vars()` wrapper.
Tests: ambient `ANTHROPIC_BASE_URL` dropped unless `request.env` re-sets it (and then only
once), allowlist/prefix kept vs. everything else dropped, `request.env` overrides an
inherited key.

Not covered by the allowlist by design: `HTTP(S)_PROXY` / `NO_PROXY`. If a proxy is ever
needed, route it through `agents.claude_env` (→ `request.env`), which is applied after the
clear. `resolve_model_arg`'s "redirected ⇒ must have a model" guard (checkpoint 1) stays
load-bearing — `env_clear()` does not hide the on-disk `~/.claude/.credentials.json` login.

**Still to verify manually:** digest skills + a chat turn under the cleared env in
`cargo tauri dev` (PATH + HOME are kept, so the on-disk subscription login and `node`
resolution should be unaffected — but confirm before committing 2–5).

## Checkpoint 4 — record cost and tokens per run — **done 2026-09-09**

- Skill/routine runs now go through `claude -p --output-format json`
  (`claude_code::run`). `parse_run_output` unwraps the envelope: `stdout` becomes the inner
  `result` (byte-identical to the old plain-text mode), and `total_cost_usd` (dropped when
  `0.0` — the subscription path), `usage.{input,output}_tokens`, and `num_turns` are lifted
  onto `AgentRunResult`. `is_error` in the envelope now forces a non-zero exit → the run is
  recorded `Failed` (plain-text mode couldn't detect an error *reply*). Non-JSON stdout
  passes straight through with no cost data.
- Migration `0005_runs_cost.sql`: `cost_usd REAL`, `input_tokens`, `output_tokens`,
  `num_turns`, `provider TEXT` on `runs`, all nullable, + `idx_runs_provider_started_at`.
  `RunRecord`/`RunSummary` and `runlog` insert/select updated; `provider` is
  `active_provider.as_str()` for a Claude Code run, `None` for Ollama.
- **Chat turns → own table** (`0006_chat_turns.sql`): `session_id`, `mode`, `provider`,
  `model`, `is_error`, `cost_usd`, tokens, `num_turns`, `duration_ms`, `created_at`. Written
  by `agents::chat_and_record` (guard → `chat` → log), which every real caller now uses
  (`assistant_send`, `create_note`, CLI `assistant` + import sort). A logging failure is
  `warn!`ed, not propagated.
- Rollup + surfacing in `crate::spend`: `spent_usd_since` UNIONs both tables;
  `active_provider_summary` → today / this-month / cap. CLI `list-runs` prints per-run
  `$cost, provider` and a trailing `spend (open_router): $X today / $2.00 cap · $Y month`
  line; dashboard `get_spend_summary` command feeds a line + a cap editor in the Settings
  provider section.
- **Verified live** (owner's `cargo tauri dev` auto-rebuilt and ran real OpenRouter digests):
  9 rows with real `cost_usd` / tokens / `num_turns` / `provider`, connector modules still
  parse the unwrapped `result` fine.

## Checkpoint 5 — spend / rate guardrail for paid providers — **done 2026-09-09**

- Config: `agents.daily_usd_cap: Option<f64>`, `#[serde(default)]` → **$2.00**; `None`
  disables it. Applies to *any* non-Anthropic active provider combined (owner's call — no
  call-count cap, dollars only). `validate_for_save` rejects a non-positive / non-finite cap.
- `spend::guard_redirected_turn(db, config)`: no-op unless the active provider
  `is_redirected()`; else sums today's (local-day) `cost_usd` for that provider across `runs`
  + `chat_turns`; at/over the cap ⇒ `AxiomataError::SpendCapReached { provider, cap_usd,
  spent_usd }`, **no spawn**. Called in `execute_and_record_skill`, the routine scheduler's
  `fire_one` (records a failed firing), and `chat_and_record`.
- UI: the Settings spend line shows `$used / $cap today`; the cap field is a one-field raise.
- **Fired for real on first run** after the migration: today's OpenRouter spend was already
  $2.52 (the tauri-dev auto-refresh loop), so the guard now blocks every paid turn until the
  owner raises the cap or the day rolls over — exactly the intended behaviour.

## Checkpoint 6 — connector modules vs. a paid provider — **done 2026-09-09**

Done: a per-skill **`timeout_secs:`** frontmatter field (`registry` →
`runner::agent_request`), with the three `*-digest` skills set to `600`, and the
**`mail-digest` SOP slimmed** — 2-day window, one `search_emails` per topic group not per
keyword, ~6 tool calls total, 1–2-sentence summaries, output capped at 12 messages. Live
`mail-digest` runs dropped from 90–230 s to <10 s. Resource + live `~/.axiomata/skills/`
copies both updated.

**Options a/b explicitly declined (owner, 2026-09-09):** digests should keep running on
whatever provider is configured — no model pinning, no auto-refresh suppression. The spend
cap (CP5) is the guardrail; the SOP slimming is the cost reduction. Closed.

## Future — per-role provider, not just per-role model

Owner idea (2026-09-09), not scheduled: today `active_provider` is one global switch and
only the *model* differs between chat (`chat_model`) and skills (`skill_model`). Want the
**provider** itself selectable per role too — e.g. **Ollama for digests / skills, OpenRouter
for chat**. Shape TBD: probably `agents.chat_provider` / `agents.skill_provider` (each an
optional `ProviderId` overriding `active_provider` for that role), threaded through
`default_chat_model` / `default_skill_model` and `claude_env` so each role gets its own
`ANTHROPIC_BASE_URL` + credential. Touches the Settings provider UI (two active-provider
pickers), `validate_for_save`, and the CP4/CP5 rollup (`provider` column already per-run, so
that part already works). Revisit as its own plan.

## Checkpoint 7 — stop shipping provider secrets to the webview

(Carried over from the `settings-provider-overhaul` security audit, HIGH.) `get_config`
returns every `providers[*].api_key` and every `agents.claude_env` value to the renderer —
reachable via IPC where nothing was before.

- `get_config` returns `api_key` redacted (a `has_key: bool`, or a masked `"…abcd"`); never
  the raw secret.
- `save_config` treats a redacted/sentinel `api_key` as "keep the stored one"; a real value
  as a change; empty as an explicit clear. Or a dedicated write-only `set_provider_key`
  command.
- Don't send `agents.claude_env` values to the renderer at all.

## Checkpoint 8 — remaining review findings (fold in where cheap)

From the `settings-provider-overhaul` architecture + security reviews, not yet applied:

- `Config::save()` — create the file already `0o600` (`OpenOptions … .mode(0o600)`), don't
  create-then-`chmod`; `warn!` on a perms failure instead of `let _ =`. Temp-file + rename
  for atomicity.
- Workspace-root "write to disk, keep old in memory" is clobbered by a second same-session
  save — make the pending root durable (`apply_config_update` re-applies it before every
  `save()`, `get_config` surfaces it).
- `migrate_legacy_model_if_needed` guard `if !providers.is_empty()` never backfills a
  *newly added* `ProviderId` — change to "seed any `ProviderId::ALL` member missing from the
  map".
- Extract `seeded_providers()` (dup'd between `Default` and migration).
- Extract `provider_env(config)` from the `claude_env` `ClaudeCode` arm (~30 lines, two
  jobs) — also makes checkpoint 3 land in one place.
- Lock-poison: scheduler + `read_config` use `.expect()`, `db` uses `map_err`. Pick graceful
  recovery (`unwrap_or_else(|e| e.into_inner())`) for config and apply consistently.
- `activeMeta` falls back to `PROVIDERS[0]` (Anthropic copy) for an unknown active provider
  while the form binds the real entry — render an explicit "unknown provider" state instead.
- Re-seed a provider entry that exists but is entirely blank from `ProviderSettings::default_for`
  on load, so the base-URL default reappears after the data-loss incident.
- `agents.claude_model` (migration-only) still round-trips through the frontend contract —
  `#[serde(skip_serializing)]` it once migration has folded it, or mark the TS field
  `@deprecated`.
- Per-section Save buttons both persist the whole config — acceptable for a single-user app
  (owner, 2026-09-08), but run `validate_for_save` on every path so the Vault button can't
  commit a half-typed provider.

## Suggested checkpoint order

0–6 done (2–5 were the safety core). Remaining: **7** (secret redaction) → **8** (cleanup
sweep), then the separate "per-role provider" idea if the owner wants it.

## Commit note

The `settings-provider-overhaul` work (its Phases 1–5) plus checkpoints 0–1 here are all
uncommitted in the working tree, alongside the unrelated fileview (commit B) and
sub-agent-cadence (commit C) changes. Decide the commit split once at least checkpoints 2–5
land — a paid-provider feature shouldn't be committed as "done" without the guardrails.
