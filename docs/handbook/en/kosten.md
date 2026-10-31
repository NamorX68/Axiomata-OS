# Costs, prices and daily limits

What agent runs cost, where the prices come from and which limits stop the chat, skills and the Studio.

## What it is

Axiomata measures money only for **paid providers** (OpenRouter; Ollama counts nothing). Anthropic runs through the
subscription and the Claude Code login — there is no cost tracking for it, and the line then reads “billed through the
subscription — no cost tracking”. What is measured is not the tool's estimate but **tokens × price per model**; the
part of the input that comes from the cache is counted at the cheaper cache price, if one is known.

**The prices** come from OpenRouter's price list: every time the model catalog is refreshed (at app start, once
a day, with the ↻ at the model field in the settings or `axiomata-cli models refresh`) the app writes the
list prices of the models in use into its price table. A paid price is never replaced by a free
one. At start the app re-meters runs already recorded against the table.

**The limits** (defaults):

| Limit | applies to | default |
|---|---|---|
| “Daily limit (USD, paid providers)” | chat and skills/routines together | $2 a day |
| A plan's limit | all sessions of a Studio plan | $15 and 6 M tokens |
| A card plan's limit | a card plan started with “Start” | $5 |
| The Studio's daily limit | all plan and card sessions of a day | $20 |
| A session's limits | one card session, by the role's tier | Light $0.50, Medium $2, Heavy $6 (plus tokens, steps) |

## Where the buttons are

- **Settings → AI providers**, at the bottom of the section “Harness & provider assignment”: per role the line “Spend
  chat → …” with today / limit / this month, below it the field “Daily limit (USD, paid providers)” (empty = no
  limit), then “Save assignment”.
- **Studio → Flow**, in the plan: the field “Cost limit (USD)” above the goal (empty = no limit of its own) and the
  line “Spend: …” with the Studio's total for the day.
- **Studio → Agents area → “Models & roles…” → “Roles”**, open a role → “Advanced”: “Cost limit (USD)”,
  “Token limit”, “Step limit” (empty = the tier's default).
- In the terminal: `axiomata-cli board usage` shows a session's steps, tokens and money against its limits.

## Common tasks

1. **Change the daily limit for chat and skills:** Settings → AI providers → “Daily limit (USD, paid providers)” →
   “Save assignment”.
2. **A paused plan should keep running:** “Continue” at the plan in the Flow — this sets its limit to the
   spend so far plus a new allowance.
3. **Check what was spent today:** Settings → AI providers, the “Spend …” lines; for the Studio the
   “Spend” line in the Flow.

## Traps

- Once the daily limit is reached, the **next** call through a paid provider is refused — including the
  refresh of a Calendar or Mail tile, because every refresh is a paid agent turn.
- The Studio's daily limit ($20) is separate from the limit for chat and skills and has **no field in the
  interface**; it is `studio_daily_usd_cap` in the config. Once it is reached, nothing new starts until tomorrow
  and running paid sessions are stopped.
- A model without a known price counts no money; then only the token or step limit applies.
- Glut's web search has limits of its own (searches per day, Tavily per month) — see [Providers](provider.md).

## See also

[Providers](provider.md) · [Flow](flow.md) · [Studio](studio.md) · [Connections](verbindungen.md)
