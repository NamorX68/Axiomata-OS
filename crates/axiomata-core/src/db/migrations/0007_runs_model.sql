-- Records which model id a skill run used (`claude --model`), so the recorded
-- `cost_usd` can be re-metered against the owner's per-model price table
-- (`config.agents.costs`) after the fact — the 2026-09-11 spend-cap incident
-- showed the CLI's own cost estimate is wildly wrong for non-Anthropic models,
-- so runs must carry enough information to recompute their real cost later.
-- NULL for runs recorded before this migration (and for absent model ids).
ALTER TABLE runs ADD COLUMN model TEXT;