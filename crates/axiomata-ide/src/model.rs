//! Domain types for the agentic IDE.
//!
//! Today that is one type, [`Project`]. The crate is cut this way from the
//! start because M7.2 onwards adds agents, worktrees and a mailbox beside it,
//! and they must land next to the project rather than inside `axiomata-core`.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A folder the IDE works in, plus the dock layout left behind in it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    /// Absolute and canonicalised. See `schema.sql` for why that matters.
    pub repo_root: PathBuf,
    /// The frontend's serialised dock tree, opaque here. `None` = never
    /// opened, which is what makes the IDE build its starting layout.
    pub layout_json: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_opened_at: Option<DateTime<Utc>>,
    /// **Computed on read, never stored**: does `repo_root` still point at a
    /// directory?
    ///
    /// The database and the file system drift — an external disk is not
    /// mounted, a folder was renamed, a repository in iCloud got evicted. The
    /// list is the right place to notice, because noticing at open time means
    /// the user already clicked something that then failed. Costs one `is_dir`
    /// per project, against a list of maybe a dozen.
    ///
    /// ⚠️ `Project` derives `Deserialize` so it can travel back over IPC, which
    /// means a command could in principle *accept* one — including a
    /// client-supplied `root_exists` that nothing checked. Mutating commands
    /// therefore take narrow requests (an id, a name, a path), never a whole
    /// `Project`, so this field can only ever come from the store's own
    /// `is_dir`.
    pub root_exists: bool,
}

/// What a caller supplies to create a project. Validated by the store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewProject {
    pub name: String,
    pub repo_root: PathBuf,
}
