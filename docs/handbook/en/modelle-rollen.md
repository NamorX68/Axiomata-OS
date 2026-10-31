# Models, tiers and roles

How the Studio switches models on and sorts them into the tiers Light, Medium and Heavy, which engine a tier starts, what a role is and what the one-time switch to roles with tiers did.

## What it is

An **agent** in the Studio is always **role + tier + engine** — worked out, not stored:

- The **role** says what for: working instructions, rights, limits, kind of work and a **default tier**. It names
  **no engine**. Seven come with the app: allrounder (Medium), implementer (Medium), documenter (Light), planner
  (Heavy), reviewer (Medium), tester (Medium), grill (Heavy); your own come on top
  (`~/.axiomata/agents/<id>/AGENT.md`).
- The **tier** (Light · Medium · Heavy) says how strong: Light for small, clearly bounded changes, Medium for the
  usual case, Heavy for big or delicate work.
- The **engine** is harness + model. Every **switched-on** model gets an engine by itself for each harness that can
  run it: Claude Code for Anthropic (only the newest model of each family), Glut for OpenRouter and Ollama. These
  **automatic engines** cannot be edited; if a model disappears from the catalog, its engine disappears from the
  choice, running sessions stay.

**Where a model's tier comes from**, in this order: your own choice → the research's proposal → the proposal from
the price → none (“Unrated”, then the model is no agent yet). The **price proposal** measures the output price per
million tokens: below $1 Light, $1–10 Medium, above $10 Heavy. The **research** is the bundled skill
`model-tiering`: it looks things up on rating sites (Artificial Analysis, LMArena, OpenRouter rankings) and files a
tier per model with a one-sentence reason and a source as a proposal — never as your choice. It starts by itself
when a switched-on model without a tier shows up after a catalog refresh (at most once a day per model), and at the
press of a button. Ollama models have no price; for them only the research applies.

**Which engine a tier starts:** your **default engine** of the tier, without a choice the cheapest of the tier. If
it cannot run (key missing, not reachable), the Studio takes the next one of the same tier.

A provider's small model list (up to 20 models) is switched on in full; for a large one everything is off, and you
tick what you want.

## Where the buttons are

Studio → bar “Agents” → **“Models & roles…”** opens a large pane with two tabs:

- **“Models”**
  - **“Default engine per tier”**: one choice per tier; “Cheapest: …” means without a choice of your own.
  - **“Price limits”**: “Light below”, “Heavy above”, “Save limits”.
  - **“Models”** with “Propose all again”; below it, per provider, the models with a filter field (“Filter …
    models…”), per model a switch (“Switch on” / “Switch off”), price and context, the tier buttons Light · Medium ·
    Heavy (a click on your choice takes it back: “Your tier — click to drop it”), the origin (“Yours”, “Proposal ·
    not reviewed”, “From the price”, “Unrated · no agent until it has a tier”), with an open proposal the mark
    “Waits for your review” and “Accept”, the reason with its source and “Propose again for …”. While the research
    runs, “Researching model tiers…” stands at the top.
  - **“Own engines”** at the very bottom: “Add engine…” with “Label”, “Harness”, “Model”, “Tier” (“The tier of its
    model” without a choice) and under “Advanced” “Command (empty = the harness default)”, “Environment” and
    “Billing”.
- **“Roles”**: the roles; “Add role…” and per role “Name”, “Description”, “Kind of work”, “Default tier”, “Working
  instructions (Markdown)”, “Rights (harness rules, one per line)”, under “Advanced” the limits (“Step limit”,
  “Token limit”, “Cost limit (USD)”) and “Creates cards without asking”; “Save role”.
- **Switch notice** in the “Roles” tab below the header (only after the switch): one line on what happened, where
  the backup is, “OK”.

In the terminal: `axiomata-cli ide models list|set|limits|default`, `ide engines list`, `ide roles list|show`,
`ide migrate-agents --dry-run`.

## Common tasks

1. **Make a model usable:** “Models” → find the model → switch it on; if it has no tier, it waits for the research
   or you click a tier yourself.
2. **Accept or change a proposal:** at the model “Accept” — or click another tier; your choice always wins over any
   proposal.
3. **Set what a tier runs on:** “Default engine per tier” → pick the engine per tier.
4. **Make a role sharper:** “Roles” → role → change instructions, rights or “Default tier” → “Save role”.
5. **An engine of your own** (own command, own environment, own tier): “Own engines” → “Add engine…”.

## The switch at the first start

Earlier every role carried an engine, hence copies such as `planner-haiku` or `reviewer-heavy`. At the first start
with the new model the app switches over once:

- The seven base roles are created if they are missing. **Every copy** (`planner-haiku`, `reviewer-heavy` …) moves
  into the backup, even one with one or two lines of its own: how strong an agent is is said by its tier, not by a
  copy of the role. Their sessions and favorites play the base role afterwards. What `reviewer-light` and
  `reviewer-heavy` said of their own, every reviewer's start instruction now says, by the card's tier.
- Own engines that are merely an automatic one merge into it (their model is switched on). An own engine with its
  own command, own environment, own tier or own billing stays.
- For each tier without your choice, the default engine is set to the one the old roles of that tier used most.
  Default engines, favorites and sessions that pointed at a removed engine move to the automatic one. Names,
  worktrees and branches of the sessions stay.
- **Nothing is deleted:** every run creates its own backup folder `~/.axiomata/agents-backup-<date>` (with `-2` …
  for a second run on the same day) with the originals of the roles and the removed engines (`engines.toml`). It
  stays until you delete it under Settings → Storage ([Storage](storage.md)).

## Traps

- A model without a tier is **no agent**: it does not show up under “Start an agent”.
- The research is a paid skill run on the skill harness ([Costs](kosten.md)).
- A role names no engine; an `engine:` in a role file is ignored — also in a project's roles
  (`.axiomata/agents/`), which the app says when you confirm them.
- Chat and skills keep choosing their model under Settings → “AI providers” ([Providers](provider.md)), not here.
- A default engine that does not fit the tier stands as “— not of this tier” in the choice; better pick a new one.

## See also

[Studio](studio.md) · [Flow](flow.md) · [Storage](storage.md) · [Costs](kosten.md) · [Providers](provider.md)
