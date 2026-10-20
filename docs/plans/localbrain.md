# LocalBrain — Glut's context kept small by a local model (owner's point 4)

Status: grilled 2026-10-20 (L1–L12), waiting for the owner's go. Builds on the decisions of 2026-10-14 in
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
- **L5 — the summary's contract**: paths, errors, numbers, names and commands verbatim; German stays German; a
  success-only output compresses to its gist ("tests passed: 412"). It is machine-checkable — the same checker is
  the CP0 measure, the guard at run time (a summary that drops a path of the original is not used), and later the
  fine-tuning filter.
- **L6 — the model is chosen by measurement (CP0)**: candidates under 4B as GGUF — Qwen3.5-2B, Gemma-4-E2B,
  Phi-4-mini (3.8B), SmolLM3-3B — on real Glut transcripts, tool results, mails and news, by the L5 checker
  (recall of paths/errors/numbers, invented ones, compression, German), plus speed on the Mac. Licence must allow
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
- **L11 — success = fewer tokens, same quality**: A/B on real runs (news-web, calendar-digest, a chat with tools):
  clearly fewer tokens in total *and* the same results. Only then is it on by default; otherwise it stays a switch
  and the plan records why.
- **L12 — later, its own plan**: the decision model choosing the model for a task (and `models.classify()` in code
  mode), Jev-shaped (`choice/noul/score` over a state), first answered by the same small model (grammar-forced
  choice, probability from the logits), a real decision model behind the same interface if it pays. Also later: the
  fine-tuning ladder (L5's checker as filter, synthetic + real pairs, LoRA).

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
