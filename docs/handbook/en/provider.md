# AI providers and harness per role

Which harness (Claude Code or Glut) and which provider (Anthropic, OpenRouter, Ollama) serve the chat and the skills, how to choose models, sign in to Claude Code and set up Glut's web search.

## What it is

- The **harness** drives the agent — its tools, rights and loop: **Claude Code** (Anthropic only, with
  a sign-in of the app's own, separate from yours in `~/.claude`) or **Glut**, Axiomata's own harness
  (OpenRouter or Ollama).
- The **provider** supplies the model: **Anthropic** (through the subscription, without a key), **OpenRouter** (with
  an API key) or **Ollama** (local on this Mac).
- There are two **roles**: the chat and “skills & routines”. Each role chooses harness and provider for itself — for
  example Claude Code for the chat and Glut with Ollama for the digests.
- A skill can deviate from this with `backend:` and `model:` in its own header. The Studio's agents, by contrast,
  choose their model through their **tier** and its engine (see [Models and roles](modelle-rollen.md)).

## Where the buttons are

**Settings → AI providers:**

- **“Edit providers”:** the three providers (the badges “Chat” / “Skills” show who is serving right now). A click
  opens the form: “Base URL”, “API key” (stored as “•••••• saved — leave empty to keep”), “Chat model”,
  “Skills/routines model” (chosen from the catalog, ↻ refreshes it) → “Save provider”.
- **“Harness & provider assignment”:** per role “Harness for …”, “Provider for …” and with Glut “Effort for …”
  (empty = “Glut's own setting”).
- **“Claude Code sign-in”** (when a role uses Claude Code): “Sign in with Anthropic”, if needed “Code (only if the
  browser page shows one)” → “Send”; once signed in: “Sign out”.
- **“Web search (Glut)”:** keys for Exa, TinyFish and Tavily, “Searches per day (all providers)”, “Tavily
  searches per month”.
- The spend lines and “Daily limit (USD, paid providers)” (see [Costs](kosten.md)), then “Save assignment”.

## Common tasks

1. **Put the chat on another model:** “Edit providers” → the chat's provider → “Chat model” → “Save provider”. For
   one conversation only: `/model <id>` in the chat.
2. **Run the digests locally:** “Harness for skills & routines” = Glut, “Provider for skills & routines” =
   Ollama → “Save assignment”; with Ollama, set the skills model.
3. **Sign in to Claude Code:** “Sign in with Anthropic” → confirm in the browser → enter the code if needed → “Send”.

## Traps

- Claude Code can do only Anthropic, Glut only OpenRouter and Ollama — the choice of providers follows the harness.
- Without a sign-in, chat and skills on Claude Code fail; a notice then appears at start.
- An API key is never shown again after saving. An emptied key field deletes the key on save
  (“The key is deleted on save.”).
- A `model:` in a skill's header beats the provider's skills model.
- Setting “Searches per day” or “Tavily searches per month” to 0 turns off the search or Tavily; Glut asks Exa, then
  TinyFish, then Tavily and skips a provider without a key.
- “Save provider” and “Save assignment” both save **everything** on this tab; it applies from the next
  agent call. Anything unsaved does not apply.

## See also

[Costs](kosten.md) · [Glut](glut.md) · [Chat](chat.md) · [Skills](skills.md) · [Settings](einstellungen.md)
