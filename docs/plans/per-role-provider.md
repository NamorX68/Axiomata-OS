# Plan: per-role model provider (chat vs. skills)

Status: **COMPLETE** (2026-09-09). Follow-up to `provider-hardening.md` §"Future — per-role
provider".

## Context

The model-provider feature had one global switch, `agents.active_provider`; only the *model*
differed by role (`chat_model` vs `skill_model` on that one provider). The owner wanted the
**provider** itself per role — Anthropic (or OpenRouter) for the interactive dashboard chat,
local **Ollama for the connector digests / skills** — so a slow local model never blocks
chat and a paid provider is only touched where wanted. (`ollama` had been running every
`claude -p` turn, ~38K-token prompts, minutes per digest; see the Ollama investigation
thread.)

## What shipped

- **Config.** `agents.active_provider` **replaced** by `agents.chat_provider` +
  `agents.skill_provider` (`ProviderId` each). New `enum ProviderRole { Chat, Skill }` and
  `AgentDefaults::provider_for(role)`. `legacy_active_provider: Option<ProviderId>`
  (`#[serde(rename = "active_provider", skip_serializing)]`) parses the old key;
  `migrate_legacy_provider_if_needed()` (run in `Config::load()` after the model migration)
  folds it into both role fields unconditionally, and `save()` drops it.
- **Validation.** `Config::validate_for_save` loops the two roles: each role's provider must
  exist in the map and — unless Anthropic — carry a valid non-blank model for *that role's*
  field (`chat_model` / `skill_model`), a well-formed base URL, and a key (unless local
  Ollama). A provider used by neither role is not validated.
- **Wiring.** `agents::default_chat_model` / `default_skill_model` → shared
  `provider_model(config, provider_id, pick)`. `skills::runner::claude_env(config, backend,
  role)` and `provider_env(config, role)` resolve `provider_for(role)`; skill/routine runs
  pass `ProviderRole::Skill`, `agents::chat` passes `ProviderRole::Chat`. Run `provider`
  label + `spend::guard_redirected_turn(db, config, role)` are role-aware.
  `spend::role_spend_summaries()` returns one `SpendSummary` per distinct provider across the
  two roles (`role` field: `"chat"` / `"skill"` / `"chat & skill"`).
- **IPC + CLI.** `commands.rs` `AgentDefaultsView` / `AgentDefaultsUpdate` carry
  `chat_provider` + `skill_provider`; `get_spend_summary` returns `Vec<SpendSummary>`. CLI
  `list-runs` prints one spend line per role provider. `core/backend.ts` + `core/devmock.ts`
  mirror the shapes.
- **Settings UI.** Two `<select>`s ("Provider für Chat" / "Provider für Skills & Routinen")
  bound to the two config fields. The provider list is now an **edit** selector
  (`editingProvider`, defaults to `chat_provider`, preserved across a save-reload); `[Chat]`
  / `[Skills]` badges mark the role providers. `providerLooksSane()` mirrors the Rust
  per-role loop. Existing per-provider form (base URL, masked key, two model fields)
  unchanged.

## Tests

`axiomata-core`: `load_migrates_legacy_active_provider_into_both_role_fields`,
`validate_for_save_allows_split_anthropic_chat_and_ollama_skills`,
`validate_for_save_rejects_a_blank_skill_model_on_the_skill_role_provider`,
`default_models_follow_their_own_role_provider`,
`claude_env_is_role_aware_across_a_split_provider_config`, plus every prior
`active_provider` test ported to the two role fields. `cargo test --workspace` green,
`clippy -D warnings` clean, `npm run check` + `vitest` green.

## Live config

`~/.axiomata/config.toml` migrated in place: `active_provider = "ollama"` →
`chat_provider = "ollama"` / `skill_provider = "ollama"` (behaviour-preserving; the owner
picks a chat provider in the UI).
