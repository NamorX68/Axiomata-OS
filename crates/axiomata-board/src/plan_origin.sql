-- The board card a plan was made for (docs/plans/card-as-plan.md, K1): starting a card as a plan makes the card the
-- plan's goal, and the card then follows the plan across the board (In Progress, Review, Done).
--
-- Beside `plans` rather than a column of it, so the plan model stays. The card deliberately gets no `plan_id`: a card
-- of a plan is one of its steps, and a plan that runs by itself would start it like one. `card_id` is not unique: once
-- a plan is closed, its card may be started as a plan again (the code allows one open plan per card).
CREATE TABLE IF NOT EXISTS plan_origin (
    plan_id INTEGER PRIMARY KEY REFERENCES plans (id) ON DELETE CASCADE,
    card_id INTEGER NOT NULL REFERENCES cards (id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS plan_origin_card ON plan_origin (card_id);
