//! Agents, computed (`docs/plans/agents-model.md`, Q7, Q8): an agent is a role at a tier on an engine — never stored,
//! only listed from the roles and the engines of each tier ([`crate::engines::engine_for`]).
//!
//! Its name is "‹Harness› ‹Tier› ‹Role› (‹Model›)", e.g. "Glut Medium Allrounder (Sonnet 5.5)", shown in two lines:
//! "Allrounder · Medium" over "Glut · Sonnet 5.5". A session appends its `#n` elsewhere; no card number goes in.

use axiomata_roster::{Harness, Role, Tier};
use serde::Serialize;

use crate::config::Config;
use crate::engines::{self, EngineInfo};
use crate::model_tiers::ModelSources;

/// One agent the owner can start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentChoice {
    pub role: String,
    pub tier: Tier,
    pub engine_id: String,
    /// Whether this engine is the tier's first choice — the owner's default, else the cheapest.
    pub standard: bool,
    /// "Glut Medium Allrounder (Sonnet 5.5)".
    pub name: String,
    /// "Allrounder · Medium".
    pub title: String,
    /// "Glut · Sonnet 5.5".
    pub subtitle: String,
}

/// The harness as an agent's name says it: "Claude" for Claude Code.
pub fn harness_word(harness: Harness) -> &'static str {
    match harness {
        Harness::ClaudeCode => "Claude",
        Harness::Glut => "Glut",
        Harness::Opencode => "Opencode",
    }
}

/// A role's name as a title: each word of the slug capitalised, `-`/`_` as spaces — "code-reviewer" → "Code Reviewer".
pub fn role_label(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The model of `engine` as an agent's name shows it: the catalog's short name, else the model id without its
/// provider prefix, else "default" (the harness picks).
pub fn model_label(engine: &EngineInfo) -> String {
    if let Some(name) = engine.model_name.as_deref().filter(|name| !name.is_empty()) {
        return name.to_owned();
    }
    match engine.engine.model.as_deref().map(str::trim) {
        Some(model) if !model.is_empty() => crate::model_tiers::parse_model_key(model)
            .map_or(model, |(_, id)| id)
            .to_owned(),
        _ => "default".to_owned(),
    }
}

/// "‹Harness› ‹Tier› ‹Role› (‹Model›)".
pub fn agent_name(harness: Harness, tier: Tier, role: &str, model: &str) -> String {
    format!(
        "{} {} {} ({model})",
        harness_word(harness),
        tier.label(),
        role_label(role)
    )
}

/// The two lines an agent is shown in: "Role · Tier" and "Harness · Model".
pub fn agent_lines(harness: Harness, tier: Tier, role: &str, model: &str) -> (String, String) {
    (
        format!("{} · {}", role_label(role), tier.label()),
        format!("{} · {model}", harness_word(harness)),
    )
}

/// Every agent of `roles` × tier × the tier's engines, given each tier's engines in order ([`engines::engine_for`]).
/// Roles keep their order, tiers go light → heavy, engines keep the tier's order with the first marked standard.
pub fn choices_from(roles: &[Role], by_tier: &[(Tier, Vec<EngineInfo>)]) -> Vec<AgentChoice> {
    let mut out = Vec::new();
    for role in roles {
        for (tier, engines) in by_tier {
            for (at, engine) in engines.iter().enumerate() {
                let model = model_label(engine);
                let harness = engine.engine.harness;
                let (title, subtitle) = agent_lines(harness, *tier, &role.name, &model);
                out.push(AgentChoice {
                    role: role.name.clone(),
                    tier: *tier,
                    engine_id: engine.engine.id.clone(),
                    standard: at == 0,
                    name: agent_name(harness, *tier, &role.name, &model),
                    title,
                    subtitle,
                });
            }
        }
    }
    out
}

/// Every agent the owner can start with `roles`.
pub fn agent_choices(roles: &[Role], config: &Config, sources: &ModelSources) -> Vec<AgentChoice> {
    let by_tier: Vec<(Tier, Vec<EngineInfo>)> = Tier::ALL
        .into_iter()
        .map(|tier| (tier, engines::engine_for(tier, config, sources)))
        .collect();
    choices_from(roles, &by_tier)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::EngineOrigin;
    use crate::model_tiers::TierSource;
    use axiomata_roster::{Billing, Engine};

    fn info(id: &str, harness: Harness, model: Option<&str>, name: Option<&str>) -> EngineInfo {
        EngineInfo {
            engine: Engine {
                id: id.into(),
                label: id.into(),
                harness,
                command: String::new(),
                model: model.map(String::from),
                env: String::new(),
                billing: Billing::Metered,
                tier: None,
            },
            origin: EngineOrigin::Auto,
            rated_tier: Some(Tier::Medium),
            tier_source: TierSource::Price,
            model_key: None,
            model_name: name.map(String::from),
            output_per_m: None,
            unusable: None,
        }
    }

    #[test]
    fn an_agent_is_named_harness_tier_role_and_model() {
        assert_eq!(
            agent_name(Harness::Glut, Tier::Medium, "allrounder", "Sonnet 5.5"),
            "Glut Medium Allrounder (Sonnet 5.5)"
        );
        assert_eq!(
            agent_name(
                Harness::ClaudeCode,
                Tier::Heavy,
                "code-reviewer",
                "Opus 5.5"
            ),
            "Claude Heavy Code Reviewer (Opus 5.5)"
        );
        assert_eq!(
            agent_lines(Harness::Glut, Tier::Medium, "allrounder", "Sonnet 5.5"),
            (
                "Allrounder · Medium".to_owned(),
                "Glut · Sonnet 5.5".to_owned()
            )
        );
    }

    #[test]
    fn the_model_falls_back_to_its_id_and_then_to_the_harness_default() {
        let named = info("a", Harness::Glut, Some("openrouter/x/y"), Some("Y"));
        let bare = info("b", Harness::Glut, Some("openrouter/x/y"), None);
        let none = info("c", Harness::ClaudeCode, None, None);
        assert_eq!(model_label(&named), "Y");
        assert_eq!(model_label(&bare), "x/y");
        assert_eq!(model_label(&none), "default");
    }

    #[test]
    fn choices_span_roles_tiers_and_engines_with_the_first_engine_as_standard() {
        let roles = vec![
            axiomata_roster::default_role(),
            axiomata_roster::tester_role(),
        ];
        let by_tier = vec![
            (
                Tier::Light,
                vec![
                    info(
                        "cheap",
                        Harness::Glut,
                        Some("openrouter/a/b"),
                        Some("Flash"),
                    ),
                    info(
                        "haiku",
                        Harness::ClaudeCode,
                        Some("claude-haiku-5-5"),
                        Some("Haiku 5.5"),
                    ),
                ],
            ),
            (Tier::Medium, Vec::new()),
        ];
        let choices = choices_from(&roles, &by_tier);
        assert_eq!(choices.len(), 4);
        assert_eq!(choices[0].name, "Glut Light Allrounder (Flash)");
        assert!(choices[0].standard && !choices[1].standard);
        assert_eq!(choices[1].subtitle, "Claude · Haiku 5.5");
        assert_eq!(choices[2].role, roles[1].name);
    }
}
