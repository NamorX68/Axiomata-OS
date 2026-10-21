# Glut robust — no endless refusals, compaction before the limit

Status: **approved** by the owner 2026-10-21 ("Ja bau so", R4 at 5/15 as proposed). Cards #112 and the "Kürzen dreifach" item of #111.

## Why

Two things cost money and nerves today, and Opencode 2 and Pi both handle the second one:

- **#112** — Glut repeats a refused call until the step limit. Measured (LocalBrain CP4, 2026-10-20): a news-web run
  denied `web_fetch` ("not permitted: no one to ask") fetched the same URL 30 times, then went around it with `curl`,
  `python3 urllib`, `task` and code mode, until step 40. Real runs show 10–30 refused `bash` calls each.
- **#111, "Kürzen dreifach"** — of the three ways of cutting, the history cut (`history.rs`) and `shrink_results`
  are already gone (Glut lean N1). What is left: **Glut has no compaction of its own near the context limit.** A chat
  that outgrows the window fails its turn ("start a new chat"), a long skill run fails at the provider. `/compact`
  exists, but only when the owner asks for it.

The other items of #111 stay on the card (Opencode out once Glut has a TUI, connector modules without a model turn).

## What exists to build on

- Every decision of the gate lands in its log (`Judgement { tool, target, allowed, reason }`), which `perform`
  (`run.rs`) takes after each call — for direct calls and a script's calls alike, the network proxy's host decisions
  included. A hook's block is answered in `perform` as well. One place sees every refusal.
- `compact.rs` summarises a conversation with the run's model (fenced tool results, untrusted).
- Each model call reports its prompt tokens (`Usage.input_tokens`); the catalog knows OpenRouter's
  `context_length` per model.

## Decisions to confirm

### #112 — refusals

- **R1 — what counts as the same refusal:** the gate's tool and target (`bash` + the command, `web_fetch` + the URL,
  `Network` + the host), or a hook's block of the same tool and arguments. An unanswered ask (headless) and the
  owner's Deny count like a rule's deny.
- **R2 — the second time:** the call is refused as before, and its answer gains a clear note: *already refused
  (n times): <reason>. Do not try it again or around it (curl, python, a script, a subagent); go on without it, or
  finish and say what is missing.*
- **R3 — the third time:** the run ends with a new `FinishReason::Refused`, the message naming the call and the
  reason — not at the step limit. In Axiomata a skill run is recorded `Failed` with that message; the chat shows it.
- **R4 — around it:** workarounds cannot be recognised safely, so only counted: from the **5th** refusal of a run
  each refused answer carries the note *n calls refused in this run*; at the **15th** the run ends as in R3.
  *(Question to the owner: are 5/15 right? Real runs had 10–30 refused `bash` calls; with 15 some of today's runs
  would end earlier — the point.)*
- **R5 — subagents:** a subagent counts its own and ends the same way; its refusals also count toward the parent's
  total (R4), so a `task` per attempt is no way around.
- **R6 — Glut's own:** the limits live in `~/.axh/settings.json` (`"refusals": { "repeat": 3, "total": 15 }`) and
  are passed as `RunConfig` fields; Axiomata adds no setting of its own (feedback "Glut standalone").

### #111 — compaction before the limit

- **K1 — when:** before each model call, when the last call's prompt tokens plus the messages added since (estimated
  at 4 characters a token) plus the output reserve (`max_output_tokens`, else 8k) reach the window minus a reserve
  of **15 % (at least 16k)** — the measure Opencode and Pi chose too. Also at the start of a chat turn, the same way.
- **K2 — the window, Glut's sources first:** `--context-window` / the run config, then `"context_window"` in
  `~/.axh/settings.json` (per model, `{ "model-id": 131072 }`), then what Axiomata passes from its catalog, then
  Glut asks the provider's model list once per run (`<base>/models`, OpenRouter answers `context_length`). Unknown
  (Ollama, a custom endpoint): **K4 only**.
- **K3 — what is kept:** the system prompt, the latest user message (the task), and the **newest ~20k tokens** of the
  conversation, cut only between whole steps (a tool call is never parted from its result); everything before them
  goes into the summary (`compact.rs`, the run's own model). Nothing is lost: the whole conversation stays in the
  journal and, in the chat, in `<id>.pre-compact.jsonl` as with `/compact`; tool results stay in the run's result
  store (`read_full_result`).
- **K4 — when the provider says it is too long anyway:** an HTTP 400 or a streamed error that names the context
  ("context length", "maximum context", "too many tokens", "prompt is too long") compacts once and asks again; a
  second one fails the run as today.
- **K5 — seen and metered:** an event `Compacted { before_tokens, after_tokens, replaced }`; the chat shows a quiet
  note ("conversation compacted — 118k → 21k tokens"); the summary call's usage is metered like any call of that
  role. The journal gets a `Line::Compacted` with the new conversation, so `axh resume` goes on from it.
- **K6 — off switch:** `"compact": { "auto": false }` in `~/.axh/settings.json`, `axh --no-auto-compact`. On by
  default.
- **K7 — the chat's "start a new chat":** only when compaction itself failed.

## Not in this round

- Prompt-cache reuse for the summary call (Opencode sends the original messages; Glut sends a transcript because some
  providers refuse tool calls without tools on offer). Measured later, if the summaries prove costly.
- Compacting a subagent's conversation (they are short; depth 1).
- The rest of #111.

## Checkpoints

- **CP1 — #112:** refusal ledger in `run.rs` (R1–R5), settings (R6), `FinishReason::Refused` through `axh`, core and
  the app; tests with a scripted model that repeats a denied call, goes around it, and uses subagents.
- **CP2 — #111 window and trigger:** K1, K2 (incl. Axiomata passing the catalog value), K6; tests for the estimate and
  the sources' order.
- **CP3 — #111 compaction in the run:** K3, K4, K5 (journal + resume), K7 in the chat; tests with a fake small window
  and a provider that answers "context length exceeded" once.
- **CP4 — live:** one long chat past a small configured window and one news-web run, both on DeepSeek V4.1 Flash;
  measure steps, cost and what the summary kept.
