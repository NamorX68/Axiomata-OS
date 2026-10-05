//! An agent role: who does what, with which engine and which limits — one `AGENT.md` per role.
//!
//! The file is YAML frontmatter plus the working instructions as Markdown, like a skill's `SKILL.md`:
//!
//! ```text
//! ---
//! name: "implementer-light"
//! description: "Small, well-specified changes"
//! kind: "implement"
//! tier: light
//! engine: "opencode-flash"
//! fallback_engines: ["claude-sonnet"]
//! creates: ["test"]
//! limits:
//!   max_cost_usd: 0.5
//! ---
//! Read the card, change only what it asks for, …
//! ```
//!
//! Frontmatter fields are strict (`deny_unknown_fields`): a typo in a limit or a right silently doing nothing is
//! worse than a file that is skipped with a reason.

use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::{Deserialize, Serialize};

use crate::error::{Result, RosterError};
use crate::ident::check_slug;

/// Longest instruction text accepted, in bytes. Far above any real role; a guard against a hostile file.
pub const MAX_INSTRUCTIONS_BYTES: usize = 48 * 1024;

/// How strong an agent is meant to be (`docs/plans/a2a.md`, A5). Ordered: escalation goes light → heavy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Light,
    #[default]
    Medium,
    Heavy,
}

/// Where a role came from. A project's own role only applies after the owner confirmed it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// `~/.axiomata/agents/`.
    #[default]
    User,
    /// `<project>/.axiomata/agents/`.
    Project,
}

/// What a session may spend (`docs/plans/a2a.md`, A9). Absent = the global default applies.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Money, for metered engines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost_usd: Option<f64>,
    /// Tokens, for subscription engines (a dollar figure there would be made up).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    /// Steps (tool calls) of one card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u32>,
}

/// The limits a session is held to: a role's own ([`Limits`]) over the default of its tier. Every field is set, so a
/// caller never has to decide what "no limit" would mean.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ResolvedLimits {
    /// Money, applied only to an engine that is paid per token.
    pub max_cost_usd: f64,
    /// Tokens: input, output and cache writes, not cache reads (they cost a fraction and would swamp the count).
    pub max_tokens: u64,
    /// Tool calls.
    pub max_steps: u32,
}

impl Tier {
    /// The default limits of a session of this tier (`docs/plans/a2a.md`, A27): a light role is meant for small
    /// changes, so a session that spends more than this has lost its way rather than found a hard card.
    pub fn default_limits(self) -> ResolvedLimits {
        match self {
            Tier::Light => ResolvedLimits {
                max_cost_usd: 0.5,
                max_tokens: 400_000,
                max_steps: 60,
            },
            Tier::Medium => ResolvedLimits {
                max_cost_usd: 2.0,
                max_tokens: 1_500_000,
                max_steps: 120,
            },
            Tier::Heavy => ResolvedLimits {
                max_cost_usd: 6.0,
                max_tokens: 4_000_000,
                max_steps: 250,
            },
        }
    }
}

impl Limits {
    /// What a session of `tier` is held to: each field this role sets, the tier's default for the rest.
    pub fn resolve(&self, tier: Tier) -> ResolvedLimits {
        let default = tier.default_limits();
        ResolvedLimits {
            max_cost_usd: self.max_cost_usd.unwrap_or(default.max_cost_usd),
            max_tokens: self.max_tokens.unwrap_or(default.max_tokens),
            max_steps: self.max_steps.unwrap_or(default.max_steps),
        }
    }
}

/// One role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Role {
    /// Slug; also the directory name.
    pub name: String,
    /// One line for lists.
    #[serde(default)]
    pub description: String,
    /// The kind of work (`implement`, `review`, `plan`, `test`, `doc`, …); a slug, not a closed list, because the
    /// planner and the cards use the same words.
    pub kind: String,
    #[serde(default)]
    pub tier: Tier,
    /// The default engine's id. `None` for a role whose engine the owner picks at every start (the planner).
    #[serde(default)]
    pub engine: Option<String>,
    /// Engine ids to fall back on, in order.
    #[serde(default)]
    pub fallback_engines: Vec<String>,
    /// Harness permission rules passed on when a session starts (CP-A6). Not interpreted here.
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub limits: Limits,
    /// Card kinds this role may create without asking (`docs/plans/a2a.md`, A7).
    #[serde(default)]
    pub creates: Vec<String>,
    /// The working instructions (the Markdown body).
    #[serde(default)]
    pub instructions: String,
    /// Set by the loader, never read from the file.
    #[serde(default)]
    pub source: Source,
}

/// The frontmatter as written in the file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Frontmatter {
    name: String,
    #[serde(default)]
    description: String,
    kind: String,
    #[serde(default)]
    tier: Tier,
    #[serde(default)]
    engine: Option<String>,
    #[serde(default)]
    fallback_engines: Vec<String>,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    limits: Limits,
    #[serde(default)]
    creates: Vec<String>,
}

impl Role {
    /// Parses the text of an `AGENT.md`. Does not check the role (see [`Role::validate`]).
    ///
    /// # Errors
    ///
    /// [`RosterError::Invalid`] for missing or malformed frontmatter.
    pub fn parse(raw: &str) -> Result<Role> {
        check_frontmatter_shape(raw)?;
        let parsed = Matter::<YAML>::new()
            .parse::<Frontmatter>(raw)
            .map_err(|err| RosterError::invalid("frontmatter", err.to_string()))?;
        let front = parsed.data.ok_or_else(|| {
            RosterError::invalid("frontmatter", "the YAML frontmatter block is missing")
        })?;
        Ok(Role {
            name: front.name,
            description: front.description,
            kind: front.kind,
            tier: front.tier,
            engine: front.engine,
            fallback_engines: front.fallback_engines,
            permissions: front.permissions,
            limits: front.limits,
            creates: front.creates,
            instructions: parsed.content.trim().to_string(),
            source: Source::User,
        })
    }

    /// Checks everything that does not need the engine catalog.
    ///
    /// # Errors
    ///
    /// [`RosterError::Invalid`] naming the first bad field.
    pub fn validate(&self) -> Result<()> {
        check_slug("name", &self.name)?;
        check_slug("kind", &self.kind)?;
        one_line("description", &self.description, 200)?;
        visible_text("description", &self.description, false)?;
        if let Some(engine) = &self.engine {
            check_slug("engine", engine)?;
        }
        let mut seen: Vec<&str> = self.engine.iter().map(String::as_str).collect();
        for id in &self.fallback_engines {
            check_slug("fallback_engines", id)?;
            if seen.contains(&id.as_str()) {
                return Err(RosterError::invalid(
                    "fallback_engines",
                    format!("“{id}” is already the engine or listed twice"),
                ));
            }
            seen.push(id);
        }
        for rule in &self.permissions {
            one_line("permissions", rule, 160)?;
            visible_text("permissions", rule, false)?;
            if rule.trim().is_empty() {
                return Err(RosterError::invalid(
                    "permissions",
                    "a rule must not be empty",
                ));
            }
        }
        let mut kinds: Vec<&str> = Vec::new();
        for kind in &self.creates {
            check_slug("creates", kind)?;
            if kinds.contains(&kind.as_str()) {
                return Err(RosterError::invalid(
                    "creates",
                    format!("“{kind}” is listed twice"),
                ));
            }
            kinds.push(kind);
        }
        if self
            .limits
            .max_cost_usd
            .is_some_and(|c| !c.is_finite() || c <= 0.0)
        {
            return Err(RosterError::invalid(
                "limits",
                "max_cost_usd must be a positive number",
            ));
        }
        if self.limits.max_tokens == Some(0) || self.limits.max_steps == Some(0) {
            return Err(RosterError::invalid(
                "limits",
                "a limit of 0 would stop every session at once",
            ));
        }
        visible_text("instructions", &self.instructions, true)?;
        if self.instructions.len() > MAX_INSTRUCTIONS_BYTES {
            return Err(RosterError::invalid(
                "instructions",
                format!("longer than {MAX_INSTRUCTIONS_BYTES} bytes"),
            ));
        }
        Ok(())
    }

    /// Checks that every engine the role names exists.
    ///
    /// # Errors
    ///
    /// [`RosterError::Invalid`] naming the field and the unknown id.
    pub fn check_engines(&self, exists: impl Fn(&str) -> bool) -> Result<()> {
        if let Some(id) = self.engine.as_deref().filter(|id| !exists(id)) {
            return Err(RosterError::invalid(
                "engine",
                format!("there is no engine “{id}”"),
            ));
        }
        if let Some(id) = self.fallback_engines.iter().find(|id| !exists(id)) {
            return Err(RosterError::invalid(
                "fallback_engines",
                format!("there is no engine “{id}”"),
            ));
        }
        Ok(())
    }

    /// The text of the `AGENT.md` for this role. [`Role::parse`] reads it back to an equal role (but the
    /// [`Source`], which the file does not carry).
    ///
    /// Scalars are written as JSON strings, which YAML reads as double-quoted strings, so no value needs
    /// escaping rules of its own.
    pub fn to_markdown(&self) -> String {
        let quote = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
        let list = |items: &[String]| {
            let quoted: Vec<String> = items.iter().map(|s| quote(s)).collect();
            format!("[{}]", quoted.join(", "))
        };
        let mut out = String::from("---\n");
        out.push_str(&format!("name: {}\n", quote(&self.name)));
        out.push_str(&format!("description: {}\n", quote(&self.description)));
        out.push_str(&format!("kind: {}\n", quote(&self.kind)));
        let tier = match self.tier {
            Tier::Light => "light",
            Tier::Medium => "medium",
            Tier::Heavy => "heavy",
        };
        out.push_str(&format!("tier: {tier}\n"));
        if let Some(engine) = &self.engine {
            out.push_str(&format!("engine: {}\n", quote(engine)));
        }
        out.push_str(&format!(
            "fallback_engines: {}\n",
            list(&self.fallback_engines)
        ));
        out.push_str(&format!("permissions: {}\n", list(&self.permissions)));
        out.push_str(&format!("creates: {}\n", list(&self.creates)));
        let limits = &self.limits;
        if limits != &Limits::default() {
            out.push_str("limits:\n");
            if let Some(cost) = limits.max_cost_usd {
                out.push_str(&format!("  max_cost_usd: {cost}\n"));
            }
            if let Some(tokens) = limits.max_tokens {
                out.push_str(&format!("  max_tokens: {tokens}\n"));
            }
            if let Some(steps) = limits.max_steps {
                out.push_str(&format!("  max_steps: {steps}\n"));
            }
        }
        out.push_str("---\n");
        out.push_str(self.instructions.trim());
        out.push('\n');
        out
    }
}

/// Longest frontmatter read by the YAML parser, in bytes. A real role needs about 1 KiB.
const MAX_FRONTMATTER_BYTES: usize = 8 * 1024;

/// Deepest `[`/`{` nesting accepted in the frontmatter. A role has one level (lists) plus `limits`.
const MAX_FLOW_DEPTH: usize = 8;

/// Refuses frontmatter that the YAML parser could be made to blow up on, **before** it runs.
///
/// The parser (yaml-rust2) resolves an alias by cloning the anchored node and has no expansion limit, so a few
/// kilobytes of nested `&a`/`*a` exhaust memory — and a project's role files are read as soon as the project is
/// opened, before the owner confirmed anything. A role has no use for anchors, aliases or tags, so they are refused;
/// the size and nesting caps bound what is left (deeply nested flow collections recurse).
///
/// Conservative on purpose: `&`, `*` and `!` are refused wherever a YAML token could start outside a quoted string
/// or a comment, which also refuses some harmless plain text (quote it: `description: "reads *.rs"`). A quote only
/// opens a string at a token start, exactly as in YAML — treating the apostrophe in `don't` as one would let the
/// next anchor hide inside a "string".
fn check_frontmatter_shape(raw: &str) -> Result<()> {
    if !raw.starts_with("---") {
        // No frontmatter block: the parser never sees YAML.
        return Ok(());
    }
    let end = raw[3..].find("\n---").map_or(raw.len(), |at| at + 3);
    let front = &raw[..end];
    if front.len() > MAX_FRONTMATTER_BYTES {
        return Err(RosterError::invalid(
            "frontmatter",
            format!("longer than {MAX_FRONTMATTER_BYTES} bytes"),
        ));
    }
    let refuse = |what: &str| {
        Err(RosterError::invalid(
            "frontmatter",
            format!(
                "{what} are not allowed in a role; write plain values (quote text that starts with one)"
            ),
        ))
    };
    let mut depth = 0usize;
    let mut prev: Option<char> = None;
    let mut chars = front.chars();
    while let Some(ch) = chars.next() {
        let at_token_start =
            prev.is_none_or(|p| p.is_whitespace() || matches!(p, '[' | '{' | ',' | ':'));
        match ch {
            '"' if at_token_start => {
                // A double-quoted string runs to the next unescaped quote.
                let mut escaped = false;
                for inner in chars.by_ref() {
                    if escaped {
                        escaped = false;
                    } else if inner == '\\' {
                        escaped = true;
                    } else if inner == '"' {
                        break;
                    }
                }
                prev = Some('"');
                continue;
            }
            '\'' if at_token_start => {
                // A single-quoted string runs to the next quote that is not doubled.
                while let Some(inner) = chars.next() {
                    if inner == '\'' {
                        let mut ahead = chars.clone();
                        if ahead.next() == Some('\'') {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                prev = Some('\'');
                continue;
            }
            '#' if at_token_start => {
                // A comment runs to the end of the line.
                for inner in chars.by_ref() {
                    if inner == '\n' {
                        break;
                    }
                }
                prev = Some('\n');
                continue;
            }
            '&' | '*' if at_token_start => return refuse("anchors and aliases"),
            '!' if at_token_start => return refuse("tags"),
            '[' | '{' => {
                depth += 1;
                if depth > MAX_FLOW_DEPTH {
                    return Err(RosterError::invalid("frontmatter", "nested too deeply"));
                }
            }
            ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        prev = Some(ch);
    }
    Ok(())
}

/// Bidirectional-formatting characters, which can make text read differently from what it is.
fn is_bidi_control(ch: char) -> bool {
    matches!(ch, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Refuses control and bidi-formatting characters. Role text is shown to the owner for confirmation, and a project's
/// role comes from a repository: an escape sequence or a right-to-left override must not disguise what they read.
/// `newlines` also allows line breaks and tabs (for the instructions).
fn visible_text(field: &'static str, value: &str, newlines: bool) -> Result<()> {
    let hidden = value.chars().any(|ch| {
        is_bidi_control(ch) || (ch.is_control() && !(newlines && matches!(ch, '\n' | '\r' | '\t')))
    });
    if hidden {
        return Err(RosterError::invalid(
            field,
            "contains control or bidirectional-formatting characters",
        ));
    }
    Ok(())
}

fn one_line(field: &'static str, value: &str, max: usize) -> Result<()> {
    if value.len() > max || value.contains(['\n', '\r', '\0']) {
        return Err(RosterError::invalid(
            field,
            format!("one line, up to {max} characters"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_a_role_leaves_open_come_from_its_tier() {
        let own = Limits {
            max_steps: Some(10),
            ..Limits::default()
        };
        let resolved = own.resolve(Tier::Heavy);
        assert_eq!(resolved.max_steps, 10);
        assert_eq!(resolved.max_tokens, 4_000_000);
        assert_eq!(resolved.max_cost_usd, 6.0);
        assert_eq!(
            Limits::default().resolve(Tier::Light),
            Tier::Light.default_limits()
        );
    }

    #[test]
    fn a_stronger_tier_may_spend_more() {
        let (light, medium, heavy) = (
            Tier::Light.default_limits(),
            Tier::Medium.default_limits(),
            Tier::Heavy.default_limits(),
        );
        assert!(light.max_steps < medium.max_steps && medium.max_steps < heavy.max_steps);
        assert!(light.max_tokens < medium.max_tokens && medium.max_tokens < heavy.max_tokens);
        assert!(
            light.max_cost_usd < medium.max_cost_usd && medium.max_cost_usd < heavy.max_cost_usd
        );
    }

    /// A field name and a change to a valid role that must break exactly that field.
    type Case = (&'static str, Box<dyn Fn(&mut Role)>);

    fn full() -> Role {
        Role {
            name: "implementer-light".into(),
            description: "Small, well-specified changes — “quoted”, with: colon".into(),
            kind: "implement".into(),
            tier: Tier::Light,
            engine: Some("opencode-flash".into()),
            fallback_engines: vec!["claude-sonnet".into()],
            permissions: vec!["bash: ask".into()],
            limits: Limits {
                max_cost_usd: Some(0.5),
                max_tokens: Some(200_000),
                max_steps: Some(40),
            },
            creates: vec!["test".into(), "doc".into()],
            instructions: "Read the card.\n\n---\n\nThen change only what it asks for.".into(),
            source: Source::User,
        }
    }

    #[test]
    fn a_role_round_trips_through_its_file() {
        let role = full();
        role.validate().unwrap();
        assert_eq!(Role::parse(&role.to_markdown()).unwrap(), role);
    }

    #[test]
    fn a_minimal_file_gets_the_defaults() {
        let role = Role::parse("---\nname: planner\nkind: plan\n---\nPlan it.\n").unwrap();
        assert_eq!(role.tier, Tier::Medium);
        assert!(
            role.engine.is_none() && role.fallback_engines.is_empty() && role.creates.is_empty()
        );
        assert_eq!(role.limits, Limits::default());
        assert_eq!(role.instructions, "Plan it.");
        role.validate().unwrap();
        assert_eq!(Role::parse(&role.to_markdown()).unwrap(), role);
    }

    #[test]
    fn broken_files_are_refused_as_invalid_frontmatter() {
        for raw in [
            "just text",
            "---\nname: x\n---\nno kind",
            "---\nname: x\nkind: plan\ntyp0: 1\n---\n",
            "---\nname: x\nkind: plan\ntier: mighty\n---\n",
            "---\nname: x\nkind: plan\nlimits:\n  max_cost: 3\n---\n",
        ] {
            assert!(
                matches!(
                    Role::parse(raw),
                    Err(RosterError::Invalid {
                        field: "frontmatter",
                        ..
                    })
                ),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn every_bad_field_is_refused_and_named() {
        let cases: Vec<Case> = vec![
            ("name", Box::new(|r| r.name = "Big".into())),
            ("kind", Box::new(|r| r.kind = "two words".into())),
            ("description", Box::new(|r| r.description = "a\nb".into())),
            ("engine", Box::new(|r| r.engine = Some("../x".into()))),
            (
                "fallback_engines",
                Box::new(|r| r.fallback_engines = vec!["opencode-flash".into()]),
            ),
            (
                "fallback_engines",
                Box::new(|r| r.fallback_engines = vec!["a".into(), "a".into()]),
            ),
            (
                "permissions",
                Box::new(|r| r.permissions = vec![" ".into()]),
            ),
            (
                "creates",
                Box::new(|r| r.creates = vec!["test".into(), "test".into()]),
            ),
            ("limits", Box::new(|r| r.limits.max_cost_usd = Some(-1.0))),
            (
                "limits",
                Box::new(|r| r.limits.max_cost_usd = Some(f64::NAN)),
            ),
            ("limits", Box::new(|r| r.limits.max_steps = Some(0))),
            (
                "instructions",
                Box::new(|r| r.instructions = "x".repeat(MAX_INSTRUCTIONS_BYTES + 1)),
            ),
        ];
        for (field, change) in cases {
            let mut role = full();
            change(&mut role);
            assert!(
                matches!(role.validate(), Err(RosterError::Invalid { field: f, .. }) if f == field),
                "{field}"
            );
        }
    }

    #[test]
    fn unknown_engines_are_found() {
        let role = full();
        assert!(role.check_engines(|_| true).is_ok());
        let only_primary = |id: &str| id == "opencode-flash";
        assert!(matches!(
            role.check_engines(only_primary),
            Err(RosterError::Invalid {
                field: "fallback_engines",
                ..
            })
        ));
        assert!(matches!(
            role.check_engines(|_| false),
            Err(RosterError::Invalid {
                field: "engine",
                ..
            })
        ));
    }

    /// A few hundred bytes of nested aliases expand to gigabytes in the YAML parser; none of them may reach it.
    #[test]
    fn an_alias_bomb_is_refused_before_it_is_parsed() {
        let mut bomb =
            String::from("---\nname: x\nkind: plan\na0: &a0 [z, z, z, z, z, z, z, z, z]\n");
        for level in 1..=12 {
            let previous = level - 1;
            let refs = vec![format!("*a{previous}"); 9].join(", ");
            bomb.push_str(&format!("a{level}: &a{level} [{refs}]\n"));
        }
        bomb.push_str("---\nbody\n");
        assert!(matches!(
            Role::parse(&bomb),
            Err(RosterError::Invalid {
                field: "frontmatter",
                ..
            })
        ));
    }

    #[test]
    fn anchors_aliases_and_tags_are_refused_in_every_position_a_token_can_start() {
        for front in [
            "x: &a 1",
            "x: *a",
            "x: [*a]",
            "x: {k: *a}",
            "x: !!str 1",
            "x:\n  - &a one",
            // The apostrophe in a plain scalar is not a quote, so what follows is still parsed as YAML.
            "description: don't\nx: &a [1]",
            "description: say \"hi\nx: &a [1]",
            // A quote closing early must not hide the alias after it.
            "description: \"a\" *a",
        ] {
            let raw = format!("---\nname: x\nkind: plan\n{front}\n---\n");
            assert!(
                matches!(
                    Role::parse(&raw),
                    Err(RosterError::Invalid {
                        field: "frontmatter",
                        ..
                    })
                ),
                "{front:?}"
            );
        }
    }

    #[test]
    fn harmless_text_with_the_same_characters_still_parses() {
        let raw = "---\nname: x\nkind: plan\n\
                   description: \"reads *.rs & more! #1\"\n\
                   permissions: ['a*b', \"c & d\", 'it''s *fine*']\n\
                   # a comment with &anchor and *alias and !tag\n\
                   creates: [a-b] # trailing *comment\n\
                   ---\nBody with *stars*, &amp; and !bangs.\n";
        let role = Role::parse(raw).unwrap();
        assert_eq!(role.description, "reads *.rs & more! #1");
        assert_eq!(role.permissions, ["a*b", "c & d", "it's *fine*"]);
        assert_eq!(role.instructions, "Body with *stars*, &amp; and !bangs.");
    }

    #[test]
    fn oversized_or_deeply_nested_frontmatter_is_refused() {
        let big = format!(
            "---\nname: x\nkind: plan\nnote: \"{}\"\n---\n",
            "a".repeat(MAX_FRONTMATTER_BYTES)
        );
        assert!(matches!(
            Role::parse(&big),
            Err(RosterError::Invalid {
                field: "frontmatter",
                ..
            })
        ));
        let deep = format!(
            "---\nname: x\nkind: plan\nn: {}1{}\n---\n",
            "[".repeat(40),
            "]".repeat(40)
        );
        assert!(matches!(
            Role::parse(&deep),
            Err(RosterError::Invalid {
                field: "frontmatter",
                ..
            })
        ));
    }

    #[test]
    fn text_that_could_disguise_itself_is_refused() {
        let cases: Vec<Case> = vec![
            (
                "description",
                Box::new(|r| r.description = "a\u{1b}[31mred".into()),
            ),
            (
                "description",
                Box::new(|r| r.description = "evil\u{202E}txt.exe".into()),
            ),
            (
                "permissions",
                Box::new(|r| r.permissions = vec!["x\u{7}".into()]),
            ),
            (
                "instructions",
                Box::new(|r| r.instructions = "rm -rf\u{2066}".into()),
            ),
            (
                "instructions",
                Box::new(|r| r.instructions = "esc \u{1b}[2J".into()),
            ),
        ];
        for (field, change) in cases {
            let mut role = full();
            change(&mut role);
            assert!(
                matches!(role.validate(), Err(RosterError::Invalid { field: f, .. }) if f == field),
                "{field}"
            );
        }
        // Line breaks, tabs and ordinary Unicode (emoji with joiners, accents) stay fine in instructions.
        let mut ok = full();
        ok.instructions = "Zeile 1\n\tZeile 2 – ünïcödé 👨\u{200D}👩".into();
        ok.validate().unwrap();
    }

    #[test]
    fn tiers_are_ordered_for_escalation() {
        assert!(Tier::Light < Tier::Medium && Tier::Medium < Tier::Heavy);
    }
}
