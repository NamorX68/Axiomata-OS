-- Per-run cost and token accounting (provider-hardening checkpoint 4). Skill
-- runs now go through `claude -p --output-format json`, whose envelope carries
-- `total_cost_usd`, `usage.{input,output}_tokens`, and `num_turns`; `provider`
-- records which model-routing provider (`config.agents.active_provider`) the
-- run was billed through, so the spend guardrail (checkpoint 5) can sum a
-- provider's cost for the current day.
--
-- All nullable: every row from before this migration predates the JSON
-- envelope, and a run on the local Ollama backend or the subscription-billed
-- Anthropic path legitimately has no cost to record.
ALTER TABLE runs ADD COLUMN cost_usd      REAL;
ALTER TABLE runs ADD COLUMN input_tokens  INTEGER;
ALTER TABLE runs ADD COLUMN output_tokens INTEGER;
ALTER TABLE runs ADD COLUMN num_turns     INTEGER;
ALTER TABLE runs ADD COLUMN provider      TEXT;

-- The spend rollup filters by provider and day.
CREATE INDEX IF NOT EXISTS idx_runs_provider_started_at ON runs (provider, started_at DESC);
