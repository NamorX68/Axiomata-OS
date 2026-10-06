//! What the studio's agent sessions spent, kept beyond the sessions (`docs/plans/a2a.md`, A9, A27; CP-A8c).
//!
//! A session's usage is read from its harness while it lives ([`crate::session_limits::Meter`]) and goes away with the
//! session when its card is integrated. A limit over a whole **plan** or a whole **day** therefore needs a ledger: every
//! look writes down how much a session used since the look before ([`record_look`]), and the limits add the rows up.
//!
//! Two figures are kept apart on purpose. **Tokens** are counted for every engine; **dollars** only for an engine paid per
//! token whose model has a price in the owner's table (`spend::metered_cost_usd`). A subscription engine never has a
//! dollar figure, and a missing price is "not metered", not "free": the dollar limit of a plan or a day therefore holds
//! only what could be priced, and the token limit of a plan holds everything.
//!
//! This ledger is separate from the chat and skill spend (`spend.rs`, `daily_usd_cap`): the day's cap here is for studio
//! sessions only.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::board::Plan;
use crate::config::Config;
use crate::error::AxiomataError;
use crate::roster::refusal;
use crate::spend;
use axiomata_ide::usage::Usage;

/// What was spent, summed over some rows of the ledger.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Spent {
    /// Input, output and cache writes of every engine.
    pub tokens: u64,
    /// Tool calls.
    pub steps: u64,
    /// Dollars of the engines that could be metered.
    pub cost_usd: f64,
}

/// What the ledger holds for one source of one session, for the increase against it. Rows without a source (written before
/// there was one) count toward every source: their origin is unknown, and counting them twice would be the worse mistake.
fn session_total(db: &Connection, agent_id: i64, source: &str) -> Result<Spent, AxiomataError> {
    sum(
        db,
        "WHERE agent_id = ?1 AND (source = ?2 OR source = '')",
        params![agent_id, source],
    )
}

/// The source of a Claude Code session's figures: all the session's transcripts together.
pub const SOURCE_CLAUDE: &str = "claude";

fn sum(db: &Connection, filter: &str, args: impl rusqlite::Params) -> Result<Spent, AxiomataError> {
    let (tokens, steps, cost): (i64, i64, f64) = db.query_row(
        &format!(
            "SELECT COALESCE(SUM(tokens), 0), COALESCE(SUM(steps), 0), COALESCE(SUM(cost_usd), 0.0) \
             FROM session_spend {filter}"
        ),
        args,
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    Ok(Spent {
        tokens: u64::try_from(tokens).unwrap_or(0),
        steps: u64::try_from(steps).unwrap_or(0),
        cost_usd: cost,
    })
}

/// Writes down what session `agent_id` used since the last look: the difference between what `usage` (and `cost_usd`)
/// say and what the ledger already holds for the same `source` — [`SOURCE_CLAUDE`], or the Opencode session id, since an
/// escalation starts a new Opencode session whose figure begins near zero. Nothing is written when nothing grew, and a reading
/// that fell (a window that moved on) writes nothing either — the ledger never takes spending back.
///
/// # Errors
///
/// A database error.
pub fn record_look(
    db: &Connection,
    agent_id: i64,
    source: &str,
    plan_id: Option<i64>,
    usage: Usage,
    cost_usd: Option<f64>,
    now: DateTime<Utc>,
) -> Result<(), AxiomataError> {
    let before = session_total(db, agent_id, source)?;
    let tokens = usage.tokens().saturating_sub(before.tokens);
    let steps = u64::from(usage.steps).saturating_sub(before.steps);
    let cost = cost_usd.map(|cost| (cost - before.cost_usd).max(0.0));
    if tokens == 0 && steps == 0 && cost.is_none_or(|cost| cost <= 0.0) {
        return Ok(());
    }
    db.execute(
        "INSERT INTO session_spend (agent_id, source, plan_id, tokens, steps, cost_usd, at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            agent_id,
            source,
            plan_id,
            i64::try_from(tokens).unwrap_or(i64::MAX),
            i64::try_from(steps).unwrap_or(i64::MAX),
            cost,
            now.to_rfc3339()
        ],
    )?;
    Ok(())
}

/// Everything the sessions of plan `plan_id` spent so far, the integrated cards' sessions included.
///
/// # Errors
///
/// A database error.
pub fn plan_spent(db: &Connection, plan_id: i64) -> Result<Spent, AxiomataError> {
    sum(db, "WHERE plan_id = ?1", params![plan_id])
}

/// Everything the studio's sessions spent since `since` (the start of the local day for the day's cap).
///
/// # Errors
///
/// A database error.
pub fn spent_since(db: &Connection, since: DateTime<Utc>) -> Result<Spent, AxiomataError> {
    sum(db, "WHERE at >= ?1", params![since.to_rfc3339()])
}

/// What the studio's sessions spent today, local time.
///
/// # Errors
///
/// A database error.
pub fn spent_today(db: &Connection, now: DateTime<Utc>) -> Result<Spent, AxiomataError> {
    spent_since(db, spend::start_of_local_day(now))
}

/// The day's cap, in words, while the studio has spent it. A look that cannot be made reads as "not reached": the cap is a
/// brake, and a database hiccup is no reason to stall the reviews.
pub fn day_cap_reached(core: &crate::AxiomataCore) -> Option<String> {
    let config = core.config_read().clone();
    let db = core.db_lock();
    spent_today(&db, Utc::now())
        .ok()
        .and_then(|today| day_over(&config, today))
}

/// Whether the day's cap stops a session: only one whose dollars were metered (`cost_usd` is `Some` for an engine paid per
/// token with a priced model, and for no other, an unreadable session included), and only when no limit of its own
/// already did. A session whose spending cannot be seen in dollars would not be helped by stopping it.
pub fn stopped_by_day(own_limit_hit: bool, cost_usd: Option<f64>, day: Option<&str>) -> bool {
    !own_limit_hit && cost_usd.is_some() && day.is_some()
}

/// The limits a plan is held to: its own where it sets them, the app's defaults for the rest.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PlanLimits {
    pub max_cost_usd: f64,
    pub max_tokens: u64,
}

/// The limits of `plan`.
pub fn plan_limits(config: &Config, plan: &Plan) -> PlanLimits {
    PlanLimits {
        max_cost_usd: plan.max_cost_usd.unwrap_or(config.agents.plan_max_cost_usd),
        max_tokens: plan.max_tokens.unwrap_or(config.agents.plan_max_tokens),
    }
}

/// Which limit of a plan its spending has reached, in words; `None` while it is within both.
pub fn plan_over(spent: Spent, limits: PlanLimits) -> Option<String> {
    if spent.tokens >= limits.max_tokens {
        return Some(format!(
            "{} tokens of {} used",
            spent.tokens, limits.max_tokens
        ));
    }
    if spent.cost_usd >= limits.max_cost_usd {
        return Some(format!(
            "${:.2} of ${:.2} spent",
            spent.cost_usd, limits.max_cost_usd
        ));
    }
    None
}

/// The day's cap, in words, once today's spending has reached it; `None` while it is not (or no cap is set).
pub fn day_over(config: &Config, today: Spent) -> Option<String> {
    let cap = config.agents.studio_daily_usd_cap?;
    (today.cost_usd >= cap).then(|| {
        format!(
            "${:.2} of the day's ${cap:.2} for studio sessions spent",
            today.cost_usd
        )
    })
}

/// What the Flow shows of a plan's spending: the figures, the limits they are held to, and why the plan is held back.
#[derive(Debug, Clone, Serialize)]
pub struct PlanSpend {
    pub spent: Spent,
    pub limits: PlanLimits,
    /// Which limit of the plan is reached, while one is.
    pub plan_over: Option<String>,
    /// The day's cap, once it is reached: it holds back every plan, not just this one.
    pub day_over: Option<String>,
    /// What the studio spent today, for the figure next to the cap.
    pub today: Spent,
    /// The day's cap in dollars; `None` when it is off.
    pub day_cap_usd: Option<f64>,
}

/// The spending of plan `plan_id`, as the Flow and `board plan spend` show it.
///
/// # Errors
///
/// A database error, or a plan that does not exist.
pub fn plan_spend(
    db: &Connection,
    config: &Config,
    plan_id: i64,
) -> Result<PlanSpend, AxiomataError> {
    let Some(plan) = crate::board::flow::get_plan(db, plan_id)? else {
        return Err(refusal("plan", format!("no plan with id {plan_id}")));
    };
    let spent = plan_spent(db, plan_id)?;
    let limits = plan_limits(config, &plan);
    let today = spent_today(db, Utc::now())?;
    Ok(PlanSpend {
        spent,
        limits,
        plan_over: plan_over(spent, limits),
        day_over: day_over(config, today),
        today,
        day_cap_usd: config.agents.studio_daily_usd_cap,
    })
}

/// Gives plan `plan_id` a fresh allowance from what it has spent now: the owner's "go on" after a limit paused it. Each
/// limit becomes what was spent plus the app's default for a plan — but never less than it was, so a limit set high by
/// hand stays, and one set low by hand is not kept at it.
///
/// # Errors
///
/// A database error, a plan that does not exist, or one that has not reached a limit.
pub fn resume_plan(db: &Connection, config: &Config, plan_id: i64) -> Result<Plan, AxiomataError> {
    let Some(plan) = crate::board::flow::get_plan(db, plan_id)? else {
        return Err(refusal("plan", format!("no plan with id {plan_id}")));
    };
    let spent = plan_spent(db, plan_id)?;
    let limits = plan_limits(config, &plan);
    if plan_over(spent, limits).is_none() {
        return Err(refusal(
            "plan",
            format!("plan #{plan_id} has not reached a limit; there is nothing to go on from"),
        ));
    }
    // Never below what the plan was set by hand: "go on" adds an allowance, it does not take one away.
    let fields = crate::board::PlanFields {
        name: plan.name.clone(),
        goal: plan.goal.clone(),
        project_id: plan.project_id,
        auto_start_max: plan.auto_start_max,
        max_cost_usd: Some(
            limits
                .max_cost_usd
                .max(spent.cost_usd + config.agents.plan_max_cost_usd),
        ),
        max_tokens: Some(
            limits
                .max_tokens
                .max(spent.tokens.saturating_add(config.agents.plan_max_tokens)),
        ),
    };
    crate::board::flow::update_plan(db, plan_id, &fields)?
        .ok_or_else(|| refusal("plan", format!("no plan with id {plan_id}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("db/migrations/0008_session_spend.sql"))
            .unwrap();
        conn.execute_batch(include_str!("db/migrations/0009_session_spend_source.sql"))
            .unwrap();
        conn
    }

    fn usage(input: u64, output: u64, steps: u32) -> Usage {
        Usage {
            input_tokens: input,
            output_tokens: output,
            steps,
        }
    }

    #[test]
    fn a_look_writes_only_what_grew_since_the_last_one() {
        let db = db();
        let now = Utc::now();
        record_look(
            &db,
            1,
            SOURCE_CLAUDE,
            Some(7),
            usage(100, 50, 3),
            Some(0.10),
            now,
        )
        .unwrap();
        record_look(
            &db,
            1,
            SOURCE_CLAUDE,
            Some(7),
            usage(100, 50, 3),
            Some(0.10),
            now,
        )
        .unwrap();
        record_look(
            &db,
            1,
            SOURCE_CLAUDE,
            Some(7),
            usage(300, 70, 5),
            Some(0.25),
            now,
        )
        .unwrap();
        let spent = plan_spent(&db, 7).unwrap();
        assert_eq!(spent.tokens, 370);
        assert_eq!(spent.steps, 5);
        assert!((spent.cost_usd - 0.25).abs() < 1e-9);
        let rows: i64 = db
            .query_row("SELECT COUNT(*) FROM session_spend", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 2, "the unchanged look wrote nothing");
    }

    #[test]
    fn a_new_opencode_session_after_an_escalation_counts_from_its_own_zero() {
        let db = db();
        let now = Utc::now();
        record_look(
            &db,
            1,
            "ses_old",
            Some(7),
            usage(900_000, 100_000, 40),
            None,
            now,
        )
        .unwrap();
        // The escalation made a new Opencode session: its figure starts near zero. Against everything the agent spent, its
        // first looks would write nothing until it passed a million tokens.
        record_look(
            &db,
            1,
            "ses_new",
            Some(7),
            usage(30_000, 5_000, 3),
            None,
            now,
        )
        .unwrap();
        assert_eq!(plan_spent(&db, 7).unwrap().tokens, 1_035_000);
        record_look(
            &db,
            1,
            "ses_new",
            Some(7),
            usage(50_000, 9_000, 6),
            None,
            now,
        )
        .unwrap();
        assert_eq!(plan_spent(&db, 7).unwrap().tokens, 1_059_000);
        // A change of harness (Claude Code to Opencode and back) is a change of source too.
        record_look(
            &db,
            1,
            SOURCE_CLAUDE,
            Some(7),
            usage(1_000, 0, 1),
            None,
            now,
        )
        .unwrap();
        assert_eq!(plan_spent(&db, 7).unwrap().tokens, 1_060_000);
    }

    #[test]
    fn rows_from_before_the_source_existed_count_toward_every_source_of_their_agent() {
        let db = db();
        let now = Utc::now().to_rfc3339();
        db.execute(
            "INSERT INTO session_spend (agent_id, plan_id, tokens, steps, cost_usd, at) VALUES (1, 7, 500, 5, NULL, ?1)",
            [&now],
        )
        .unwrap();
        // The same session read again under a source: only what is more than the old rows holds is new.
        record_look(
            &db,
            1,
            SOURCE_CLAUDE,
            Some(7),
            usage(500, 0, 5),
            None,
            Utc::now(),
        )
        .unwrap();
        assert_eq!(
            plan_spent(&db, 7).unwrap().tokens,
            500,
            "nothing counted twice"
        );
        record_look(
            &db,
            1,
            SOURCE_CLAUDE,
            Some(7),
            usage(700, 0, 6),
            None,
            Utc::now(),
        )
        .unwrap();
        assert_eq!(plan_spent(&db, 7).unwrap().tokens, 700);
    }

    #[test]
    fn a_reading_that_fell_takes_nothing_back() {
        let db = db();
        let now = Utc::now();
        record_look(
            &db,
            1,
            SOURCE_CLAUDE,
            Some(7),
            usage(500, 100, 9),
            Some(1.0),
            now,
        )
        .unwrap();
        record_look(
            &db,
            1,
            SOURCE_CLAUDE,
            Some(7),
            usage(200, 50, 4),
            Some(0.4),
            now,
        )
        .unwrap();
        let spent = plan_spent(&db, 7).unwrap();
        assert_eq!(spent.tokens, 600);
        assert!((spent.cost_usd - 1.0).abs() < 1e-9);
    }

    #[test]
    fn sessions_and_plans_add_up_separately_and_an_unmetered_session_has_no_dollars() {
        let db = db();
        let now = Utc::now();
        record_look(
            &db,
            1,
            SOURCE_CLAUDE,
            Some(7),
            usage(100, 0, 1),
            Some(0.5),
            now,
        )
        .unwrap();
        record_look(&db, 2, SOURCE_CLAUDE, Some(7), usage(100, 0, 1), None, now).unwrap();
        record_look(
            &db,
            3,
            SOURCE_CLAUDE,
            Some(8),
            usage(100, 0, 1),
            Some(0.5),
            now,
        )
        .unwrap();
        let seven = plan_spent(&db, 7).unwrap();
        assert_eq!(seven.tokens, 200);
        assert!((seven.cost_usd - 0.5).abs() < 1e-9);
        assert_eq!(plan_spent(&db, 8).unwrap().tokens, 100);
    }

    #[test]
    fn today_leaves_out_what_was_spent_before_midnight() {
        let db = db();
        let now = Utc::now();
        let long_ago = now - chrono::Duration::days(3);
        record_look(
            &db,
            1,
            SOURCE_CLAUDE,
            None,
            usage(100, 0, 1),
            Some(5.0),
            long_ago,
        )
        .unwrap();
        record_look(
            &db,
            2,
            SOURCE_CLAUDE,
            None,
            usage(100, 0, 1),
            Some(1.5),
            now,
        )
        .unwrap();
        let today = spent_today(&db, now).unwrap();
        assert!((today.cost_usd - 1.5).abs() < 1e-9);
    }

    #[test]
    fn a_plan_is_over_by_tokens_or_by_dollars_and_a_missing_cap_never_stops_the_day() {
        let limits = PlanLimits {
            max_cost_usd: 15.0,
            max_tokens: 6_000_000,
        };
        let under = Spent {
            tokens: 1,
            steps: 1,
            cost_usd: 1.0,
        };
        assert_eq!(plan_over(under, limits), None);
        assert!(
            plan_over(
                Spent {
                    tokens: 6_000_000,
                    ..under
                },
                limits
            )
            .is_some()
        );
        assert!(
            plan_over(
                Spent {
                    cost_usd: 15.0,
                    ..under
                },
                limits
            )
            .is_some()
        );
        let mut config = Config::default();
        assert!(
            day_over(
                &config,
                Spent {
                    cost_usd: 20.0,
                    ..under
                }
            )
            .is_some()
        );
        config.agents.studio_daily_usd_cap = None;
        assert_eq!(
            day_over(
                &config,
                Spent {
                    cost_usd: 999.0,
                    ..under
                }
            ),
            None
        );
    }

    #[test]
    fn resuming_a_plan_gives_it_a_fresh_allowance_on_top_of_what_it_spent() {
        let dir = std::env::temp_dir().join(format!("axiomata-studiospend-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut db = crate::db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap();
        let board = crate::board::store::create_board(&mut db, "B").unwrap();
        let plan = crate::board::flow::create_plan(
            &db,
            board.id,
            &crate::board::PlanFields {
                name: "p".into(),
                goal: String::new(),
                project_id: None,
                auto_start_max: Some(64),
                max_cost_usd: Some(1.0),
                max_tokens: Some(1_000),
            },
        )
        .unwrap();
        record_look(
            &db,
            1,
            SOURCE_CLAUDE,
            Some(plan.id),
            usage(900, 300, 4),
            Some(1.2),
            Utc::now(),
        )
        .unwrap();
        let config = Config::default();
        let spent = plan_spent(&db, plan.id).unwrap();
        assert!(plan_over(spent, plan_limits(&config, &plan)).is_some());

        let resumed = resume_plan(&db, &config, plan.id).unwrap();
        assert_eq!(
            resumed.auto_start_max,
            Some(64),
            "nothing else about the plan changes"
        );
        assert_eq!(resumed.max_tokens, Some(1_200 + 6_000_000));
        assert!((resumed.max_cost_usd.unwrap() - 16.2).abs() < 1e-9);
        assert_eq!(plan_over(spent, plan_limits(&config, &resumed)), None);
        assert!(resume_plan(&db, &config, 999).is_err());
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_day_cap_stops_only_a_session_whose_dollars_were_metered_and_not_one_already_at_its_own_limit()
     {
        let day = Some("cap");
        assert!(stopped_by_day(false, Some(0.1), day));
        assert!(
            !stopped_by_day(true, Some(0.1), day),
            "its own limit speaks first"
        );
        assert!(
            !stopped_by_day(false, None, day),
            "subscription, unpriced or unreadable: nothing to save"
        );
        assert!(!stopped_by_day(false, Some(0.1), None));
    }

    #[test]
    fn a_plan_that_has_not_reached_a_limit_is_not_resumed_and_a_high_hand_set_limit_is_kept() {
        let dir =
            std::env::temp_dir().join(format!("axiomata-studiospend-keep-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut db = crate::db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap();
        let board = crate::board::store::create_board(&mut db, "B").unwrap();
        let plan = crate::board::flow::create_plan(
            &db,
            board.id,
            &crate::board::PlanFields {
                name: "p".into(),
                goal: String::new(),
                project_id: None,
                auto_start_max: Some(64),
                max_cost_usd: Some(100.0),
                max_tokens: Some(1_000),
            },
        )
        .unwrap();
        let config = Config::default();
        record_look(
            &db,
            1,
            SOURCE_CLAUDE,
            Some(plan.id),
            usage(900, 300, 4),
            Some(10.0),
            Utc::now(),
        )
        .unwrap();
        let resumed = resume_plan(&db, &config, plan.id).unwrap();
        assert_eq!(
            resumed.max_cost_usd,
            Some(100.0),
            "the hand-set $100 is not cut to spent + default"
        );
        assert_eq!(resumed.max_tokens, Some(1_200 + 6_000_000));
        assert!(
            resume_plan(&db, &config, plan.id).is_err(),
            "no longer at a limit: nothing to go on from"
        );
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
