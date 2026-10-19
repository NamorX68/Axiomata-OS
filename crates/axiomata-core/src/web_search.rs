//! The app's side of Glut's `web_search` (`docs/plans/glut-round.md`, W2/W3/W7): the providers' keys from
//! `config.toml` and a budget kept in the database — the searches of the local day against
//! `agents.web.daily_search_limit`, Tavily's of the local month against `tavily_monthly_limit`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axiomata_harness::tools::search::{
    BudgetVerdict, Provider, SearchBudget, SearchKeys, WebSearch,
};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

use crate::config::Config;
use crate::paths;
use crate::spend::{start_of_local_day, start_of_local_month};

/// How long a budget look waits for a database another process writes to.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The `web_search` tool for a run of the app, or `None` when the owner turned search off (a daily limit of 0).
pub fn web_search_tool(config: &Config) -> Option<WebSearch> {
    let web = &config.agents.web;
    if web.daily_search_limit == 0 {
        return None;
    }
    let key = |key: &Option<String>| key.clone().filter(|key| !key.trim().is_empty());
    let keys = SearchKeys {
        exa: key(&web.exa_api_key),
        tinyfish: key(&web.tinyfish_api_key),
        tavily: key(&web.tavily_api_key),
    };
    let budget = DbBudget {
        db: paths::db_path(),
        daily: web.daily_search_limit,
        tavily_monthly: web.tavily_monthly_limit,
    };
    Some(WebSearch::new(keys, Arc::new(budget)))
}

/// The limits, counted in the `web_searches` table. Each look opens its own connection: a search is rare, and the
/// harness's tools hold no connection of the app's.
struct DbBudget {
    db: PathBuf,
    daily: u32,
    tavily_monthly: u32,
}

impl DbBudget {
    fn open(&self) -> rusqlite::Result<Connection> {
        let conn = Connection::open(&self.db)?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        Ok(conn)
    }
}

impl SearchBudget for DbBudget {
    fn allow(&self, provider: Provider) -> BudgetVerdict {
        // A budget that cannot be read closes the search: the limits exist so no search costs unmeant money.
        let verdict = self
            .open()
            .and_then(|conn| verdict(&conn, provider, self.daily, self.tavily_monthly, Utc::now()));
        verdict.unwrap_or_else(|err| {
            BudgetVerdict::Closed(format!("the search limits cannot be read: {err}"))
        })
    }

    fn record(&self, provider: Provider) {
        if let Err(err) = self
            .open()
            .and_then(|conn| record(&conn, provider, Utc::now()))
        {
            tracing::warn!(%err, provider = provider.as_str(), "a web search could not be counted");
        }
    }
}

/// What the limits say about `provider` at `now`.
fn verdict(
    conn: &Connection,
    provider: Provider,
    daily: u32,
    tavily_monthly: u32,
    now: DateTime<Utc>,
) -> rusqlite::Result<BudgetVerdict> {
    let today = count_since(conn, None, start_of_local_day(now))?;
    if today >= u64::from(daily) {
        return Ok(BudgetVerdict::Closed(format!(
            "the daily web search limit ({daily}) is reached; it opens again tomorrow"
        )));
    }
    if provider == Provider::Tavily {
        let month = count_since(conn, Some(provider), start_of_local_month(now))?;
        if month >= u64::from(tavily_monthly) {
            return Ok(BudgetVerdict::Skip(format!(
                "its monthly limit ({tavily_monthly}) is reached"
            )));
        }
    }
    Ok(BudgetVerdict::Allow)
}

fn count_since(
    conn: &Connection,
    provider: Option<Provider>,
    since: DateTime<Utc>,
) -> rusqlite::Result<u64> {
    let since = since.to_rfc3339();
    match provider {
        None => conn.query_row(
            "SELECT COUNT(*) FROM web_searches WHERE searched_at >= ?1",
            params![since],
            |row| row.get(0),
        ),
        Some(provider) => conn.query_row(
            "SELECT COUNT(*) FROM web_searches WHERE searched_at >= ?1 AND provider = ?2",
            params![since, provider.as_str()],
            |row| row.get(0),
        ),
    }
}

fn record(conn: &Connection, provider: Provider, now: DateTime<Utc>) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO web_searches (provider, searched_at) VALUES (?1, ?2)",
        params![provider.as_str(), now.to_rfc3339()],
    )
    .map(|_| ())
}

/// Today's searches and Tavily's of this month, for the Settings dialog.
pub fn usage(db: &Path) -> rusqlite::Result<(u64, u64)> {
    let conn = Connection::open(db)?;
    let now = Utc::now();
    Ok((
        count_since(&conn, None, start_of_local_day(now))?,
        count_since(&conn, Some(Provider::Tavily), start_of_local_month(now))?,
    ))
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;
    use crate::test_support::unique_temp_dir;

    fn db() -> (PathBuf, Connection) {
        let dir = unique_temp_dir("axiomata-test-web-search");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("axiomata.db");
        let conn = crate::db::open_and_migrate_at(&path).unwrap();
        (path, conn)
    }

    #[test]
    fn the_daily_limit_closes_every_provider_and_tavilys_month_skips_only_tavily() {
        let (_path, conn) = db();
        let now = Utc::now();
        assert_eq!(
            verdict(&conn, Provider::Exa, 2, 1, now).unwrap(),
            BudgetVerdict::Allow
        );
        record(&conn, Provider::Tavily, now).unwrap();
        // One Tavily search this month: Tavily is used up, the others are not.
        assert!(matches!(
            verdict(&conn, Provider::Tavily, 2, 1, now).unwrap(),
            BudgetVerdict::Skip(_)
        ));
        assert_eq!(
            verdict(&conn, Provider::TinyFish, 2, 1, now).unwrap(),
            BudgetVerdict::Allow
        );
        record(&conn, Provider::Exa, now).unwrap();
        // Two searches today: the day is closed for every provider.
        let closed = verdict(&conn, Provider::Exa, 2, 1, now).unwrap();
        assert!(
            matches!(&closed, BudgetVerdict::Closed(reason) if reason.contains("(2)")),
            "{closed:?}"
        );
    }

    #[test]
    fn yesterdays_searches_do_not_count_today() {
        let (_path, conn) = db();
        let now = Utc::now();
        record(&conn, Provider::Exa, now - Duration::days(2)).unwrap();
        assert_eq!(
            verdict(&conn, Provider::Exa, 1, 1000, now).unwrap(),
            BudgetVerdict::Allow
        );
    }

    #[test]
    fn the_budget_counts_through_its_own_connection() {
        let (path, _conn) = db();
        let budget = DbBudget {
            db: path.clone(),
            daily: 1,
            tavily_monthly: 1000,
        };
        assert_eq!(budget.allow(Provider::Exa), BudgetVerdict::Allow);
        budget.record(Provider::Exa);
        assert!(matches!(
            budget.allow(Provider::TinyFish),
            BudgetVerdict::Closed(_)
        ));
        assert_eq!(usage(&path).unwrap(), (1, 0));
    }

    #[test]
    fn a_budget_that_cannot_be_read_closes_the_search() {
        let budget = DbBudget {
            db: PathBuf::from("/nonexistent/dir/axiomata.db"),
            daily: 50,
            tavily_monthly: 1000,
        };
        assert!(matches!(
            budget.allow(Provider::Exa),
            BudgetVerdict::Closed(_)
        ));
    }

    #[test]
    fn a_daily_limit_of_zero_offers_no_search() {
        let mut config = Config::default();
        assert!(web_search_tool(&config).is_some());
        config.agents.web.daily_search_limit = 0;
        assert!(web_search_tool(&config).is_none());
    }
}
