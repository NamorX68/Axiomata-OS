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
    /// Axiomata's own harness, Glut (`axiomata-harness`, `docs/plans/glut-cards.md`): started through the app's CLI
    /// (`axiomata-cli harness card`). Stored before as `mini`, the placeholder it grew out of (M7.4), which still reads
    /// as this.
    #[serde(alias = "mini")]
    Glut,
}

impl Harness {
    /// The stored spelling, and what the CLI accepts.
    pub fn as_str(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "claude_code",
            Harness::Opencode => "opencode",
            Harness::Glut => "glut",
        }
    }

    /// Parses the stored spelling. `None` for anything else — see the type's docs.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "claude_code" => Some(Harness::ClaudeCode),
            "opencode" => Some(Harness::Opencode),
            "glut" | "mini" => Some(Harness::Glut),
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
            // The app's CLI; a start replaces it with the CLI's full path and the session's own arguments.
            Harness::Glut => "axiomata-cli",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stored_spelling_round_trips_and_unknown_text_is_refused() {
        for harness in [Harness::ClaudeCode, Harness::Opencode, Harness::Glut] {
            assert_eq!(Harness::parse(harness.as_str()), Some(harness));
        }
        // The placeholder Glut grew out of reads as Glut, stored or in a config file.
        assert_eq!(Harness::parse("mini"), Some(Harness::Glut));
        assert_eq!(
            serde_json::from_str::<Harness>("\"mini\"").unwrap(),
            Harness::Glut
        );
        assert_eq!(Harness::parse("Claude"), None);
        assert_eq!(Harness::parse(""), None);
    }
}
