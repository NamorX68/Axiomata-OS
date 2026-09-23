//! Which places on disk the file app may touch, and what they are called
//! (decisions E1, E7 and E8 of `docs/plans/editor.md` §ED0).
//!
//! `axiomata-files` guards a root once it has one; this module is where the
//! roots come from. A root id is what the webview sends instead of a path:
//!
//! | id               | root                                   | links      |
//! |------------------|----------------------------------------|------------|
//! | `workspace`      | the Second-Brain workspace             | strict     |
//! | `project:<id>`   | an IDE project folder                  | contained  |
//! | `worktree:<id>`  | an agent's git worktree (agent id)     | contained  |
//! | `grant:<id>`     | a file or folder picked in the dialog  | contained  |
//!
//! Every lookup reads the config, the database or the grant file afresh, so
//! a project moved with "Pfad ändern", a discarded worktree or a revoked
//! grant takes effect at the next access.

use std::path::{Path, PathBuf};

use axiomata_files::{FilesError, GrantKind, GrantStore, LinkPolicy, Root, RootResolver};
use rusqlite::Connection;
use serde::Serialize;

use crate::config::Config;
use crate::ide;
use crate::memory::guarded_root;
use crate::paths;

/// A parsed root id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootId {
    Workspace,
    Project(i64),
    /// Keyed by the agent's id: an agent has at most one worktree.
    Worktree(i64),
    Grant(String),
}

impl RootId {
    /// Parses `workspace`, `project:<n>`, `worktree:<n>` or `grant:<id>`.
    pub fn parse(id: &str) -> Option<Self> {
        if id == "workspace" {
            return Some(Self::Workspace);
        }
        let (kind, rest) = id.split_once(':')?;
        match kind {
            "project" => rest.parse().ok().map(Self::Project),
            "worktree" => rest.parse().ok().map(Self::Worktree),
            "grant" if !rest.is_empty() => Some(Self::Grant(rest.to_string())),
            _ => None,
        }
    }
}

impl std::fmt::Display for RootId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Workspace => f.write_str("workspace"),
            Self::Project(id) => write!(f, "project:{id}"),
            Self::Worktree(id) => write!(f, "worktree:{id}"),
            Self::Grant(id) => write!(f, "grant:{id}"),
        }
    }
}

/// One root, as listed for people (`files roots`, later the editor).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RootInfo {
    pub id: String,
    /// What to call it: the project's or agent's name, the picked path.
    pub label: String,
    /// Where it lives — for display only; nothing accepts a path back.
    pub path: PathBuf,
    /// `workspace`, `project`, `worktree`, `grant-file` or `grant-folder`.
    pub kind: &'static str,
}

/// The grant file under `~/.axiomata/`.
pub fn grant_store() -> GrantStore {
    GrantStore::new(paths::file_grants_path())
}

/// The roots of one Axiomata install: config + database + grant file.
/// Cheap to build; build one per request and drop it with the DB lock.
pub struct Roots<'a> {
    config: &'a Config,
    db: &'a Connection,
    grants: GrantStore,
}

impl<'a> Roots<'a> {
    /// The roots of this install, with the grant file at its usual place.
    pub fn new(config: &'a Config, db: &'a Connection) -> Self {
        Self::with_grants(config, db, grant_store())
    }

    /// As [`Self::new`], with the grant file somewhere else (tests).
    pub fn with_grants(config: &'a Config, db: &'a Connection, grants: GrantStore) -> Self {
        Self { config, db, grants }
    }

    /// The grant store these roots read.
    pub fn grants(&self) -> &GrantStore {
        &self.grants
    }

    /// Every root that currently resolves, in the order of the table in the
    /// module docs. A project whose folder is gone, or an agent without a
    /// worktree, is left out rather than listed as broken.
    ///
    /// Errors:
    ///     [`FilesError::Io`] if the database or the grant file cannot be read.
    pub fn list(&self) -> Result<Vec<RootInfo>, FilesError> {
        let mut out = Vec::new();
        if let Ok(root) = self.workspace() {
            out.push(RootInfo {
                id: RootId::Workspace.to_string(),
                label: "Second Brain".into(),
                path: root.path().to_path_buf(),
                kind: "workspace",
            });
        }
        for project in ide::store::list_projects(self.db).map_err(db_error)? {
            if project.root_exists {
                out.push(RootInfo {
                    id: RootId::Project(project.id).to_string(),
                    label: project.name.clone(),
                    path: project.repo_root.clone(),
                    kind: "project",
                });
            }
            for agent in ide::agent_store::list_agents(self.db, project.id).map_err(db_error)? {
                if let Some(path) = agent.worktree_path.filter(|p| p.is_dir()) {
                    out.push(RootInfo {
                        id: RootId::Worktree(agent.id).to_string(),
                        label: format!("{} · {}", project.name, agent.name),
                        path,
                        kind: "worktree",
                    });
                }
            }
        }
        for grant in self.grants.list()? {
            out.push(RootInfo {
                id: RootId::Grant(grant.id.clone()).to_string(),
                label: grant.path.display().to_string(),
                kind: match grant.kind {
                    GrantKind::File => "grant-file",
                    GrantKind::Folder => "grant-folder",
                },
                path: grant.path,
            });
        }
        Ok(out)
    }

    /// Finds an already registered root that gives out the file or folder
    /// at `path` (a path from the open dialog), so picking something inside
    /// a project does not pile up a grant for it. The innermost matching root
    /// wins — a worktree or project inside the workspace is found as itself.
    /// File grants are not searched: they would only ever match themselves.
    ///
    /// For a folder this matters beyond tidiness: a folder grant is
    /// [`LinkPolicy::Contained`], so granting the workspace folder (or one
    /// inside it) afresh would open a second, looser way into content the
    /// strict `workspace` root guards (ED0.2 security audit).
    ///
    /// `path` is canonicalised first, so a picked symlink finds what it
    /// names. Returns `(root id, relative path)` — `""` for a root's own
    /// folder — or `None` if no root gives it out (including when its policy
    /// refuses a file — say, a hard-linked one in the strict workspace); the
    /// caller then grants it.
    pub fn locate(&self, path: &Path) -> Option<(String, String)> {
        let file = path.canonicalize().ok()?;
        let is_dir = file.is_dir();
        let mut best: Option<(usize, String, String)> = None;
        for info in self.list().ok()? {
            if info.kind == "grant-file" {
                continue;
            }
            let Ok(root) = self.root(&info.id) else {
                continue;
            };
            let Ok(inside) = file.strip_prefix(root.path()) else {
                continue;
            };
            let Some(rel) = inside.to_str().map(str::to_string) else {
                continue;
            };
            let depth = root.path().components().count();
            // A canonical folder under the root is reachable as it is; a
            // file must also pass the root's policy.
            let gives_it_out = is_dir || root.resolve(&rel).is_ok_and(|p| p.is_file());
            if gives_it_out && best.as_ref().is_none_or(|(d, ..)| depth > *d) {
                best = Some((depth, info.id, rel));
            }
        }
        best.map(|(_, id, rel)| (id, rel))
    }

    fn workspace(&self) -> Result<Root, FilesError> {
        let dir = guarded_root(self.config).map_err(|err| FilesError::Refused {
            path: self.config.workspace_root.clone(),
            reason: err.to_string(),
        })?;
        Root::dir(&dir, LinkPolicy::Strict)
    }
}

impl RootResolver for Roots<'_> {
    fn root(&self, id: &str) -> Result<Root, FilesError> {
        let unknown = || FilesError::UnknownRoot(id.to_string());
        match RootId::parse(id).ok_or_else(unknown)? {
            RootId::Workspace => self.workspace(),
            RootId::Project(project_id) => {
                let project = ide::store::get_project(self.db, project_id)
                    .map_err(db_error)?
                    .ok_or_else(unknown)?;
                Root::dir(&project.repo_root, LinkPolicy::Contained)
            }
            RootId::Worktree(agent_id) => {
                let path = ide::agent_store::get_agent(self.db, agent_id)
                    .map_err(db_error)?
                    .and_then(|agent| agent.worktree_path)
                    .ok_or_else(unknown)?;
                Root::dir(&path, LinkPolicy::Contained)
            }
            RootId::Grant(grant_id) => self.grants.get(&grant_id)?.ok_or_else(unknown)?.root(),
        }
    }
}

/// A database failure while looking a root up, as the file service's error.
fn db_error(err: ide::IdeError) -> FilesError {
    FilesError::Io {
        path: paths::db_path(),
        source: std::io::Error::other(err),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::test_support::unique_temp_dir;

    struct Fixture {
        dir: PathBuf,
        config: Config,
        db: Connection,
        grants: GrantStore,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = unique_temp_dir("axiomata-core-files");
            for sub in ["brain/notes", "code/src", "tree", "loose"] {
                fs::create_dir_all(dir.join(sub)).unwrap();
            }
            fs::write(dir.join("brain/notes/a.md"), "a").unwrap();
            fs::write(dir.join("code/src/main.rs"), "fn main() {}").unwrap();
            fs::write(dir.join("tree/wip.rs"), "wip").unwrap();
            fs::write(dir.join("loose/x.txt"), "x").unwrap();
            let db = crate::db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap();
            let config = Config {
                workspace_root: dir.join("brain"),
                ..Config::default()
            };
            let grants = GrantStore::new(dir.join("file-grants.json"));
            Self {
                dir,
                config,
                db,
                grants,
            }
        }

        fn roots(&self) -> Roots<'_> {
            Roots::with_grants(&self.config, &self.db, self.grants.clone())
        }

        /// A project over `code/` with one agent whose worktree is `tree/`.
        fn project_with_agent(&self) -> (i64, i64) {
            let project = ide::store::create_project(
                &self.db,
                ide::NewProject {
                    name: "Code".into(),
                    repo_root: self.dir.join("code"),
                },
            )
            .unwrap();
            let agent = ide::agent_store::create_agent(
                &self.db,
                ide::NewAgent {
                    project_id: project.id,
                    fields: ide::AgentFields {
                        name: "builder".into(),
                        harness: ide::Harness::Opencode,
                        command: String::new(),
                        model: None,
                        env: String::new(),
                    },
                },
            )
            .unwrap();
            ide::agent_store::set_worktree(
                &self.db,
                agent.id,
                Some(&self.dir.join("tree")),
                Some("b"),
            )
            .unwrap();
            (project.id, agent.id)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn root_ids_round_trip_and_junk_is_refused() {
        for id in ["workspace", "project:3", "worktree:12", "grant:7"] {
            assert_eq!(RootId::parse(id).unwrap().to_string(), id);
        }
        for junk in [
            "",
            "Workspace",
            "project:",
            "project:x",
            "grant:",
            "file:1",
            "/etc",
        ] {
            assert_eq!(RootId::parse(junk), None, "{junk}");
        }
    }

    #[test]
    fn root_id_parse_handles_the_numeric_edge_cases_of_i64_parsing() {
        // `rest.parse().ok()` inherits whatever `i64::from_str` accepts —
        // including a negative id and a zero-padded one — rather than a
        // grammar of its own. Harmless in practice (a negative or padded id
        // just resolves to `UnknownRoot`, same as any other id nothing owns),
        // but worth pinning down since it is easy to assume digits-only.
        assert_eq!(RootId::parse("project:-1"), Some(RootId::Project(-1)));
        assert_eq!(RootId::parse("worktree:007"), Some(RootId::Worktree(7)));
        for junk in [
            "project:1:2",                  // an extra colon is not numeric
            "project: 1",                   // leading whitespace
            "project:1 ",                   // trailing whitespace
            "project:99999999999999999999", // overflows i64
            "worktree:-1x",
        ] {
            assert_eq!(RootId::parse(junk), None, "{junk}");
        }
    }

    #[test]
    fn resolves_every_kind_of_root_with_its_link_policy() {
        let fx = Fixture::new();
        let (project, agent) = fx.project_with_agent();
        let grant = fx.grants.grant(&fx.dir.join("loose")).unwrap();
        let roots = fx.roots();

        let workspace = roots.root("workspace").unwrap();
        assert_eq!(workspace.policy(), LinkPolicy::Strict);
        assert!(workspace.resolve("notes/a.md").is_ok());

        let code = roots.root(&format!("project:{project}")).unwrap();
        assert_eq!(code.policy(), LinkPolicy::Contained);
        assert!(code.resolve("src/main.rs").is_ok());
        assert!(
            roots
                .root(&format!("worktree:{agent}"))
                .unwrap()
                .resolve("wip.rs")
                .is_ok()
        );
        assert!(
            roots
                .root(&format!("grant:{}", grant.id))
                .unwrap()
                .resolve("x.txt")
                .is_ok()
        );

        for missing in ["project:999", "worktree:999", "grant:999", "nonsense"] {
            assert_eq!(
                roots.root(missing).unwrap_err().kind(),
                "UnknownRoot",
                "{missing}"
            );
        }
    }

    #[test]
    fn a_discarded_worktree_or_revoked_grant_is_gone_at_the_next_lookup() {
        let fx = Fixture::new();
        let (_, agent) = fx.project_with_agent();
        let grant = fx.grants.grant(&fx.dir.join("loose/x.txt")).unwrap();
        let roots = fx.roots();
        assert!(roots.root(&format!("worktree:{agent}")).is_ok());

        ide::agent_store::set_worktree(&fx.db, agent, None, None).unwrap();
        fx.grants.revoke(&grant.id).unwrap();
        assert_eq!(
            roots.root(&format!("worktree:{agent}")).unwrap_err().kind(),
            "UnknownRoot"
        );
        assert_eq!(
            roots
                .root(&format!("grant:{}", grant.id))
                .unwrap_err()
                .kind(),
            "UnknownRoot"
        );
    }

    #[test]
    fn lists_what_resolves_and_locates_the_innermost_root() {
        let fx = Fixture::new();
        let (project, agent) = fx.project_with_agent();
        fx.grants.grant(&fx.dir.join("loose/x.txt")).unwrap();
        let roots = fx.roots();

        let kinds: Vec<_> = roots.list().unwrap().iter().map(|r| r.kind).collect();
        assert_eq!(kinds, ["workspace", "project", "worktree", "grant-file"]);

        assert_eq!(
            roots.locate(&fx.dir.join("code/src/main.rs")),
            Some((format!("project:{project}"), "src/main.rs".into()))
        );
        assert_eq!(
            roots.locate(&fx.dir.join("tree/wip.rs")),
            Some((format!("worktree:{agent}"), "wip.rs".into()))
        );
        assert_eq!(
            roots.locate(&fx.dir.join("brain/notes/a.md")),
            Some(("workspace".into(), "notes/a.md".into()))
        );
        // A folder inside a root (or the root itself) is found as that root.
        assert_eq!(
            roots.locate(&fx.dir.join("brain/notes")),
            Some(("workspace".into(), "notes".into()))
        );
        assert_eq!(
            roots.locate(&fx.dir.join("brain")),
            Some(("workspace".into(), String::new()))
        );
        assert_eq!(roots.locate(&fx.dir.join("loose")), None);
        // Outside every root, or only reachable through a file grant: none.
        assert_eq!(roots.locate(&fx.dir.join("loose/x.txt")), None);
        assert_eq!(roots.locate(&fx.dir.join("nope.txt")), None);
    }

    #[test]
    fn locate_prefers_a_worktree_root_nested_inside_its_own_project_root() {
        let fx = Fixture::new();
        // The worktree lives *inside* the project's own repo root here
        // (unlike `project_with_agent`'s sibling layout) -- both roots'
        // canonical directories contain the file, and the deeper one, the
        // worktree, must win.
        fs::create_dir_all(fx.dir.join("code/.worktrees/agent")).unwrap();
        fs::write(fx.dir.join("code/.worktrees/agent/note.md"), "n").unwrap();

        let project = ide::store::create_project(
            &fx.db,
            ide::NewProject {
                name: "Code".into(),
                repo_root: fx.dir.join("code"),
            },
        )
        .unwrap();
        let agent = ide::agent_store::create_agent(
            &fx.db,
            ide::NewAgent {
                project_id: project.id,
                fields: ide::AgentFields {
                    name: "builder".into(),
                    harness: ide::Harness::Opencode,
                    command: String::new(),
                    model: None,
                    env: String::new(),
                },
            },
        )
        .unwrap();
        ide::agent_store::set_worktree(
            &fx.db,
            agent.id,
            Some(&fx.dir.join("code/.worktrees/agent")),
            Some("b"),
        )
        .unwrap();

        let roots = fx.roots();
        assert_eq!(
            roots.locate(&fx.dir.join("code/.worktrees/agent/note.md")),
            Some((format!("worktree:{}", agent.id), "note.md".into()))
        );
        // A file elsewhere in the project still resolves to the project, not
        // the (unrelated) worktree.
        assert_eq!(
            roots.locate(&fx.dir.join("code/src/main.rs")),
            Some((format!("project:{}", project.id), "src/main.rs".into()))
        );
        // The worktree's own folder, named as a folder, is the worktree too.
        assert_eq!(
            roots.locate(&fx.dir.join("code/.worktrees/agent")),
            Some((format!("worktree:{}", agent.id), String::new()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn locate_follows_a_picked_symlink_to_the_file_it_names() {
        let fx = Fixture::new();
        std::os::unix::fs::symlink(
            fx.dir.join("brain/notes/a.md"),
            fx.dir.join("brain/link.md"),
        )
        .unwrap();
        // The strict workspace would refuse `link.md` itself, but the dialog
        // hands over a path that is canonicalised first: the real file.
        assert_eq!(
            fx.roots().locate(&fx.dir.join("brain/link.md")),
            Some(("workspace".into(), "notes/a.md".into()))
        );
    }
}
