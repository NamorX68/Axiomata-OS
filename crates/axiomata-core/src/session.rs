//! Who is acting in this shell.
//!
//! A Studio agent is started with its identity in the environment (`AXIOMATA_AGENT_ID`, `AXIOMATA_AGENT_NAME`, see
//! `axiomata_ide::Agent::resolve_env`). A command run from such a shell acts as **that agent**, whatever it types after
//! `--actor`: otherwise the two-party rule and the owner's gates would only stop an agent that forgot to lie.
//!
//! This is a guard against mistakes and against agents that follow their instructions, not a sandbox: a process of
//! the same user can unset the variables or open the database directly. Real separation needs the per-session
//! scoping of CP-A5; this is what can be had cheaply before it, and the one rule it already enforces — the owner's
//! gates are not reachable with an agent identity — is the one the later layers build on.

use std::env;

/// The actor of the current shell if it belongs to an agent session, else `None` (the owner's own terminal).
pub fn session_actor() -> Option<String> {
    actor_from(
        env::var("AXIOMATA_AGENT_ID").ok().as_deref(),
        env::var("AXIOMATA_AGENT_NAME").ok().as_deref(),
    )
}

/// [`session_actor`] from explicit values, for tests. The id makes the actor unique per agent (two agents may share a
/// name in different projects); the name is only there so a history line reads like a person wrote it.
pub fn actor_from(id: Option<&str>, name: Option<&str>) -> Option<String> {
    let id = id?.trim();
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut slug = String::new();
    for ch in name.unwrap_or("").chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    Some(if slug.is_empty() {
        format!("agent:agent-{id}")
    } else {
        format!("agent:{slug}-{id}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agent_session_acts_as_its_agent_with_a_name_a_person_can_read() {
        assert_eq!(
            actor_from(Some("7"), Some("Builder")).as_deref(),
            Some("agent:builder-7")
        );
        assert_eq!(
            actor_from(Some("12"), Some("  My  Review/Bot! ")).as_deref(),
            Some("agent:my-review-bot-12")
        );
        assert_eq!(
            actor_from(Some("3"), Some("")).as_deref(),
            Some("agent:agent-3")
        );
        assert_eq!(
            actor_from(Some("3"), None).as_deref(),
            Some("agent:agent-3")
        );
    }

    #[test]
    fn without_a_valid_id_it_is_the_owners_terminal() {
        assert_eq!(actor_from(None, Some("Builder")), None);
        assert_eq!(actor_from(Some(""), Some("Builder")), None);
        assert_eq!(actor_from(Some("x7"), Some("Builder")), None);
        assert_eq!(actor_from(Some("-1"), None), None);
    }

    #[test]
    fn every_derived_actor_is_a_valid_board_actor() {
        for name in ["Builder", "ä ö ü", "A_B", "x.y", "名前"] {
            let actor = actor_from(Some("1"), Some(name)).unwrap();
            let (kind, rest) = actor.split_once(':').unwrap();
            assert_eq!(kind, "agent");
            assert!(
                rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "{actor}"
            );
        }
    }
}
