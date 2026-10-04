//! An engine: harness + model + environment — the thing that costs money and can be unreachable.
//!
//! The owner's catalog, global (`config.agents.engines`), so switching an engine happens in one place. A role
//! (see [`crate::role`]) points at engines **by id** and never carries a command line: whatever starts a process
//! is decided here, by the owner, and not by a file that could arrive with a cloned repository.

use serde::{Deserialize, Serialize};

use crate::error::{Result, RosterError};
use crate::harness::Harness;
use crate::ident::check_slug;

/// Longest engine label, in characters (it is shown in lists and tabs).
pub const MAX_LABEL_CHARS: usize = 80;

/// How an engine is paid for, which decides how its limits are measured (`docs/plans/a2a.md`, A9).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Billing {
    /// Tokens times a price: limited in money.
    #[default]
    Metered,
    /// A subscription (Claude Code on the account): limited in tokens, since a dollar figure would be invented.
    Subscription,
}

/// One entry of the engine catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Engine {
    /// The key roles and sessions refer to. A slug (see [`check_slug`]); never changes once sessions use it.
    pub id: String,
    /// What the UI shows, e.g. "Claude Code · Opus".
    pub label: String,
    pub harness: Harness,
    /// The command line that starts it. Empty = [`Harness::default_command`].
    #[serde(default)]
    pub command: String,
    /// `None` = whatever the harness picks. For Opencode this is the full `provider/model` id, so the provider is
    /// not a field of its own.
    #[serde(default)]
    pub model: Option<String>,
    /// `KEY=value` per line, the same shape the Terminal module's env setting uses.
    #[serde(default)]
    pub env: String,
    #[serde(default)]
    pub billing: Billing,
}

impl Engine {
    /// Checks everything a caller can get wrong.
    ///
    /// # Errors
    ///
    /// [`RosterError::Invalid`] naming the first bad field.
    pub fn validate(&self) -> Result<()> {
        check_slug("id", &self.id)?;
        if self.label.trim().is_empty() {
            return Err(RosterError::invalid("label", "must not be empty"));
        }
        if self.label.chars().count() > MAX_LABEL_CHARS || self.label.contains('\n') {
            return Err(RosterError::invalid(
                "label",
                format!("one line, up to {MAX_LABEL_CHARS} characters"),
            ));
        }
        if self.command.contains('\n') || self.command.contains('\0') {
            return Err(RosterError::invalid("command", "must be a single line"));
        }
        if self
            .model
            .as_deref()
            .is_some_and(|m| m.trim().is_empty() || m.contains(['\n', '\0']))
        {
            return Err(RosterError::invalid(
                "model",
                "must be a single non-empty line, or absent",
            ));
        }
        for line in self.env.lines().filter(|l| !l.trim().is_empty()) {
            let Some((key, _)) = line.split_once('=') else {
                return Err(RosterError::invalid(
                    "env",
                    format!("“{line}” is not KEY=value"),
                ));
            };
            let key = key.trim();
            let valid = !key.is_empty()
                && !key.starts_with(|c: char| c.is_ascii_digit())
                && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !valid {
                return Err(RosterError::invalid(
                    "env",
                    format!("“{key}” is not a variable name"),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field name and a change to a valid engine that must break exactly that field.
    type Case = (&'static str, Box<dyn Fn(&mut Engine)>);

    fn engine() -> Engine {
        Engine {
            id: "claude-opus".into(),
            label: "Claude Code · Opus".into(),
            harness: Harness::ClaudeCode,
            command: String::new(),
            model: Some("claude-opus-5-5".into()),
            env: "FOO=1\n\nBAR_2=x y".into(),
            billing: Billing::Subscription,
        }
    }

    #[test]
    fn a_complete_engine_is_valid() {
        assert!(engine().validate().is_ok());
    }

    #[test]
    fn every_bad_field_is_refused_and_named() {
        let cases: [Case; 6] = [
            ("id", Box::new(|e| e.id = "Not A Slug".into())),
            ("label", Box::new(|e| e.label = "  ".into())),
            ("command", Box::new(|e| e.command = "a\nb".into())),
            ("model", Box::new(|e| e.model = Some(" ".into()))),
            ("env", Box::new(|e| e.env = "no equals sign".into())),
            ("env", Box::new(|e| e.env = "1BAD=x".into())),
        ];
        for (field, change) in cases {
            let mut engine = engine();
            change(&mut engine);
            assert!(
                matches!(engine.validate(), Err(RosterError::Invalid { field: f, .. }) if f == field),
                "{field}"
            );
        }
    }

    #[test]
    fn an_engine_survives_toml_style_defaults() {
        let json = r#"{"id":"oc","label":"Opencode","harness":"opencode"}"#;
        let engine: Engine = serde_json::from_str(json).unwrap();
        assert_eq!(engine.billing, Billing::Metered);
        assert!(engine.command.is_empty() && engine.model.is_none() && engine.env.is_empty());
    }
}
