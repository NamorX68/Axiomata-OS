//! Agent backends: `ClaudeCode` (headless Claude Code CLI) and `Ollama` (local
//! models via the Ollama HTTP API). See the M0 plan's "Agent-Backend" section
//! for the full design.
//!
//! Implemented starting in M1.

pub mod claude_code;
pub mod ollama;
