//! A plan that runs by itself (`docs/plans/a2a.md`, A4, A25, CP-A8): the owner approves it once, and its cards are
//! started, reviewed and integrated without a click each.
//!
//! One [`PlanRun::tick`] looks at every approved plan that runs by itself ([`runs_by_itself`]):
//!
//! 1. A card the reviewer signed off is **integrated** into the plan's line ([`integrate_card`]).
//! 2. A card that is **ready** — in an open column, and everything it builds on is on the line — is **started**, as many
//!    at once as the dependencies allow and the machine's cap ([`AgentDefaults::max_parallel_sessions`]) holds. The
//!    cards of a plan are independent unless a proposal said otherwise, so a plan without edges starts all of them.
//! 3. A plan whose cards are all on the line is reported **ready to take over**: the owner's one click.
//!
//! The reviewer needs no step here: [`card_watch`](../../../apps/axiomata/src-tauri/src/card_watch.rs) starts it for
//! every reported card.
//!
//! What a tick does not do is run a harness: a session's harness starts when its pane opens, which is the Studio's
//! job, told through the events this returns. A plan runs while the app and its Studio are open.
//!
//! A card that cannot be started or integrated (no engine for its role, work that changed after the review) is reported
//! once and left alone for a while ([`RETRY_AFTER`]), not retried every few seconds.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::AxiomataCore;
use crate::board::{Card, Plan, TaskState, store as board_store};
use crate::card_session::{
    CardIntegration, StartRequest, integrate_card, runs_by_itself, start_card_session,
};
use crate::config::Config;
use crate::studio_spend;

/// How long a card that could not be started or integrated is left alone before the next try.
const RETRY_AFTER: Duration = Duration::from_secs(300);

/// What a tick did, for the Studio to show and to act on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "event")]
pub enum RunEvent {
    /// A session was made for a ready card; its pane is to be opened (that starts the harness).
    Started {
        card_id: i64,
        plan_id: i64,
        project_id: i64,
        agent_id: i64,
    },
    /// A reviewed card was integrated (or put back, or failed): see [`CardIntegration`].
    Integrated(CardIntegration),
    /// A card could not be started or integrated; `reason` says why. Said once per card until it is tried again.
    Blocked { card_id: i64, reason: String },
    /// Every card of the plan is on its line: the owner can take the plan over.
    ReadyToTakeOver { plan_id: i64, name: String },
    /// The plan has spent what it may (A9, A27): it starts nothing more and asks. Said once; the owner's "go on"
    /// ([`crate::studio_spend::resume_plan`]) gives it a fresh allowance.
    Paused {
        plan_id: i64,
        name: String,
        reason: String,
    },
    /// The studio has spent its day's cap: no plan starts anything until tomorrow or until the cap is raised. Said once.
    DayCapReached { reason: String },
}

/// Which plans are held back by what they spent, and whether the whole day is.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pauses {
    /// Plan id → which of its limits it has reached.
    pub plans: HashMap<i64, String>,
    /// The day's cap, once it is reached.
    pub day: Option<String>,
}

/// What one tick is going to do, read under the lock.
#[derive(Debug, Default, PartialEq, Eq)]
struct Work {
    /// Reviewed cards to integrate, oldest first.
    integrate: Vec<i64>,
    /// Ready cards to start, with the project each runs in.
    start: Vec<(i64, i64, i64)>,
    /// Plans whose cards are all on the line.
    finished: Vec<(i64, String)>,
    /// Cards that wait for a card that failed or was called off, with what is to be said: the plan cannot go on until the
    /// owner removes the edge or redoes the card, and nothing else would tell them.
    stuck: Vec<(i64, String)>,
}

/// The memory of the tick: what it refused and when, and which plans it already called finished.
#[derive(Debug, Default)]
pub struct PlanRun {
    refused: HashMap<i64, Instant>,
    /// Cards the studio gave up on: left alone for good (until the app restarts, and then the card itself says so).
    given_up: HashSet<i64>,
    finished: HashSet<i64>,
    /// Cards already said to be stuck: once each.
    warned: HashSet<i64>,
    /// Plans already said to be paused; forgotten when they are not, so a second pause is said again.
    paused: HashSet<i64>,
    /// The day's cap was said to be reached.
    day_capped: bool,
}

fn live(state: TaskState) -> bool {
    matches!(
        state,
        TaskState::Working | TaskState::InputRequired | TaskState::InReview
    )
}

/// How many harnesses a card keeps running. A worker's session stays until its work is integrated (it is idle, but its
/// harness is a process), and a reviewer's likewise: a card in review has both, and a card that is signed off and waits
/// to be integrated still has both.
fn sessions_of(card: &Card) -> usize {
    match card.state {
        TaskState::Working | TaskState::InputRequired => 1,
        TaskState::InReview => 2,
        TaskState::Verified if card.integrated_at.is_none() => 2,
        _ => 0,
    }
}

/// Works out what is to be done, from the cards and plans as they are. Pure: no database, no clock.
///
/// `plans` are the plans of every board, `cards` all cards of every board; `cap` the most sessions at once.
fn plan_work(
    plans: &[Plan],
    cards: &[Card],
    cap: usize,
    skip: &HashSet<i64>,
    already_finished: &HashSet<i64>,
    already_warned: &HashSet<i64>,
    pauses: &Pauses,
) -> Work {
    let mut work = Work::default();
    // Every harness that runs counts against the machine's cap, across all plans.
    let mut running: usize = cards.iter().map(sessions_of).sum();
    let mut plans: Vec<&Plan> = plans.iter().filter(|plan| runs_by_itself(plan)).collect();
    plans.sort_by_key(|plan| plan.id);
    for plan in plans {
        let mine: Vec<&Card> = cards
            .iter()
            .filter(|card| card.plan_id == Some(plan.id) && card.archived_at.is_none())
            .collect();
        work.integrate.extend(
            mine.iter()
                .filter(|card| card.state == TaskState::Verified && !skip.contains(&card.id))
                .map(|card| card.id),
        );
        let integrated: HashSet<i64> = mine
            .iter()
            .filter(|card| card.integrated_at.is_some())
            .map(|card| card.id)
            .collect();
        let limit = plan.auto_start_max.map_or(usize::MAX, |n| n as usize);
        let mut in_plan = mine.iter().filter(|card| live(card.state)).count();
        let mut ready: Vec<&&Card> = mine
            .iter()
            .filter(|card| {
                card.state == TaskState::Ready
                    && !skip.contains(&card.id)
                    && card.depends_on.iter().all(|dep| integrated.contains(dep))
            })
            .collect();
        ready.sort_by(|a, b| a.position.total_cmp(&b.position).then(a.id.cmp(&b.id)));
        // A plan over its limit, or a day over its cap, starts nothing; what is integrated and finished goes on, it costs
        // nothing.
        let paused = pauses.day.is_some() || pauses.plans.contains_key(&plan.id);
        for card in ready {
            if paused || running >= cap || in_plan >= limit {
                break;
            }
            if let Some(project) = plan.project_id {
                work.start.push((card.id, plan.id, project));
                running += 1;
                in_plan += 1;
            }
        }
        for card in &mine {
            if !matches!(card.state, TaskState::Ready | TaskState::Blocked)
                || already_warned.contains(&card.id)
            {
                continue;
            }
            if let Some(gone) = card.depends_on.iter().find_map(|dep| {
                cards
                    .iter()
                    .find(|other| other.id == *dep)
                    .filter(|other| matches!(other.state, TaskState::Failed | TaskState::Canceled))
            }) {
                let what = if gone.state == TaskState::Failed {
                    "failed"
                } else {
                    "was called off"
                };
                work.stuck.push((
                    card.id,
                    format!(
                        "it builds on card #{}, which {what}; the plan waits until the dependency is removed or that \
                         card is done again",
                        gone.id
                    ),
                ));
            }
        }
        // Finished: there are cards, every one of them is on the line (or taken over) or was called off, and at least one
        // is on the line: a plan whose cards were all called off has nothing to take over.
        let settled = |card: &&Card| {
            card.integrated_at.is_some()
                || card.taken_over_at.is_some()
                || card.state == TaskState::Canceled
        };
        let all_done =
            mine.iter().all(settled) && mine.iter().any(|card| card.state != TaskState::Canceled);
        if all_done && !already_finished.contains(&plan.id) {
            work.finished.push((plan.id, plan.name.clone()));
        }
    }
    work
}

impl PlanRun {
    /// What one integration came to, as the memory of the tick and the event for the Studio. A card that failed to be
    /// integrated is left alone for a while; one the studio gave up on is left alone for good and said once.
    fn absorb(
        &mut self,
        card_id: i64,
        result: Result<CardIntegration, crate::AxiomataError>,
        now: Instant,
    ) -> Option<RunEvent> {
        match result {
            Ok(CardIntegration::Busy { .. }) => None,
            Ok(CardIntegration::LeftForOwner { .. }) => {
                self.given_up.insert(card_id);
                None
            }
            Ok(done) => {
                if matches!(done, CardIntegration::Conflict { gave_up: true, .. }) {
                    self.given_up.insert(card_id);
                }
                Some(RunEvent::Integrated(done))
            }
            Err(err) => {
                self.refused.insert(card_id, now);
                Some(RunEvent::Blocked {
                    card_id,
                    reason: err.to_string(),
                })
            }
        }
    }

    /// Says which plan, or the day, has just been held back — once each — and forgets the ones that are free again.
    fn announce_pauses(&mut self, pauses: &Pauses, names: &HashMap<i64, String>) -> Vec<RunEvent> {
        self.paused.retain(|id| pauses.plans.contains_key(id));
        let mut events = Vec::new();
        let mut ids: Vec<&i64> = pauses.plans.keys().collect();
        ids.sort();
        for id in ids {
            if self.paused.insert(*id) {
                events.push(RunEvent::Paused {
                    plan_id: *id,
                    name: names.get(id).cloned().unwrap_or_default(),
                    reason: pauses.plans[id].clone(),
                });
            }
        }
        match (&pauses.day, self.day_capped) {
            (Some(reason), false) => {
                self.day_capped = true;
                events.push(RunEvent::DayCapReached {
                    reason: reason.clone(),
                });
            }
            (None, true) => self.day_capped = false,
            _ => {}
        }
        events
    }

    /// One look at every plan that runs by itself. See the module documentation.
    pub async fn tick(&mut self, core: &AxiomataCore) -> Vec<RunEvent> {
        let now = Instant::now();
        self.refused
            .retain(|_, at| now.duration_since(*at) < RETRY_AFTER);
        let skip: HashSet<i64> = self
            .refused
            .keys()
            .chain(self.given_up.iter())
            .copied()
            .collect();
        let (work, pauses, names) = match tokio::task::block_in_place(|| {
            read_work(core, &skip, &self.finished, &self.warned)
        }) {
            Ok(read) => read,
            Err(err) => {
                tracing::warn!(%err, "could not look at the plans that run by themselves");
                return Vec::new();
            }
        };
        let mut events = self.announce_pauses(&pauses, &names);
        for card_id in work.integrate {
            let result = integrate_card(core, card_id).await;
            events.extend(self.absorb(card_id, result, now));
        }
        for (card_id, plan_id, project_id) in work.start {
            let request = StartRequest {
                card_id,
                project_id,
                engine_id: None,
            };
            match tokio::task::block_in_place(|| start_card_session(core, &request)) {
                Ok(session) => events.push(RunEvent::Started {
                    card_id,
                    plan_id,
                    project_id,
                    agent_id: session.agent.id,
                }),
                Err(err) => {
                    self.refused.insert(card_id, now);
                    events.push(RunEvent::Blocked {
                        card_id,
                        reason: err.to_string(),
                    });
                }
            }
        }
        for (card_id, reason) in work.stuck {
            self.warned.insert(card_id);
            events.push(RunEvent::Blocked { card_id, reason });
        }
        for (plan_id, name) in work.finished {
            self.finished.insert(plan_id);
            events.push(RunEvent::ReadyToTakeOver { plan_id, name });
        }
        events
    }
}

/// What a tick is going to do, which plans and the day are held back by their spending, and the plans' names.
type Read = (Work, Pauses, HashMap<i64, String>);

fn read_work(
    core: &AxiomataCore,
    skip: &HashSet<i64>,
    finished: &HashSet<i64>,
    warned: &HashSet<i64>,
) -> Result<Read, crate::AxiomataError> {
    let config: Config = core.config_read().clone();
    let db = core.db_lock();
    let mut plans = Vec::new();
    let mut cards = Vec::new();
    for board in board_store::list_boards(&db)? {
        plans.extend(crate::board::flow::list_plans(&db, board.id)?);
        cards.extend(board_store::list_cards(&db, board.id, false)?);
    }
    let cap = config.agents.max_parallel_sessions.max(1) as usize;
    let mut pauses = Pauses {
        day: studio_spend::day_over(&config, studio_spend::spent_today(&db, chrono::Utc::now())?),
        ..Pauses::default()
    };
    for plan in plans.iter().filter(|plan| runs_by_itself(plan)) {
        // A plan with nothing left to start is not "paused": it only waits for the owner's take-over.
        let unsettled = cards.iter().any(|card| {
            card.plan_id == Some(plan.id)
                && card.archived_at.is_none()
                && card.integrated_at.is_none()
                && card.taken_over_at.is_none()
                && card.state != TaskState::Canceled
        });
        if !unsettled {
            continue;
        }
        let spent = studio_spend::plan_spent(&db, plan.id)?;
        if let Some(reason) =
            studio_spend::plan_over(spent, studio_spend::plan_limits(&config, plan))
        {
            pauses.plans.insert(plan.id, reason);
        }
    }
    let names = plans
        .iter()
        .map(|plan| (plan.id, plan.name.clone()))
        .collect();
    let work = plan_work(&plans, &cards, cap, skip, finished, warned, &pauses);
    Ok((work, pauses, names))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::PlanStatus;

    fn plan(id: i64, auto: Option<u32>) -> Plan {
        Plan {
            id,
            board_id: 1,
            name: format!("plan {id}"),
            goal: String::new(),
            project_id: Some(9),
            base_branch: None,
            line_tip: None,
            status: PlanStatus::Approved,
            auto_start_max: auto,
            max_cost_usd: None,
            max_tokens: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            approved_at: None,
        }
    }

    fn card(id: i64, plan: i64, state: TaskState) -> Card {
        let mut card: Card = serde_json::from_value(serde_json::json!({
            "id": id, "board_id": 1, "column_id": 1, "position": id as f64, "title": format!("c{id}"),
            "body": "", "labels": [], "assignee": null, "claimed_by": null, "claimed_at": null,
            "verified_by": null, "verified_at": null, "due_at": null, "archived_at": null,
            "created_at": "2026-10-05T10:00:00Z", "updated_at": "2026-10-05T10:00:00Z", "plan_id": plan
        }))
        .unwrap();
        card.state = state;
        card
    }

    fn integrated(mut card: Card) -> Card {
        card.integrated_at = Some(chrono::Utc::now());
        card.state = TaskState::Integrated;
        card
    }

    fn none() -> HashSet<i64> {
        HashSet::new()
    }

    #[test]
    fn independent_cards_all_start_at_once_up_to_the_machines_cap() {
        let plans = [plan(1, Some(64))];
        let cards: Vec<Card> = (1..=5).map(|id| card(id, 1, TaskState::Ready)).collect();
        let work = plan_work(
            &plans,
            &cards,
            3,
            &none(),
            &none(),
            &none(),
            &Pauses::default(),
        );
        assert_eq!(
            work.start.iter().map(|s| s.0).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        let wide = plan_work(
            &plans,
            &cards,
            10,
            &none(),
            &none(),
            &none(),
            &Pauses::default(),
        );
        assert_eq!(wide.start.len(), 5, "as many as the dependencies allow");
    }

    #[test]
    fn a_card_that_builds_on_another_waits_until_that_ones_work_is_on_the_line() {
        let plans = [plan(1, Some(64))];
        let mut third = card(3, 1, TaskState::Ready);
        third.depends_on = vec![1, 2];
        let cards = vec![
            integrated(card(1, 1, TaskState::Verified)),
            card(2, 1, TaskState::Verified),
            third.clone(),
        ];
        // The second is signed off but not integrated: the third is not started, the second is integrated.
        let work = plan_work(
            &plans,
            &cards,
            9,
            &none(),
            &none(),
            &none(),
            &Pauses::default(),
        );
        assert_eq!(work.integrate, [2]);
        assert!(work.start.is_empty());

        let cards = vec![
            integrated(card(1, 1, TaskState::Verified)),
            integrated(card(2, 1, TaskState::Verified)),
            third,
        ];
        let work = plan_work(
            &plans,
            &cards,
            9,
            &none(),
            &none(),
            &none(),
            &Pauses::default(),
        );
        assert_eq!(work.start.iter().map(|s| s.0).collect::<Vec<_>>(), [3]);
    }

    #[test]
    fn running_cards_count_against_the_cap_across_plans_and_a_plans_own_limit() {
        let plans = [plan(1, Some(1)), plan(2, Some(64))];
        let cards = vec![
            card(1, 1, TaskState::Working),
            card(2, 1, TaskState::Ready),
            card(3, 2, TaskState::InReview),
            card(4, 2, TaskState::Ready),
            card(5, 2, TaskState::Ready),
        ];
        // Plan 1 is at its own limit of one. A card in review has a worker and a reviewer running: that is three of the
        // machine's four harnesses, so one more may start, from plan 2.
        let work = plan_work(
            &plans,
            &cards,
            4,
            &none(),
            &none(),
            &none(),
            &Pauses::default(),
        );
        assert_eq!(work.start.iter().map(|s| s.0).collect::<Vec<_>>(), [4]);
        let full = plan_work(
            &plans,
            &cards,
            3,
            &none(),
            &none(),
            &none(),
            &Pauses::default(),
        );
        assert!(full.start.is_empty());
    }

    #[test]
    fn a_plan_that_does_not_run_by_itself_is_left_alone() {
        let manual = plan(1, None);
        let mut draft = plan(2, Some(64));
        draft.status = PlanStatus::Draft;
        let mut no_project = plan(3, Some(64));
        no_project.project_id = None;
        let cards = vec![
            card(1, 1, TaskState::Ready),
            card(2, 2, TaskState::Ready),
            card(3, 3, TaskState::Ready),
            card(4, 1, TaskState::Verified),
        ];
        let work = plan_work(
            &[manual, draft, no_project],
            &cards,
            9,
            &none(),
            &none(),
            &none(),
            &Pauses::default(),
        );
        assert_eq!(work, Work::default());
    }

    #[test]
    fn a_refused_card_is_skipped_and_a_finished_plan_is_reported_once() {
        let plans = [plan(1, Some(64)), plan(2, Some(64))];
        let cards = vec![
            card(1, 1, TaskState::Ready),
            card(2, 1, TaskState::Ready),
            integrated(card(3, 2, TaskState::Verified)),
        ];
        let skip = HashSet::from([1]);
        let work = plan_work(
            &plans,
            &cards,
            9,
            &skip,
            &none(),
            &none(),
            &Pauses::default(),
        );
        assert_eq!(work.start.iter().map(|s| s.0).collect::<Vec<_>>(), [2]);
        assert_eq!(work.finished, [(2, "plan 2".to_owned())]);
        let again = plan_work(
            &plans,
            &cards,
            9,
            &skip,
            &HashSet::from([2]),
            &none(),
            &Pauses::default(),
        );
        assert!(again.finished.is_empty());
    }

    #[test]
    fn the_cards_of_a_plan_start_in_board_order() {
        let plans = [plan(1, Some(64))];
        let mut first = card(7, 1, TaskState::Ready);
        first.position = 1.0;
        let mut second = card(2, 1, TaskState::Ready);
        second.position = 2.0;
        let work = plan_work(
            &plans,
            &[second, first],
            1,
            &none(),
            &none(),
            &none(),
            &Pauses::default(),
        );
        assert_eq!(work.start.iter().map(|s| s.0).collect::<Vec<_>>(), [7]);
    }
    #[test]
    fn a_signed_off_card_that_waits_to_be_integrated_still_holds_its_two_harnesses() {
        let plans = [plan(1, Some(64))];
        let cards = vec![
            card(1, 1, TaskState::Verified),
            card(2, 1, TaskState::Ready),
        ];
        assert!(
            plan_work(
                &plans,
                &cards,
                2,
                &none(),
                &none(),
                &none(),
                &Pauses::default()
            )
            .start
            .is_empty()
        );
        assert_eq!(
            plan_work(
                &plans,
                &cards,
                3,
                &none(),
                &none(),
                &none(),
                &Pauses::default()
            )
            .start
            .len(),
            1
        );
        // Once it is on the line its sessions are gone.
        let done = vec![
            integrated(card(1, 1, TaskState::Verified)),
            card(2, 1, TaskState::Ready),
        ];
        assert_eq!(
            plan_work(
                &plans,
                &done,
                1,
                &none(),
                &none(),
                &none(),
                &Pauses::default()
            )
            .start
            .len(),
            1
        );
    }

    fn conflict(gave_up: bool) -> CardIntegration {
        CardIntegration::Conflict {
            card_id: 5,
            plan_id: 1,
            files: vec!["a.txt".into()],
            gave_up,
            agent_ids: Vec::new(),
        }
    }

    #[test]
    fn a_card_the_studio_gave_up_on_is_said_once_and_then_left_alone_for_good() {
        let mut run = PlanRun::default();
        let now = Instant::now();
        assert!(matches!(
            run.absorb(5, Ok(conflict(true)), now),
            Some(RunEvent::Integrated(CardIntegration::Conflict {
                gave_up: true,
                ..
            }))
        ));
        assert!(run.given_up.contains(&5));
        // Seen again (after an app restart the card itself says it is left for the owner): nothing to say.
        assert!(
            run.absorb(5, Ok(CardIntegration::LeftForOwner { card_id: 5 }), now)
                .is_none()
        );
        // And it is skipped by the next look for as long as the app runs, however long that is.
        let cards = vec![card(5, 1, TaskState::Verified)];
        let skip: HashSet<i64> = run.given_up.iter().copied().collect();
        assert!(
            plan_work(
                &[plan(1, Some(64))],
                &cards,
                9,
                &skip,
                &none(),
                &none(),
                &Pauses::default()
            )
            .integrate
            .is_empty()
        );
    }

    #[test]
    fn a_card_that_is_put_back_is_not_given_up_a_busy_worker_is_tried_again_and_a_failure_is_said_and_waited_out()
     {
        let mut run = PlanRun::default();
        let now = Instant::now();
        assert!(run.absorb(5, Ok(conflict(false)), now).is_some());
        assert!(!run.given_up.contains(&5) && !run.refused.contains_key(&5));
        assert!(
            run.absorb(5, Ok(CardIntegration::Busy { card_id: 5 }), now)
                .is_none()
        );
        assert!(run.refused.is_empty());

        let blocked = run
            .absorb(
                5,
                Err(crate::roster::refusal("card", "no engine".into())),
                now,
            )
            .unwrap();
        assert!(matches!(blocked, RunEvent::Blocked { card_id: 5, .. }));
        assert!(run.refused.contains_key(&5));
    }

    #[test]
    fn the_events_reach_the_frontend_in_the_shape_it_reads() {
        let started = serde_json::to_value(RunEvent::Started {
            card_id: 1,
            plan_id: 2,
            project_id: 3,
            agent_id: 4,
        })
        .unwrap();
        assert_eq!(started["event"], "started");
        assert_eq!(started["agent_id"], 4);
        let integrated = serde_json::to_value(RunEvent::Integrated(CardIntegration::Done {
            card_id: 1,
            plan_id: 2,
            project_id: 3,
            commit: Some("abc".into()),
            agent_ids: vec![7],
        }))
        .unwrap();
        // The card integration is flattened into the event: `event` says what happened, `outcome` how it came out.
        assert_eq!(integrated["event"], "integrated");
        assert_eq!(integrated["outcome"], "done");
        assert_eq!(integrated["agent_ids"], serde_json::json!([7]));
        let conflict = serde_json::to_value(RunEvent::Integrated(conflict(true))).unwrap();
        assert_eq!(
            (conflict["outcome"].as_str(), conflict["gave_up"].as_bool()),
            (Some("conflict"), Some(true))
        );
        assert_eq!(
            serde_json::to_value(RunEvent::ReadyToTakeOver {
                plan_id: 1,
                name: "n".into()
            })
            .unwrap()["event"],
            "ready_to_take_over"
        );
    }

    #[test]
    fn a_card_that_builds_on_a_failed_or_called_off_card_is_said_to_be_stuck_once() {
        let plans = [plan(1, Some(64))];
        let mut next = card(2, 1, TaskState::Blocked);
        next.depends_on = vec![1];
        let cards = [card(1, 1, TaskState::Failed), next];
        let work = plan_work(
            &plans,
            &cards,
            9,
            &none(),
            &none(),
            &none(),
            &Pauses::default(),
        );
        assert_eq!(work.stuck.len(), 1);
        assert_eq!(work.stuck[0].0, 2);
        assert!(work.stuck[0].1.contains("#1") && work.stuck[0].1.contains("failed"));
        assert!(work.start.is_empty());
        let again = plan_work(
            &plans,
            &cards,
            9,
            &none(),
            &none(),
            &HashSet::from([2]),
            &Pauses::default(),
        );
        assert!(again.stuck.is_empty());
    }

    #[test]
    fn a_plan_with_a_called_off_card_is_finished_when_the_rest_is_on_the_line_and_not_when_all_were_called_off()
     {
        let plans = [plan(1, Some(64))];
        let cards = [
            integrated(card(1, 1, TaskState::Verified)),
            card(2, 1, TaskState::Canceled),
        ];
        let work = plan_work(
            &plans,
            &cards,
            9,
            &none(),
            &none(),
            &none(),
            &Pauses::default(),
        );
        assert_eq!(work.finished.len(), 1);
        let nothing = [card(1, 1, TaskState::Canceled)];
        assert!(
            plan_work(
                &plans,
                &nothing,
                9,
                &none(),
                &none(),
                &none(),
                &Pauses::default()
            )
            .finished
            .is_empty()
        );
    }

    #[test]
    fn a_plan_over_its_limit_or_a_day_over_its_cap_starts_nothing_but_still_integrates() {
        let plans = [plan(1, Some(64)), plan(2, Some(64))];
        let cards = vec![
            card(1, 1, TaskState::Ready),
            card(2, 1, TaskState::Verified),
            card(3, 2, TaskState::Ready),
        ];
        let one_paused = Pauses {
            plans: HashMap::from([(1, "6000000 tokens of 6000000 used".to_owned())]),
            day: None,
        };
        let work = plan_work(&plans, &cards, 9, &none(), &none(), &none(), &one_paused);
        assert_eq!(work.start.iter().map(|s| s.0).collect::<Vec<_>>(), [3]);
        assert_eq!(work.integrate, [2], "integrating costs nothing");
        let day = Pauses {
            plans: HashMap::new(),
            day: Some("$20.00 of the day's $20.00 for studio sessions spent".to_owned()),
        };
        let work = plan_work(&plans, &cards, 9, &none(), &none(), &none(), &day);
        assert!(work.start.is_empty());
        assert_eq!(work.integrate, [2]);
    }

    #[test]
    fn a_pause_is_said_once_and_again_after_the_plan_was_free_in_between() {
        let mut run = PlanRun::default();
        let names = HashMap::from([(1, "Plan".to_owned())]);
        let paused = Pauses {
            plans: HashMap::from([(1, "reason".to_owned())]),
            day: Some("cap".to_owned()),
        };
        assert_eq!(run.announce_pauses(&paused, &names).len(), 2);
        assert!(run.announce_pauses(&paused, &names).is_empty());
        assert!(run.announce_pauses(&Pauses::default(), &names).is_empty());
        assert_eq!(run.announce_pauses(&paused, &names).len(), 2);
    }
}
