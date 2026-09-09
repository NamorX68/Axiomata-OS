//! Cost accounting and the daily spend guardrail for paid model-routing
//! providers (provider-hardening checkpoints 4–5).
//!
//! Two record sources feed the rollup: the `runs` table (skill / routine
//! runs, `cost_usd` column added in migration 0005) and the `chat_turns`
//! table (dashboard-assistant turns, migration 0006). Both carry a `provider`
//! string and a timestamp; [`spent_usd_since`] sums `cost_usd` across the two
//! for one provider since a cutoff, and [`guard_redirected_turn`] refuses to
//! start a new turn once today's sum has reached `config.agents.daily_usd_cap`.
//!
//! The Anthropic provider is subscription-billed through the CLI's own login
//! and is never metered here — the guard is a no-op unless the active provider
//! [redirects the CLI](crate::config::ProviderId::is_redirected).

use chrono::{DateTime, Datelike, Local, TimeZone, Utc};
use rusqlite::Connection;

use crate::config::Config;
use crate::error::AxiomataError;

/// One dashboard-assistant turn, for the `chat_turns` spend log. Built by the
/// caller right after [`crate::agents::chat`] returns.
#[derive(Debug, Clone)]
pub struct ChatTurnRecord {
    /// `session_id` claude returned for this turn.
    pub session_id: String,
    /// `"chat"` or `"instruct"`.
    pub mode: &'static str,
    /// Active model-routing provider when the turn ran (`ProviderId::as_str`),
    /// or `None` if it could not be determined.
    pub provider: Option<String>,
    /// The resolved `claude --model`, or `None` for the CLI default.
    pub model: Option<String>,
    pub is_error: bool,
    pub cost_usd: Option<f64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub num_turns: Option<u32>,
    pub duration_ms: u64,
}

/// Appends one row to `chat_turns`. A logging failure must never sink an
/// otherwise-successful chat turn, so callers typically `warn!` and move on
/// rather than propagate this.
///
/// Errors:
///     [`AxiomataError::Database`] if the insert fails.
pub fn record_chat_turn(db: &Connection, turn: &ChatTurnRecord) -> Result<(), AxiomataError> {
    db.execute(
        "INSERT INTO chat_turns \
         (session_id, mode, provider, model, is_error, cost_usd, \
          input_tokens, output_tokens, num_turns, duration_ms, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            turn.session_id,
            turn.mode,
            turn.provider,
            turn.model,
            turn.is_error as i64,
            turn.cost_usd,
            turn.input_tokens.map(|n| n as i64),
            turn.output_tokens.map(|n| n as i64),
            turn.num_turns.map(|n| n as i64),
            turn.duration_ms as i64,
            Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(())
}

/// Sums recorded `cost_usd` for `provider` across both `runs` and
/// `chat_turns` for rows at or after `since`. Rows with a `NULL` cost (the
/// subscription path, Ollama, an unparsed envelope) contribute nothing.
///
/// Errors:
///     [`AxiomataError::Database`] if the query fails.
pub fn spent_usd_since(
    db: &Connection,
    provider: &str,
    since: DateTime<Utc>,
) -> Result<f64, AxiomataError> {
    let since = since.to_rfc3339();
    let total: f64 = db.query_row(
        "SELECT \
           COALESCE((SELECT SUM(cost_usd) FROM runs \
                     WHERE provider = ?1 AND started_at >= ?2), 0) \
         + COALESCE((SELECT SUM(cost_usd) FROM chat_turns \
                     WHERE provider = ?1 AND created_at >= ?2), 0)",
        rusqlite::params![provider, since],
        |row| row.get(0),
    )?;
    Ok(total)
}

/// Midnight (local time) opening `date`, as a UTC instant.
fn local_midnight_utc(date: chrono::NaiveDate, fallback: DateTime<Utc>) -> DateTime<Utc> {
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is always a valid time");
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map(|dt| dt.with_timezone(&Utc))
        // A DST "spring forward" gap at local midnight is not a real-world
        // instant; fall back rather than panic.
        .unwrap_or(fallback)
}

/// Start of the current local calendar day, as a UTC instant. The spend cap
/// is a "today" budget in the owner's own timezone, not a rolling 24h window.
pub fn start_of_local_day(now: DateTime<Utc>) -> DateTime<Utc> {
    local_midnight_utc(now.with_timezone(&Local).date_naive(), now)
}

/// Start of the current local calendar month, as a UTC instant.
pub fn start_of_local_month(now: DateTime<Utc>) -> DateTime<Utc> {
    let today = now.with_timezone(&Local).date_naive();
    local_midnight_utc(today.with_day(1).unwrap_or(today), now)
}

/// Today's spend (local day) for one provider.
pub fn spent_today_usd(
    db: &Connection,
    provider: &str,
    now: DateTime<Utc>,
) -> Result<f64, AxiomataError> {
    spent_usd_since(db, provider, start_of_local_day(now))
}

/// The spend rollup the Settings UI shows for one provider, plus which
/// role(s) route to it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SpendSummary {
    /// Provider token (`ProviderId::as_str`).
    pub provider: String,
    /// Which role(s) this provider serves: `"chat"`, `"skill"`, or
    /// `"chat & skill"` when both role selectors point at it.
    pub role: String,
    /// Spend so far in the current local day.
    pub today_usd: f64,
    /// Spend so far in the current local calendar month.
    pub month_usd: f64,
    /// The configured daily cap, if any.
    pub daily_cap_usd: Option<f64>,
    /// Whether this provider is metered at all (`false` for Anthropic).
    pub metered: bool,
}

/// [`role_spend_summaries`] as of now — the form UI callers want, so they
/// don't need a `chrono` dependency just to pass the current instant.
pub fn role_spend_summaries_now(
    db: &Connection,
    config: &Config,
) -> Result<Vec<SpendSummary>, AxiomataError> {
    role_spend_summaries(db, config, Utc::now())
}

/// One [`SpendSummary`] per **distinct** provider across the chat and skill
/// role selectors (a single entry, labelled `"chat & skill"`, when both point
/// at the same provider).
///
/// Errors:
///     [`AxiomataError::Database`] if a query fails.
pub fn role_spend_summaries(
    db: &Connection,
    config: &Config,
    now: DateTime<Utc>,
) -> Result<Vec<SpendSummary>, AxiomataError> {
    let chat = config.agents.chat_provider;
    let skill = config.agents.skill_provider;
    let mut summaries = vec![provider_summary(db, config, chat, "chat", now)?];
    if skill == chat {
        summaries[0].role = "chat & skill".to_string();
    } else {
        summaries.push(provider_summary(db, config, skill, "skill", now)?);
    }
    Ok(summaries)
}

/// Builds the [`SpendSummary`] for one provider.
///
/// Errors:
///     [`AxiomataError::Database`] if a query fails.
pub fn provider_summary(
    db: &Connection,
    config: &Config,
    provider: crate::config::ProviderId,
    role: &str,
    now: DateTime<Utc>,
) -> Result<SpendSummary, AxiomataError> {
    let token = provider.as_str();
    Ok(SpendSummary {
        provider: token.to_string(),
        role: role.to_string(),
        today_usd: spent_today_usd(db, token, now)?,
        month_usd: spent_usd_since(db, token, start_of_local_month(now))?,
        daily_cap_usd: config.agents.daily_usd_cap,
        metered: provider.is_redirected(),
    })
}

/// Refuses a new agent turn when today's spend through the paid provider
/// serving `role` has reached the daily cap. A no-op when that provider is
/// Anthropic (subscription-billed) or the cap is unset.
///
/// Call this immediately before dispatching any turn that could reach a
/// `claude -p` spawn: skill/routine runs (`ProviderRole::Skill`) and dashboard
/// chat/instruct turns (`ProviderRole::Chat`).
///
/// Errors:
///     [`AxiomataError::SpendCapReached`] when over the cap (the turn must not
///     be started); [`AxiomataError::Database`] if the spend query fails.
pub fn guard_redirected_turn(
    db: &Connection,
    config: &Config,
    role: crate::config::ProviderRole,
) -> Result<(), AxiomataError> {
    let provider = config.agents.provider_for(role);
    if !provider.is_redirected() {
        return Ok(());
    }
    let Some(cap) = config.agents.daily_usd_cap else {
        return Ok(());
    };
    let spent = spent_today_usd(db, provider.as_str(), Utc::now())?;
    if spent >= cap {
        return Err(AxiomataError::SpendCapReached {
            provider: provider.as_str().to_string(),
            cap_usd: cap,
            spent_usd: spent,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProviderId, ProviderRole};
    use chrono::Duration;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // The two tables the rollup reads, in their post-migration shape.
        conn.execute_batch(
            "CREATE TABLE runs (id INTEGER PRIMARY KEY, provider TEXT, cost_usd REAL, \
                                started_at TEXT NOT NULL);
             CREATE TABLE chat_turns (id INTEGER PRIMARY KEY, session_id TEXT, mode TEXT, \
                                provider TEXT, model TEXT, is_error INTEGER DEFAULT 0, \
                                cost_usd REAL, input_tokens INTEGER, output_tokens INTEGER, \
                                num_turns INTEGER, duration_ms INTEGER DEFAULT 0, \
                                created_at TEXT NOT NULL);",
        )
        .unwrap();
        conn
    }

    fn add_run(db: &Connection, provider: &str, cost: Option<f64>, at: DateTime<Utc>) {
        db.execute(
            "INSERT INTO runs (provider, cost_usd, started_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![provider, cost, at.to_rfc3339()],
        )
        .unwrap();
    }

    #[test]
    fn spent_usd_since_sums_both_tables_and_ignores_other_providers_and_null_costs() {
        let db = mem_db();
        let now = Utc::now();
        add_run(&db, "open_router", Some(0.40), now);
        add_run(&db, "open_router", Some(0.10), now);
        add_run(&db, "open_router", None, now); // subscription/Ollama row
        add_run(&db, "anthropic", Some(5.00), now); // different provider
        record_chat_turn(
            &db,
            &ChatTurnRecord {
                session_id: "s".into(),
                mode: "chat",
                provider: Some("open_router".into()),
                model: None,
                is_error: false,
                cost_usd: Some(0.25),
                input_tokens: None,
                output_tokens: None,
                num_turns: None,
                duration_ms: 10,
            },
        )
        .unwrap();

        let total = spent_usd_since(&db, "open_router", now - Duration::hours(1)).unwrap();
        assert!((total - 0.75).abs() < 1e-9, "got {total}");
    }

    #[test]
    fn spent_usd_since_excludes_rows_before_the_cutoff() {
        let db = mem_db();
        let now = Utc::now();
        add_run(&db, "open_router", Some(1.00), now - Duration::days(2));
        add_run(&db, "open_router", Some(0.20), now);
        let total = spent_usd_since(&db, "open_router", now - Duration::hours(1)).unwrap();
        assert!((total - 0.20).abs() < 1e-9, "got {total}");
    }

    #[test]
    fn guard_is_a_no_op_for_anthropic_regardless_of_spend() {
        let db = mem_db();
        add_run(&db, "anthropic", Some(999.0), Utc::now());
        let config = Config::default(); // active = Anthropic, cap = Some(2.0)
        assert!(guard_redirected_turn(&db, &config, ProviderRole::Skill).is_ok());
    }

    #[test]
    fn guard_blocks_a_redirected_turn_once_the_cap_is_reached() {
        let db = mem_db();
        add_run(&db, "open_router", Some(2.50), Utc::now());
        let mut config = Config::default();
        config.agents.chat_provider = ProviderId::OpenRouter;
        config.agents.skill_provider = ProviderId::OpenRouter;
        // cap defaults to $2.00
        let err = guard_redirected_turn(&db, &config, ProviderRole::Skill).unwrap_err();
        match err {
            AxiomataError::SpendCapReached {
                provider,
                cap_usd,
                spent_usd,
            } => {
                assert_eq!(provider, "open_router");
                assert!((cap_usd - 2.0).abs() < 1e-9);
                assert!(spent_usd >= 2.5 - 1e-9);
            }
            other => panic!("expected SpendCapReached, got {other:?}"),
        }
    }

    #[test]
    fn guard_allows_a_redirected_turn_under_the_cap_and_when_cap_is_disabled() {
        let db = mem_db();
        add_run(&db, "open_router", Some(1.00), Utc::now());
        let mut config = Config::default();
        config.agents.chat_provider = ProviderId::OpenRouter;
        config.agents.skill_provider = ProviderId::OpenRouter;
        assert!(
            guard_redirected_turn(&db, &config, ProviderRole::Skill).is_ok(),
            "under cap"
        );

        add_run(&db, "open_router", Some(50.0), Utc::now());
        config.agents.daily_usd_cap = None;
        assert!(
            guard_redirected_turn(&db, &config, ProviderRole::Skill).is_ok(),
            "cap disabled -> never blocks"
        );
    }
}
