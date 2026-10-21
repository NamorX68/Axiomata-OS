//! The app's side of Glut's `web_search` (`docs/plans/glut-round.md`, W2/W3/W7): the providers' keys from
//! `config.toml` and a budget kept in the database — the searches of the local day against
//! `agents.web.daily_search_limit`, Tavily's of the local month against `tavily_monthly_limit`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use glut::tools::search::{BudgetVerdict, Provider, SearchBudget, SearchKeys, Ticket, WebSearch};
use rusqlite::{Connection, TransactionBehavior, params};

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
    fn reserve(&self, provider: Provider) -> BudgetVerdict {
        // A budget that cannot be read or written closes the search: the limits exist so no search costs unmeant
        // money, and a search that could not be counted would be a free one.
        let verdict = self.open().and_then(|mut conn| {
            reserve(
                &mut conn,
                provider,
                self.daily,
                self.tavily_monthly,
                Utc::now(),
            )
        });
        verdict.unwrap_or_else(|err| {
            BudgetVerdict::Closed(format!("the search limits cannot be read: {err}"))
        })
    }

    fn release(&self, ticket: Ticket) {
        if let Err(err) = self.open().and_then(|conn| release(&conn, ticket)) {
            // Left counted: a search too many in the count is the safe side.
            tracing::warn!(%err, "a free web search could not be given back");
        }
    }
}

/// [`refusal`] and, when there is none, the search, its row — in one immediate transaction, so two runs cannot both see
/// the day's last search free.
fn reserve(
    conn: &mut Connection,
    provider: Provider,
    daily: u32,
    tavily_monthly: u32,
    now: DateTime<Utc>,
) -> rusqlite::Result<BudgetVerdict> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let verdict = match refusal(&tx, provider, daily, tavily_monthly, now)? {
        Some(refused) => refused,
        None => BudgetVerdict::Allow(Ticket(record(&tx, provider, now)?)),
    };
    tx.commit()?;
    Ok(verdict)
}

/// Removes the search `ticket` counted.
fn release(conn: &Connection, ticket: Ticket) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM web_searches WHERE id = ?1", params![ticket.0])
        .map(|_| ())
}

/// Why the limits refuse a search with `provider` at `now` ([`BudgetVerdict::Closed`] or [`BudgetVerdict::Skip`]), or
/// `None` when they allow it.
fn refusal(
    conn: &Connection,
    provider: Provider,
    daily: u32,
    tavily_monthly: u32,
    now: DateTime<Utc>,
) -> rusqlite::Result<Option<BudgetVerdict>> {
    let today = count_since(conn, None, start_of_local_day(now))?;
    if today >= u64::from(daily) {
        return Ok(Some(BudgetVerdict::Closed(format!(
            "the daily web search limit ({daily}) is reached; it opens again tomorrow"
        ))));
    }
    if provider == Provider::Tavily {
        let month = count_since(conn, Some(provider), start_of_local_month(now))?;
        if month >= u64::from(tavily_monthly) {
            return Ok(Some(BudgetVerdict::Skip(format!(
                "its monthly limit ({tavily_monthly}) is reached"
            ))));
        }
    }
    Ok(None)
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

/// Counts one search; its row's id.
fn record(conn: &Connection, provider: Provider, now: DateTime<Utc>) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO web_searches (provider, searched_at) VALUES (?1, ?2)",
        params![provider.as_str(), now.to_rfc3339()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Today's searches and Tavily's of this month, for the Settings dialog.
pub fn usage(db: &Path) -> rusqlite::Result<(u64, u64)> {
    let conn = Connection::open(db)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
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
        assert_eq!(refusal(&conn, Provider::Exa, 2, 1, now).unwrap(), None);
        record(&conn, Provider::Tavily, now).unwrap();
        // One Tavily search this month: Tavily is used up, the others are not.
        assert!(matches!(
            refusal(&conn, Provider::Tavily, 2, 1, now).unwrap(),
            Some(BudgetVerdict::Skip(_))
        ));
        assert_eq!(refusal(&conn, Provider::TinyFish, 2, 1, now).unwrap(), None);
        record(&conn, Provider::Exa, now).unwrap();
        // Two searches today: the day is closed for every provider.
        let closed = refusal(&conn, Provider::Exa, 2, 1, now).unwrap();
        assert!(
            matches!(&closed, Some(BudgetVerdict::Closed(reason)) if reason.contains("(2)")),
            "{closed:?}"
        );
    }

    /// The day and the month start at local midnight: a search a second before belongs to the one before.
    #[test]
    fn the_day_and_the_month_begin_at_local_midnight() {
        let (_path, conn) = db();
        let now = Utc::now();
        let day = start_of_local_day(now);
        record(&conn, Provider::Exa, day - Duration::seconds(1)).unwrap();
        assert_eq!(count_since(&conn, None, day).unwrap(), 0);
        record(&conn, Provider::Exa, day).unwrap();
        assert_eq!(count_since(&conn, None, day).unwrap(), 1);

        let month = start_of_local_month(now);
        record(&conn, Provider::Tavily, month - Duration::seconds(1)).unwrap();
        // Last month's Tavily search and today's Exa ones leave Tavily's month at 0.
        assert_eq!(
            count_since(&conn, Some(Provider::Tavily), month).unwrap(),
            0
        );
        assert_eq!(refusal(&conn, Provider::Tavily, 50, 1, now).unwrap(), None);
        record(&conn, Provider::Tavily, month).unwrap();
        assert!(matches!(
            refusal(&conn, Provider::Tavily, 50, 1, now).unwrap(),
            Some(BudgetVerdict::Skip(_))
        ));
    }

    #[test]
    fn the_budget_counts_when_it_allows_and_gives_back_a_ticket() {
        let (path, _conn) = db();
        let budget = DbBudget {
            db: path.clone(),
            daily: 1,
            tavily_monthly: 1000,
        };
        let BudgetVerdict::Allow(ticket) = budget.reserve(Provider::Exa) else {
            panic!("the first search is allowed");
        };
        assert_eq!(usage(&path).unwrap(), (1, 0));
        assert!(matches!(
            budget.reserve(Provider::TinyFish),
            BudgetVerdict::Closed(_)
        ));
        budget.release(ticket);
        assert_eq!(usage(&path).unwrap(), (0, 0));
        assert!(matches!(
            budget.reserve(Provider::TinyFish),
            BudgetVerdict::Allow(_)
        ));
    }

    #[test]
    fn a_budget_that_cannot_be_read_closes_the_search() {
        let budget = DbBudget {
            db: PathBuf::from("/nonexistent/dir/axiomata.db"),
            daily: 50,
            tavily_monthly: 1000,
        };
        assert!(matches!(
            budget.reserve(Provider::Exa),
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
