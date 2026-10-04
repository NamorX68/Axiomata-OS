//! Which program an engine runs.

use serde::{Deserialize, Serialize};

/// Which harness runs an agent.
///
/// Stored as text and refused rather than defaulted when it is anything else: a row whose harness nobody
/// recognises is a row that would otherwise be started with the wrong program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Harness {
    /// Anthropic's Claude Code CLI.
    ClaudeCode,
    /// The Opencode CLI — the same harness skills and routines already run on.
    Opencode,
    /// Axiomata's own agent loop, `axiomata-miniagent` (M7.4). A profile can name it before it exists; starting
    /// one then fails, which is honest.
    Mini,
}

impl Harness {
    /// The stored spelling, and what the CLI accepts.
    pub fn as_str(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "claude_code",
            Harness::Opencode => "opencode",
            Harness::Mini => "mini",
        }
    }

    /// Parses the stored spelling. `None` for anything else — see the type's docs.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "claude_code" => Some(Harness::ClaudeCode),
            "opencode" => Some(Harness::Opencode),
            "mini" => Some(Harness::Mini),
            _ => None,
        }
    }

    /// The command line used when an engine's own `command` is empty.
    ///
    /// Resolved here rather than written into every row, so changing what "the default Opencode agent" means does
    /// not need a data migration — and so a row can still pin its own command when the default moves.
    pub fn default_command(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "claude",
            Harness::Opencode => "opencode",
            // No binary yet (M7.4). Naming the crate rather than an empty string makes the failure message say
            // what is missing.
            Harness::Mini => "axiomata-miniagent",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stored_spelling_round_trips_and_unknown_text_is_refused() {
        for harness in [Harness::ClaudeCode, Harness::Opencode, Harness::Mini] {
            assert_eq!(Harness::parse(harness.as_str()), Some(harness));
        }
        assert_eq!(Harness::parse("Claude"), None);
        assert_eq!(Harness::parse(""), None);
    }
}
