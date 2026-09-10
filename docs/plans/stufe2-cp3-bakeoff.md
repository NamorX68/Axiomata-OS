# Stufe 2 CP3 — bake-off notes

Status: **mechanism shipped; first bake-off round done (2026-09-10).** The
provider-switch (`local_backend` / `effective_backend`), the `prepend_files`
bridge, the loop tracing, and `axiomata-cli get-run` are implemented with unit
tests. This file is the run-by-run record the CP3 spec asks for
(`docs/plans/stufe2-lean-ollama-agent.md` → "Success criteria" / "Repo
artefacts vs report").

## Setup (per bake-off session)

Scratch `AXIOMATA_HOME`; `mcp import` seeded `[mcp_servers]` (`apple-mail`,
`apple-reminders`); `workspace_root = <home>/workspace` with
`Mail/.topics.md` = `Fotografie` / `Development` / `KI/AI/LLM`; config switch:

```
[agents]
skill_provider = "ollama"

[agents.providers.ollama]
base_url = "http://localhost:11434"
skill_model = "<candidate>"
```

Digest `SKILL.md`s carry `local_backend: ollama-agent` (+ mail:
`prepend_files: ["Mail/.topics.md"]` and the "Context file" step-1 reword), so
**`skill_provider = ollama` resolves all three to `ollama-agent`** — verified
by the `backend` column in every run below (`ollama-agent`, not `claude-code`).

## Candidate 1 — `gemma4:e4b-mlx` (4B)

Local MLX 4B, the plan's primary re-test candidate. Ollama `0.33.3`,
Apple-silicon MLX runtime.

| Run | Skill | Status | Time | Steps / tool seq (from log) | stdout shape |
|---|---|---|---|---|---|
| #1 | calendar-digest | Success | 54.8 s | 9 · `calendar_calendars` → `calendar_events` ×8 | **shape-pass**, but the model emitted a second, truncated copy of the JSON after the first valid object (concatenated, not parseable as one doc) |
| #2 | calendar-digest | Success | 178.3 s | (multi-turn `calendar_events`) | **shape-fail** — unterminated string mid-object |
| #3 | reminders-digest | Success | 93.7 s | 3 · `reminders_lists` → `reminders_tasks` | **shape-pass** — `{"lists": 12, "tasks": 60}`, `priority ∈ {none}`, clean JSON |
| #4 | reminders-digest | Success | 104.1 s | 3 · `reminders_lists` → `reminders_tasks` | **shape-pass** — `{"lists": 12, "tasks": 55}`, clean JSON |
| #5 | mail-digest | Success | 554.2 s | 2 · `get_needs_response` + `search_emails` ×3 (skipped `list_inbox_emails`) | **shape-pass** — `{"emails": []}` (empty; 554 s ≈ 600 s budget) |

Verdict: the **mechanism works end to end** (backend resolves to
`ollama-agent`; MCP servers spawned per `allowed_tools`; topics block was fed
via `prepend_files`; loop stays under `timeout_secs`; `Success` runs are
persisted with real tool calls). **But gemma4:e4b-mlx is not reliable enough
to ship as the digest model**: reminders 2/2 clean, yet calendar duplicated /
truncated its JSON and mail came back empty after nearly the whole budget.

## Candidate 2 — `SparkLLM/Spark-X2.5-4B:latest`

Pulled (8.2 GB), but **cannot run on this machine's Ollama 0.33.3**:

```
error loading model: unknown model architecture: 'spark2_5'
```

The GGUF is built on a llama.cpp architecture (`spark2_5`) the installed
Ollama build does not know; Homebrew's `ollama` formula is still 0.33.3
(GitHub latest = v0.34.0). Not an Axiomata issue — the loader reported the
model error faithfully (`run #6` failed after 879 ms with the message
above). Re-test after an Ollama version that knows `spark2_5`.
**No bake-off runs to record.**

## Conclusion vs the CP3 spec

- Switch works: all three digests resolve to `ollama-agent` under
  `skill_provider = ollama`; reverting to `anthropic`/`open_router` yields
  `claude-code` again (`effective_backend` unit-tested).
- **reminders-digest passes the bar on the tested model**: Success + clean
  shape + under timeout, 2/2.
- **calendar-digest / mail-digest do not clear shape on this model**, and no
  second candidate could be tested (Spark unloadable, `granite4.2`/`lfm2.5`
  not pulled). Per the spec's fallback: all three keep `local_backend:
  ollama-agent` in frontmatter (the switch still targets them), but calendar
  and mail are **flagged cloud-only for now**, and `skill_provider` stays on a
  cloud provider until a local model clears them.
- Mail summary **quality** is untested (run returned `{"emails": []}`).

## Notes for the next round

- Re-try `Spark-X2.5-4B` once a newer Ollama (≥ the release that adds
  `spark2_5` to llama.cpp — i.e. GBLLM built past the current 0.33.3) is
  installed; or `ollama pull granite4.2:8b` / `lfm2.5:8b` and run those.
- If calendar/mail stay flaky across all candidates, the loop already isolates
  the failure mode: the model's JSON is syntactically broken (duplicated /
  truncated), so a post-processing JSON-repair step would be strictly a
  band-aid — a bigger/steadier model is the real fix.