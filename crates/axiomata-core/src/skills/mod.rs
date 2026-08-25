//! Skill discovery and headless execution.
//!
//! Skills are read from two locations and merged: global skills under
//! `~/.axiomata/skills/`, and workspace-local skills under
//! `<workspace_root>/.claude/skills/`, which take priority on name collisions.
//!
//! Implemented starting in M1.

pub mod registry;
pub mod runlog;
pub mod runner;
