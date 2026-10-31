# Glut in Axiomata

What Glut is and where it works in Axiomata: as the harness for chat and skills, as a card run in a Studio pane, as the Glut master in a project — and how to answer its questions.

## What it is

**Glut** is Axiomata's own harness: the program that drives an agent with tools, rights and a loop —
on models from OpenRouter or Ollama. It is an application of its own (`glut`) with its own storage `~/.glut`
(settings, MCP servers in `~/.glut/mcp.json`, sessions); Axiomata only passes values on top. In
Axiomata, Glut works in three places:

- **Chat and skills:** when “Glut” is chosen as a role's harness under Settings → AI providers. Then you get
  the “Effort” switch and Glut's web search. The digests reach Mail, Calendar and Reminders through Glut's
  MCP servers.
- **Cards in the Studio:** an engine of kind Glut runs a card in its pane with Glut's own interface (TUI).
  The model is fixed (no `/model` in the pane), the rights come from the role, and the session's remaining budget
  goes along as the run's limit. A task the reviewer returns continues in the same conversation.
- **Glut master:** the first master tile in the Studio's Agents area — your own Glut in the real project folder,
  with the team tools, without a card and without a worktree.

## Where the buttons are

- **Choose the harness:** Settings → AI providers → “Harness for chat” / “Harness for skills & routines” → “Glut”.
- **Engine for cards:** every enabled OpenRouter or Ollama model is a Glut engine by itself (Studio →
  Agents → “Models & roles…” → “Models”, [Models and roles](modelle-rollen.md)). Your own: there under “Own
  engines” → “Add engine…” → harness Glut; in the “Model” field, `openrouter/…` or `ollama/…` picks the provider.
- **Open the master:** Studio → Agents → the master tile “Glut” at the top.
- **Watch:** “Watch” on a live tile in the Flow — follows the session read-only in a pane of its own.
- **Answer questions:** in the dialog in the pane **or** in the app (“Allow” / “Deny” / “In terminal”); the first
  answer counts.
- **Space:** Settings → Storage shows what Glut uses; Glut cleans up `~/.glut` itself.

## Common tasks

1. **Digests on Glut + Ollama:** Settings → AI providers → Skills: harness “Glut”, provider “Ollama” → “Save
   assignment”.
2. **Answer a Glut question:** follow the badge “… is waiting for you” → “Allow” or “Deny” — or answer in the
   pane.
3. **Resume an interrupted run:** in the Skills Deck “resume” (or “drop”); in the chat “Resume”; a
   card pane offers to continue when you reopen it.

## Traps

- Glut serves **no Anthropic**; a skill with `backend: axh` needs a skill provider with an endpoint
  (OpenRouter or Ollama), otherwise it fails with a clear message instead of silently switching elsewhere.
- Glut's sandbox hides `~/.axiomata` from every command; only some approved `axiomata-cli` commands bypass it.
  A skill gets only the rights from its `allowed_tools`; `deny`/`ask` from `~/.glut` still apply.
- A new build of the app takes Glut from its own repository; a Glut setting belongs in
  `~/.glut/settings.json` or in Glut's own commands, not in Axiomata.
- If a Glut card waits for an answer and nobody is at the pane, it waits until you answer in the app.

## See also

[Providers](provider.md) · [Studio](studio.md) · [Flow](flow.md) · [Skills](skills.md) · [Storage](storage.md)
