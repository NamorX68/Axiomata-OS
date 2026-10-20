# LocalBrain — Glut's context kept small by a local model (owner's point 4)

Status: **closed, not taken over** (owner, 2026-10-20). Grilled (L1–L13), CP0–CP4 built and measured on the branch
`localbrain` (kept locally as an archive, `b27f104`; never merged): the end-to-end measurement found no clear saving
at the same quality, and the owner does not want code that only sits behind a switch that stays off. See
"Outcome" below. Builds on the decisions of 2026-10-14 in
`docs/plans/harness.md` (context diet (a)–(e)) and replaces the shrinking Glut 2 turned off (`glut-2.md`, G6).

## What it is for

LocalBrain is a **fixed part of Glut**: a small model on the owner's machine that summarises as the run goes, so the
conversation grows more slowly and the paid model reads less. It is not a vault tool (no vault summaries, no local
search). Choosing a model per task — the decision model of the owner's point 4 — is a **separate later step** (L12);
the interface is shaped for it from the start.

## Where we stand

- **Researched before** (`docs/plans/jev-einsatz.md`, 2026-09; `docs/plans/harness.md`, 2026-10-14): Jev-shaped
  decisions; the context-diet directions; the owner's decisions *no Ollama in production, llama.cpp + GGUF on every
  platform, a stdio helper process (crash isolation, no port), weights in `~/.axh/models`, one trait for the jobs,
  defaults off so Glut stays standalone, a summariser of the ≤ ~3B class, own fine-tuning (LoRA) as the last rung
  after a prompt template and an eval set*. Local models as *whole agents* failed the digests (2026-09 bake-off);
  LocalBrain only does parts of a run.
- **Measured in Glut 2**: cutting old results halved the context per step on news-web but not the total — the runs
  took more steps (`glut-2.md`, G6). A summary keeps what a cut drops; whether that pays is what CP4 measures.
- **The field, October 2026** (sources at the end): sub-4B general models with GGUF that llama.cpp runs — Qwen3.5
  0.8B / 2B (Apache-2.0, 262K context), Gemma 4 E2B, Phi-4-mini 3.8B (MIT), SmolLM3 3B (Apache-2.0), Granite 4.1 3B.
  No benchmark covers *German* summaries at this size: measure on the owner's own texts. Decision models (tev1,
  JevK5, jevos, Strands Decider …) are mostly Qwen3.5 0.8–2B fine-tunes, often with custom runtimes and unsettled
  licences. `llama-cpp-2` (utilityai/llama-cpp-rs) is a thin binding with logits and embeddings; no semver — pin it.

## Decisions

- **L1 — the scope**: summarising for Glut's context. Deciding (L12) comes later; vectors and vault search are out.
- **L2 — tool results after the first look**: the model sees a long result whole in the step it arrives in; once it
  has answered on it, the result is replaced in the conversation by its local summary. The whole text stays on disk
  (`read_full_result`, its id in the summary). As Glut 2's shrinking, but a summary instead of a cut.
- **L3 — auto-compaction**: when the conversation passes a budget, its oldest turns become one local summary; the last
  turns stay verbatim. `/compact` uses the local model too. The summary is framed as notes, never as the owner's
  words, and tool output in it as untrusted data (as `compact.rs` does since #108).
- **L4 — the model's own long answers** from earlier turns are summarised too.
- **L5 — the summary's contract**: paths, errors, numbers, names, commands and **source code** verbatim — a code
  block, a signature, a diff hunk the run works with is copied, never paraphrased; German stays German; a
  success-only output compresses to its gist ("tests passed: 412"). It is machine-checkable — the same checker is
  the CP0 measure, the guard at run time (a summary that drops a path of the original is not used), and later the
  fine-tuning filter.
- **L6 — the model is chosen by measurement (CP0)** → Qwen3.5-4B (see CP0): candidates under 4B as GGUF — Qwen3.5-2B, Gemma-4-E2B,
  Phi-4-mini (3.8B), SmolLM3-3B — on real Glut transcripts, tool results, mails and news, by the L5 checker
  (recall of paths/errors/numbers/code, invented ones, compression, German), plus speed on the Mac. Licence must allow
  shipping (Apache-2.0/MIT). Ollama serves only as this bench, never in production (2026-10-14).
- **L7 — the runtime** (decided 2026-10-14, confirmed): llama.cpp through `llama-cpp-2` (pinned) in a small helper
  binary of the harness (`axh-brain`), JSON lines over stdio, the model resident, KV cache reused; the run never
  waits on a crashed helper — it restarts it, or goes on without summaries.
- **L8 — off the critical path**: a result is summarised while the paid model works on the next step, and swapped
  in when ready; a summary not ready yet just waits for the next turn. LocalBrain never makes a step slower.
- **L9 — first use downloads**: one download from Hugging Face into `~/.axh/models`, version and SHA-256 pinned in
  the harness; until it is there Glut works as today (no summaries) and says so once. No model in the app bundle.
- **L10 — the harness owns it**: the trait and the helper live in `axiomata-harness`, configured by `RunConfig`
  (default off in the library, H2); `axh` and the app switch it on when the model is there.
- **L11 — success = less money and fewer tokens in total, same quality**: A/B on real runs (news-web,
  calendar-digest, a chat with tools): clearly less spent in total — metered with the cached price (L13), the steps
  counted — *and* the same results. Only then is it on by default; otherwise it stays a switch
  and the plan records why.
- **L13 — kind to the provider's prompt cache** (owner's question, 2026-10-20): a swap is a change, and the cache
  holds only an unchanged prefix — everything after the first changed message is read at full price again.
  Measured: 67 % of Glut's input tokens on Ling came from the cache, at 20 % of the input price. So: swap only at the
  conversation's end (L2's "after the first look" keeps the swapped result among the newest messages — the prefix
  before it stays cached); never touch a summary again (the prefix stays stable); a minimum size (~2,000 characters —
  below it the broken cache costs more than the summary saves); auto-compaction rarely, down well under its budget
  (hysteresis), since it rewrites the start and breaks the whole cache once. Break-even of a swap: about one step.
- **L12 — later, its own plan**: the decision model choosing the model for a task (and `models.classify()` in code
  mode), Jev-shaped (`choice/noul/score` over a state), first answered by the same small model (grammar-forced
  choice, probability from the logits), a real decision model behind the same interface if it pays. Also later: the
  fine-tuning ladder (L5's checker as filter, synthetic + real pairs, LoRA).

## CP0 — measured 2026-10-20

Eval set: 32 real items (test runs, compiler errors, source reads, grep/shell/git output, web pages, Glut tool
results, mails, news, long German answers) — kept outside the repo. Ollama 0.40.6, GGUF Q4, an M-series Mac. The
checker (prototype in Python): recall of the paths, error codes, numbers of error lines and code names; invented
paths/numbers; length ratio; German in → German out. "Usable" = recall ≥ 0.8, at most half the length, nothing
invented — what the L5 guard would let through.

| Round / prompt | Model | recall | code | length | invented | German | time | usable |
|---|---|---|---|---|---|---|---|---|
| 1 (plain rules) | Qwen3.5-2B | 0.55 | 0.66 | 0.48 | 4/32 | 3/5 | 5.2 s | 2 |
| | Gemma-4-E2B (7.5 GB) | 0.51 | 0.31 | 0.32 | 9/32 | 1/5 | 2.1 s | 5 |
| | Phi-4-mini 3.8B | 0.31 | 0.27 | 0.26 | 2/32 | 5/5 | 3.5 s | 5 |
| | SmolLM3-3B | 0.47 | 0.46 | 0.60 | 3/32 | 4/5 | 6.8 s | 2 |
| 2 (+ size target, examples) | Qwen3.5-2B | 0.51 | 0.33 | 0.36 | 11/32 | 4/5 | 3.0 s | 1 |
| | Gemma-4-E2B | 0.26 | 0.09 | 0.09 | 20/32 | 2/5 | 0.9 s | 5 |
| | Phi-4-mini | 0.38 | 0.31 | 0.23 | 5/32 | 3/5 | 3.5 s | 3 |
| | SmolLM3-3B | 0.54 | 0.44 | 0.83 | 7/32 | 5/5 | 8.6 s | 0 |
| 3 (no example values) | Qwen3.5-2B | 0.75 | 0.68 | 0.71 | 4/32 | 2/5 | 6.9 s | 0 |
| | Phi-4-mini | 0.39 | 0.36 | 0.30 | 2/32 | 4/5 | 4.2 s | 4 |
| | SmolLM3-3B | 0.45 | 0.33 | 0.52 | 1/32 | 4/5 | 6.1 s | 0 |

| 3, at 4B (owner asked) | Qwen3.5-4B | 0.42 | 0.25 | 0.14 | 3/32 | 2/5 | 2.4 s | 5 |
| | Gemma-4-E4B (MLX) | 0.44 | 0.18 | 0.19 | 3/32 | 1/5 | 2.6 s | 7 |

No newer small Qwen exists (checked 2026-10-20): Qwen3.6 is 27B and 35B-A3B, there is no Qwen3.7, Qwen3.8 is 27B and
larger; Qwen3.5 (0.8–9B) is still the smallest family.

Findings:
- **No model up to 4B keeps the L5 contract on structured tool output** — size is not the bottleneck: the 4B ones
  compress harder and lose more paths and code. A model either compresses and drops paths,
  errors and numbers (Phi-4-mini, Gemma) or keeps them and hardly compresses (Qwen3.5-2B: recall 0.75 at 71 %
  length). At most 5 of 32 summaries would pass the guard, saving 1–7 % of the characters.
- **Small models copy the prompt's example values** (`src/run.rs:412` turned up in Gemma's summaries 20 times):
  the template must hold no concrete paths or numbers.
- They do best on **prose** (mails, news, web pages, answers); worst on grep/shell/git listings, whose content *is*
  the paths.
- A **rule-based extract** without a model (error lines and the lines they point at, paths, test counts, noise
  dropped) passed 4 of 18 structured items in a first draft and saved 27 % on them — deterministic, instant, never
  inventing; the checker itself still over-counts there (a summed test count reads as "invented").
- Speed is no problem: 1–7 s per item, prefill 2–10k tok/s, generation 90–215 tok/s, off the critical path (L8).

| 4 (+ size target, must-keep list, language — applied for real) | **Qwen3.5-4B** | **0.89** | **0.88** | **0.21** | 2/32 | **5/5** | 3.5 s | **27** (67 % of the characters saved) |
| | Phi-4-mini | 0.61 | 0.48 | 0.31 | 1/32 | 4/5 | 4.8 s | 12 (24 %) |
| | Qwen3.5-2B | 0.81 | 0.69 | 0.42 | 4/32 | 2/5 | 5.1 s | 11 (18 %) |

(Rounds 2 and 3 never sent the size target — a silent no-op in the bench script; their differences come from the
system prompt alone. Round 4 sends it.)

**What made the difference (round 4):** the closing instruction carries (1) the size target in characters, (2) the
items the rules find — error codes, paths, code names, numbers of error lines — as "must appear exactly as written",
(3) "Schreibe die Zusammenfassung auf Deutsch." when the text is German. The model no longer has to find what matters,
only to write around it. Read by hand: the compiler, source-read and German summaries are sound; one invented count
("13 distinct error locations") shows the guard must also catch numbers in a sentence. The news list scores low only
because the checker reads model ids (`org/name`) as paths.

**Decision after CP0 (owner, 2026-10-20): Qwen3.5-4B** (Apache-2.0, 262K context, GGUF Q4 ~3 GB; the 4B limit
stretched to the size itself). With round 4 the model handles structured tool output too,
so the design becomes: **rules pick what must stay, the model writes around it** (L5's extraction feeds the prompt
and checks the result); the rule-based extract is the fallback when a summary fails the guard, and the full text
stays when both would lose something. Fine-tuning stays the later rung (L12) — not needed for a start.

## Outcome (2026-10-20)

**Built on `localbrain`, not merged:** the contract (must-keep list, check, rule extract), the context diet (swap
after the first look, compaction of the older steps), the `axh-brain` helper (llama.cpp via `llama-cpp-2`, Qwen3.5-4B,
pinned download), `[agents.glut] localbrain`, `axh --local-brain`, loading the model at the app's start; hardened after
a combined review, a security audit and a dependency audit.

**CP4 — measured.** Each run in its own `AXIOMATA_HOME` with a copy of the config and of the vault files (or a git
worktree for the code sessions), so every arm started from the same state. A: off, B: LocalBrain, C: rule extract only.
Skills on Ling 3.0 Flash, the chat on DeepSeek V4.1 Flash, metered with the cached price. About $0.30 in all.

| Task | A (off) | B (LocalBrain) | C (rules only) |
|---|---|---|---|
| news-web, $ per run | 0.0117 / 0.0125 / 0.0120 | 0.0105 / 0.0099 / **0.0303** / 0.0057 / 0.0124 / 0.0104 | 0.0130 / 0.0053 / 0.0100 |
| news-web, steps | 18 / 21 / 16 | 10 / 11 / **40** / 10 / 14 / 10 | 16 / 8 / 12 |
| news-web, new entries | 3 / 0 / 4 | 4 / 3 / **0** / 9 / 10 / 6 | 11 / 6 / 6 |
| calendar-digest, $ | 0.0020 / 0.0040 | 0.0017 / 0.0017 | — |
| chat, read 3 files + answer, $ | 0.0049 / 0.0034 | 0.0022 / 0.0035 | — |
| code session, 7 turns, $ | 0.0231 / 0.0096 / 0.0102 | 0.0128 / 0.0091 / 0.0122 | — |
| code session, steps | 55 / 37 / 36 | 60 / 57 / 69 | — |

- The runs' own scatter (8–40 steps, 0–11 entries in every arm) is larger than the effect; C, which leaves web pages
  whole, did as well as B on news-web.
- Calendar and the short chat are not touched: their results are short, or the model answers at once.
- The code sessions (`axiomata-tasks`: a justfile detection with tests, clippy, edge cases) ended equally well in
  every run; LocalBrain saved 45 % once and took far more steps twice (likely the compaction's note: the model then
  worked out again what it had done — not proven).
- Found on the way: a compaction rewrote a tiny older part for nothing (fixed on the branch); Glut repeats a refused
  call up to the turn limit (card #112, independent of LocalBrain).

**Why it was not taken over:** with the providers in use, old context is already nearly free — cached input costs
3 % (DeepSeek) to 20 % (Ling) of the input price — so what saves money is a *stable* conversation start, and every
swap breaks part of it. What a summary would really buy is room under the context limit in long code sessions, and the
usual way for that needs no second model: Glut's own model compacts, only shortly before the limit (`/compact` exists;
card #111 "Kürzen dreifach"). Against that stood a native build dependency (cmake, llama.cpp), a sidecar, ~3 GB of
memory, a 2.7 GB download and the upkeep of every llama.cpp update — for a switch that stays off.

**Kept for later:** the branch; the idea of a fine-tuned 1.5–2B model for machines with less memory (L12's ladder), if
a case for a second model ever appears; L12's decision model is unaffected and remains its own plan. The model, the
CP0 candidates in Ollama and cmake were removed from the machine (owner, 2026-10-20).

## Checkpoints

- **CP0 — bench** (no product code): the prompt template for L5, the checker (Rust, reused at run time), an eval set
  from real transcripts/results/mails/news (kept out of the repo — the owner's data), the four candidates on Ollama;
  pick model + quantisation, write the numbers into this plan.
- **CP1 — the trait and the context diet** (`axiomata-harness`): `LocalBrain::summarise`, L2 (summary instead of the
  cut in `history.rs`), L3 (auto-compaction, `/compact`), L4, L8 (background summaries swapped in), the L5 guard; a
  scripted brain in the tests.
- **CP2 — the helper** (`axh-brain`): `llama-cpp-2` pinned, stdio protocol, resident model, restart on crash, time
  limits; the download with SHA-256 check (L9).
- **CP3 — wired in**: `axh` and the app's Glut runs use it when the model is there; a setting to switch it off; an
  event per summary (what it saved), visible in the run's log.
- **CP4 — measure (L11)**: A/B on real runs, the result decides the default.
- **CP5 — docs + review**: `architecture.md`, AGENTS.md; the combined review, the security-auditor (a download, a
  spawned helper, model output into the conversation), the dependency audit (`llama-cpp-2`).

## Sources (2026-10-20)

- [tinyweights.dev — best small language models 2026](https://tinyweights.dev/posts/best-small-language-models-2026/)
- [arXiv 2502.00641 — small models for news summarisation](https://arxiv.org/html/2502.00641v2)
- [mindstudio — Gemma 4 vs Qwen 3.5](https://www.mindstudio.ai/blog/gemma-4-vs-qwen-3-5-open-weight-comparison)
- [Hugging Face — Qwen3.5-4B GGUF (llama.cpp)](https://huggingface.co/SoAIHQ/Qwen3.5-4B-GGUF)
- [arXiv 2606.13111 — MÖVE, German public-sector benchmark](https://arxiv.org/pdf/2606.13111)
- [awesome-decision-models](https://github.com/AnotiaWang/awesome-decision-models)
- [modelfit — Nimble and Tev1 on Mac](https://modelfit.io/blog/ollama-decision-models-nimble-tev1-mac/)
- [utilityai/llama-cpp-rs](https://github.com/utilityai/llama-cpp-rs), [docs.rs llama-cpp-2](https://docs.rs/llama-cpp-2)
