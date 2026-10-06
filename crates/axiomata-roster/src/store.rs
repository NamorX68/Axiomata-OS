//! Roles on disk: `<dir>/<name>/AGENT.md`.
//!
//! The file system is the single source of truth, as for skills. One bad file never breaks the rest: it is
//! reported in [`Loaded::skipped`] with the reason instead of vanishing.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{Result, RosterError};
use crate::ident::check_slug;
use crate::role::{Limits, Role, Source, Tier};

/// The file name inside a role's directory.
pub const ROLE_FILE: &str = "AGENT.md";

/// Largest `AGENT.md` read, in bytes: the instruction limit plus room for the frontmatter. A guard against a
/// hostile or accidental huge file (and YAML anchor expansion in the frontmatter).
const MAX_FILE_BYTES: u64 = 64 * 1024;

/// Most role directories read from one directory. A repository's role files are read as soon as its project is
/// opened, so the count and the total size are bounded (a few dozen roles is already a lot).
const MAX_ENTRIES: usize = 64;

/// Most bytes of role files read from one directory in total (all of them are held in memory for the hash).
const MAX_TOTAL_BYTES: usize = 1024 * 1024;

/// Name of the marker entry reported when a directory has more role files than are read.
pub const TRUNCATED_NAME: &str = "(more role files)";

/// A role directory that was left out, and why.
#[derive(Debug, Clone, Serialize)]
pub struct Skipped {
    /// The directory's name — not necessarily the `name:` inside, which may not even have parsed.
    pub name: String,
    pub reason: String,
}

/// Everything found in one roles directory.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Loaded {
    /// Sorted by name.
    pub roles: Vec<Role>,
    pub skipped: Vec<Skipped>,
}

/// One scanned directory entry; keeps the raw bytes so a hash is taken over exactly what was parsed.
pub(crate) struct Entry {
    pub name: String,
    /// `None` when the file could not be read at all (symlink, oversize, I/O error).
    pub bytes: Option<Vec<u8>>,
    pub outcome: std::result::Result<Role, String>,
}

fn io_error(path: &Path, source: io::Error) -> RosterError {
    RosterError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// Opens `path` for reading without following a symlink as its last component, and without blocking on a FIFO.
#[cfg(unix)]
fn open_plain(path: &Path) -> io::Result<fs::File> {
    use rustix::fs::{CWD, Mode, OFlags, openat};
    let fd = openat(
        CWD,
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    Ok(fs::File::from(fd))
}

#[cfg(not(unix))]
fn open_plain(path: &Path) -> io::Result<fs::File> {
    fs::File::open(path)
}

/// Reads one `AGENT.md` with the guards: a regular file, not a symlink, bounded size.
///
/// The symlink check on the path only gives a readable reason; what actually protects is opening with
/// `O_NOFOLLOW` and asking the open file (not the path again) whether it is a regular file — a repository that is
/// written to concurrently could otherwise swap the file for a link between the check and the open.
fn read_file(path: &Path) -> std::result::Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path).map_err(|_| format!("no {ROLE_FILE}"))?;
    if !meta.file_type().is_file() {
        return Err(format!("{ROLE_FILE} is not a regular file (a symlink?)"));
    }
    let file = open_plain(path).map_err(|err| format!("{ROLE_FILE} could not be opened: {err}"))?;
    if !file
        .metadata()
        .map_err(|err| err.to_string())?
        .file_type()
        .is_file()
    {
        return Err(format!("{ROLE_FILE} is not a regular file (a symlink?)"));
    }
    // Bounded read instead of trusting the metadata: the file could grow between the check and the read.
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(format!("{ROLE_FILE} is larger than {MAX_FILE_BYTES} bytes"));
    }
    Ok(bytes)
}

fn parse_entry(dir_name: &str, bytes: &[u8], source: Source) -> std::result::Result<Role, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "not valid UTF-8".to_string())?;
    let mut role = Role::parse(text).map_err(|e| e.to_string())?;
    role.source = source;
    role.validate().map_err(|e| e.to_string())?;
    // A directory called `a` whose file says `b` would be two names for one role.
    if role.name != dir_name {
        return Err(format!(
            "the name “{}” does not match the directory",
            role.name
        ));
    }
    Ok(role)
}

/// What a scan found, and whether it stopped at a limit.
pub(crate) struct Scan {
    pub entries: Vec<Entry>,
    /// There were more role directories (or more bytes) than are read. What was read is still reported, but the
    /// directory must not be taken as complete — a project's roles are then not confirmable.
    pub truncated: bool,
}

/// Scans `dir` for `<name>/AGENT.md`, in directory-name order. A missing directory is empty. At most
/// [`MAX_ENTRIES`] directories and [`MAX_TOTAL_BYTES`] bytes are read; the rest is not touched.
pub(crate) fn scan(dir: &Path, source: Source) -> Result<Scan> {
    let reader = match fs::read_dir(dir) {
        Ok(reader) => reader,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Ok(Scan {
                entries: Vec::new(),
                truncated: false,
            });
        }
        Err(err) => return Err(io_error(dir, err)),
    };
    // Names first (cheap), sorted, so which entries are read when there are too many does not depend on the
    // order the file system happens to list them in.
    let mut candidates: Vec<(String, std::fs::FileType, PathBuf)> = Vec::new();
    for item in reader {
        let item = item.map_err(|e| io_error(dir, e))?;
        // `file_type` does not follow symlinks, so a link is seen as a link.
        if let Ok(kind) = item.file_type() {
            candidates.push((
                item.file_name().to_string_lossy().into_owned(),
                kind,
                item.path(),
            ));
        }
    }
    candidates.sort_by(|a, b| a.0.cmp(&b.0));

    let mut entries = Vec::new();
    let mut truncated = false;
    let mut total = 0usize;
    for (name, kind, path) in candidates {
        if kind.is_symlink() {
            entries.push(Entry {
                name,
                bytes: None,
                outcome: Err("the role directory itself is a symlink".into()),
            });
        } else if kind.is_dir() {
            if entries.len() >= MAX_ENTRIES || total >= MAX_TOTAL_BYTES {
                truncated = true;
                break;
            }
            match read_file(&path.join(ROLE_FILE)) {
                Ok(bytes) => {
                    total += bytes.len();
                    let outcome = parse_entry(&name, &bytes, source);
                    entries.push(Entry {
                        name,
                        bytes: Some(bytes),
                        outcome,
                    });
                }
                Err(reason) => entries.push(Entry {
                    name,
                    bytes: None,
                    outcome: Err(reason),
                }),
            }
        }
        // A stray file next to the role directories is not a role.
    }
    Ok(Scan { entries, truncated })
}

/// Loads every role under `dir`; `source` is stamped on each.
///
/// # Errors
///
/// [`RosterError::Io`] only for a failure listing `dir` itself.
pub fn load_roles(dir: &Path, source: Source) -> Result<Loaded> {
    let scanned = scan(dir, source)?;
    let mut loaded = Loaded::default();
    for entry in scanned.entries {
        match entry.outcome {
            Ok(role) => loaded.roles.push(role),
            Err(reason) => loaded.skipped.push(Skipped {
                name: entry.name,
                reason,
            }),
        }
    }
    if scanned.truncated {
        loaded.skipped.push(Skipped {
            name: TRUNCATED_NAME.into(),
            reason: format!("more than {MAX_ENTRIES} role directories (or {MAX_TOTAL_BYTES} bytes); the rest was not read"),
        });
    }
    Ok(loaded)
}

fn role_dir(dir: &Path, name: &str) -> Result<PathBuf> {
    check_slug("name", name)?;
    Ok(dir.join(name))
}

/// Writes `role` to `<dir>/<name>/AGENT.md` (atomically), creating or replacing it.
///
/// # Errors
///
/// [`RosterError::Invalid`] if the role is not valid or its directory is a symlink; [`RosterError::Io`].
pub fn save_role(dir: &Path, role: &Role) -> Result<()> {
    role.validate()?;
    let target_dir = role_dir(dir, &role.name)?;
    if is_symlink(&target_dir) {
        return Err(RosterError::invalid(
            "name",
            "the role directory is a symlink",
        ));
    }
    fs::create_dir_all(&target_dir).map_err(|e| io_error(&target_dir, e))?;
    let file = target_dir.join(ROLE_FILE);
    if is_symlink(&file) {
        return Err(RosterError::invalid("name", "the role file is a symlink"));
    }
    let tmp = target_dir.join(format!("{ROLE_FILE}.tmp"));
    // `create_new` after removing a stale one: a pre-planted symlink at the temporary name is never written through.
    let _ = fs::remove_file(&tmp);
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| io_error(&tmp, e))?;
    std::io::Write::write_all(&mut out, role.to_markdown().as_bytes())
        .map_err(|e| io_error(&tmp, e))?;
    drop(out);
    fs::rename(&tmp, &file).map_err(|e| io_error(&file, e))
}

/// Removes a role's `AGENT.md` and its directory. `false` if there was no such role.
///
/// # Errors
///
/// [`RosterError::Invalid`] for a bad name or a symlinked directory; [`RosterError::Io`].
pub fn delete_role(dir: &Path, name: &str) -> Result<bool> {
    let target_dir = role_dir(dir, name)?;
    if is_symlink(&target_dir) {
        return Err(RosterError::invalid(
            "name",
            "the role directory is a symlink",
        ));
    }
    let file = target_dir.join(ROLE_FILE);
    match fs::remove_file(&file) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(io_error(&file, err)),
    }
    // Other files the owner keeps in the directory stay; then the directory is simply not removed.
    let _ = fs::remove_dir(&target_dir);
    Ok(true)
}

/// What a session of a working or judging role may run without asking, in Claude Code's spelling (`Bash(prefix:*)`; the
/// studio gives Opencode the same as shell patterns): the builds and checks of the stacks the owner works in. Each is
/// narrow
/// on purpose — no `Bash(*)`, nothing that pushes or reaches beyond the checkout (`agent_entry::grantable` drops what
/// is not).
/// A session that has to ask for `cargo test` stands still in a pane nobody watches.
const BUILD_AND_TEST: [&str; 18] = [
    "Bash(cargo build:*)",
    "Bash(cargo check:*)",
    "Bash(cargo clippy:*)",
    "Bash(cargo test:*)",
    "Bash(cargo fmt --check)",
    "Bash(cargo fmt --check:*)",
    "Bash(npm run check)",
    "Bash(npm run build)",
    "Bash(npm test)",
    "Bash(npx vitest run)",
    "Bash(npx vitest run:*)",
    "Bash(uv run pytest:*)",
    "Bash(uv run ruff check:*)",
    "Bash(uv run ruff format --check:*)",
    "Bash(uv run pyright:*)",
    "Bash(pytest:*)",
    "Bash(ruff check:*)",
    "Bash(pyright:*)",
];

/// The git commands that only read: the roles that have shell rules at all may run them without asking. The studio
/// holds back
/// `--output` and `--no-index` for every session, which these rules would otherwise take in.
const READ_GIT: [&str; 4] = [
    "Bash(git status:*)",
    "Bash(git diff:*)",
    "Bash(git log:*)",
    "Bash(git show:*)",
];

fn rules(lists: &[&[&str]]) -> Vec<String> {
    lists
        .iter()
        .flat_map(|list| list.iter())
        .map(|rule| (*rule).to_owned())
        .collect()
}

/// The role every existing Studio agent is given (CP-A1 migration) and every fresh install starts with.
///
/// Without an engine: a session already carries the engine it was started with, and a new session picks one.
pub fn default_role() -> Role {
    Role {
        name: "allrounder".into(),
        description: "Does whatever the card asks for".into(),
        kind: "implement".into(),
        tier: Tier::Medium,
        engine: None,
        fallback_engines: Vec::new(),
        permissions: rules(&[&BUILD_AND_TEST, &READ_GIT]),
        limits: Limits::default(),
        creates: Vec::new(),
        instructions: "Read the card, do what it asks for in your own worktree, and keep to its acceptance \
                       criteria. Check your inbox before you start, between steps and before you report done."
            .into(),
        source: Source::User,
    }
}

/// The role that judges cards (A21, A24): kind `review`, without an engine — the studio picks one that is not the engine
/// of the session that did the work, or asks. The text is what a reviewer is told about *how* to review; what to review
/// and against what comes in the start prompt.
pub fn reviewer_role() -> Role {
    Role {
        name: "reviewer".into(),
        description: "Judges the work another session did on a card".into(),
        kind: "review".into(),
        tier: Tier::Medium,
        engine: None,
        fallback_engines: Vec::new(),
        // Reads with git only. A build or a test runs the code under review — `build.rs`, a proc-macro, a test config
        // — as
        // the owner, and a reviewer reads text that may try to talk it into that; whoever wants its reviewers to run
        // the
        // tests adds `BUILD_AND_TEST`-style rules to the role file, knowing that.
        permissions: rules(&[&READ_GIT]),
        limits: Limits::default(),
        creates: Vec::new(),
        instructions: "You judge work somebody else did; you do not do it again. The card's acceptance criteria are the \
                       standard, not your taste: approve when they are met and you found no defect you would not want \
                       on the main branch, and return the card otherwise. Read the changed files in their context, not \
                       only the diff, and run what can be run without changing anything. A note that returns a card \
                       names what is wrong and how to see it (file, line, command), in a few lines, so the worker can \
                       fix it without asking you. Do not fix the work yourself, and do not write to the worker apart \
                       from the verdict — the studio passes your note on."
            .into(),
        source: Source::User,
    }
}

/// The role that plans (A5, A24): kind `plan`, without an engine — the owner picks the model at every start. It works
/// in a
/// read-only checkout of the project and makes cards; what it makes and for which plan comes through `get_plan`.
pub fn planner_role() -> Role {
    Role {
        name: "planner".into(),
        description: "Cuts a plan's goal into cards for the other roles".into(),
        kind: "plan".into(),
        tier: Tier::Heavy,
        engine: None,
        fallback_engines: Vec::new(),
        permissions: rules(&[&READ_GIT]),
        limits: Limits::default(),
        creates: Vec::new(),
        instructions: "You plan; you do not build. Read the plan's goal and the catalog of roles with `get_plan`, then \
                       read the project — your checkout is read-only — as far as you need to cut the work well. A card \
is one concern a single session can finish and a reviewer can judge in one sitting: a title that \
                       says what changes, a body that says why, and acceptance criteria that can be checked without \
                       asking you (a command to run, a behaviour to see) and are proportionate: a change that cannot break the \
                       build or the tests (documentation) gets no build or test criterion — its check is reading the \
                       diff. Name for every card the role that should do \
                       it (`agent`, from the catalog) and why in a sentence (`agent_reason`); set `tier` by how hard \
                       the card is, not how long, and `kind` by the sort of work (implement, test, doc). Every card is reviewed \
                       automatically once its session reports it done, so never make a card for reviewing. Order the \
                       work with `needs`: a card that cannot start before another one is merged names it, and nothing \
                       else does. Every card is a session that costs money, so a few well-cut cards beat many small \
                       ones. Your cards are proposals: the owner reads them and approves the plan, and you start and \
                       change nothing else. When you are done, say in a few lines what the plan consists of and in \
                       what order it should run."
            .into(),
        source: Source::User,
    }
}

/// The role that grills a plan's proposed cards (`docs/plans/a2a.md`, "Plan bearbeiten und grillen"): kind `grill`,
/// without
/// an engine — the owner picks the model at every start, as for the planner. It runs *after* the planner has proposed
/// the
/// cards, as an interactive session: it interviews the owner in its terminal, in rounds, about what the cards leave
/// open, and
/// hands what was agreed to the planner by mail — the planner owns the proposals and is the only one that mends them.
/// The
/// method is the `grilling` skill's; it is the role's text because a skill run is a single turn and an interview is
/// not.
pub fn grill_role() -> Role {
    Role {
        name: "grill".into(),
        description: "Questions the cards a planner proposed until nothing is left open; the planner mends them".into(),
        kind: "grill".into(),
        tier: Tier::Heavy,
        engine: None,
        fallback_engines: Vec::new(),
        permissions: rules(&[&READ_GIT]),
        limits: Limits::default(),
        creates: Vec::new(),
        instructions: GRILL_INSTRUCTIONS.into(),
        source: Source::User,
    }
}

/// The grilling method, as the text of the `grill` role.
const GRILL_INSTRUCTIONS: &str = "\
    You interview the owner about the cards a planner proposed for a plan, until you share one understanding of \
     them; you do not plan and you build nothing. Read the goal and the proposed cards with `get_plan` and look \
     at the project (your checkout is read-only) — finding facts is your job, never the owner's: do not ask what \
     the code or the files can tell you. Look for what the cards leave open: a card that is too big or too vague \
     to be checked, acceptance criteria nobody could test, a missing or wrong order between cards, a role or tier\
      that does not fit the work, work the goal needs and no card does, a card the goal does not need. Work in \
     rounds: ask everything whose answer does not depend on another open question in one round, numbered, each \
     question with your recommended answer and why, and wait for the owner's answers before the next round. Ask \
     in the language the goal is written in. You are done when nothing is left open and the owner agrees. You \
     change no card yourself — the planner owns its proposals. Find it with `list_agents` and send it the agreed \
     changes with `send_message`, one message, card by card (what to change, in which words); it mends its \
     proposals with `update_proposal` and `withdraw_proposal` and adds missing cards with `create_card`. Then \
     tell the owner what you sent. If the planner is not running any more, give the owner the changes as a list \
     instead — they edit the proposals in the plan panel. If the goal itself turns out to be the problem, say so \
     and, when the owner agrees, call `propose_goal` once.";

/// A role that works cards, in one of three strengths (`docs/plans/a2a.md`, "Rollen und Engines"): the same job at the tier
/// the card calls for. Without an engine — the first start asks once and saves the pick into the role.
fn working_role(name: &str, description: &str, tier: Tier, kind: &str, instructions: &str) -> Role {
    Role {
        name: name.into(),
        description: description.into(),
        kind: kind.into(),
        tier,
        engine: None,
        fallback_engines: Vec::new(),
        // A documenter reads the diff and builds nothing (its text says so); every other working role builds and tests.
        permissions: if kind == "doc" {
            rules(&[&READ_GIT])
        } else {
            rules(&[&BUILD_AND_TEST, &READ_GIT])
        },
        limits: Limits::default(),
        creates: Vec::new(),
        instructions: instructions.into(),
        source: Source::User,
    }
}

/// The light implementer: small, well-specified changes — a rename, a missing guard, a text, a small component — done
/// exactly as the card says. Cheap and fast; a card that turns out larger is sent back (and, after two returns, to a
/// stronger role).
pub fn implementer_light_role() -> Role {
    working_role(
        "implementer-light",
        "Small, well-specified changes, exactly as the card says",
        Tier::Light,
        "implement",
        "You make small, well-specified changes. Do exactly what the card asks and nothing around it: no refactoring you were \
         not asked for, no new abstractions, no renames beyond the card. Follow the project's own conventions \
         (AGENTS.md, CLAUDE.md, the code around yours) rather than your habits. If the card turns out to need a design \
         decision, a change across several modules or something its criteria do not cover, stop and say so in your report \
         instead of guessing: a stronger role takes it from there.",
    )
}

/// The heavy implementer: cross-cutting or delicate work — a change that touches several modules, concurrency, data
/// migrations, security-relevant code. The target of the escalation from the lighter roles.
pub fn implementer_heavy_role() -> Role {
    working_role(
        "implementer-heavy",
        "Cross-cutting or delicate changes: several modules, concurrency, migrations, security",
        Tier::Heavy,
        "implement",
        "You take the cards that need judgement: a change across several modules, concurrency, a data migration, \
         security-relevant code, or work an earlier session got wrong. Read the code and its tests first, and the history \
         of the card if it was sent back — it says what was wrong. Work out the design before you touch a file, keep the \
         change as small as the problem allows, and say in your report what you decided and why. Write the tests that \
         would have caught the problem. Follow the project's conventions (AGENTS.md, CLAUDE.md). If acceptance criteria \
         contradict each other, report that instead of satisfying one of them.",
    )
}

/// The documenter: documentation, comments and README text — not code. Light by default: reading the diff is its check.
pub fn documenter_role() -> Role {
    working_role(
        "documenter",
        "Writes documentation, doc comments and README sections; changes no behaviour",
        Tier::Light,
        "doc",
        "You write documentation and change no behaviour. Read the code you document and write what it does and why — for the \
         next reader, not the author. Public items get the project's doc-comment format (rustdoc, TSDoc, docstrings as the \
         project uses), a module gets a short purpose and usage, and a comment explains *why*, never what the code already \
         says. Keep to the language and the line length the project's AGENTS.md or CLAUDE.md names. Do not run builds or \
         tests for a text change; reading the diff is its check. Never document what you did not verify in the code.",
    )
}

/// The tester: tests and their gaps. Medium: a test that cannot fail is worse than none.
pub fn tester_role() -> Role {
    working_role(
        "tester",
        "Writes tests and closes gaps in them; changes no behaviour",
        Tier::Medium,
        "test",
        "You write tests and change no production behaviour. Find what the code does that no test would notice if it broke, \
         and test that: the edge cases, the refusals, the errors — assert on what is returned or stored, not on text that \
         could change. A test touches no real user state (home folder, network) and runs alone and in any order. Run the \
         tests you wrote, and the ones around them. A test that cannot fail is worse than none: make sure yours fails when \
         the behaviour it guards is removed. If you find a bug, report it with the failing test instead of fixing it \
         silently.",
    )
}

fn reviewer_with(name: &str, description: &str, tier: Tier, focus: &str) -> Role {
    let mut role = reviewer_role();
    role.name = name.into();
    role.description = description.into();
    role.tier = tier;
    role.instructions = format!("{}\n\n{focus}", role.instructions);
    role
}

/// The light reviewer: judges small cards against their criteria — what was asked, nothing less, nothing more.
pub fn reviewer_light_role() -> Role {
    reviewer_with(
        "reviewer-light",
        "Judges small cards against their acceptance criteria",
        Tier::Light,
        "This card is small: check that it does what the criteria say and nothing around it, that it builds or reads \
         correctly where that can be seen at once, and that no unrelated file changed. Do not hold a small change to the \
         standard of a redesign.",
    )
}

/// The heavy reviewer: for cards where being wrong is expensive — design, concurrency, security, migrations.
pub fn reviewer_heavy_role() -> Role {
    reviewer_with(
        "reviewer-heavy",
        "Judges delicate cards: design, concurrency, security, migrations",
        Tier::Heavy,
        "This card is delicate: besides the criteria, look for what the author may not have thought of — a state that is \
         not handled, a race, an input that is not checked, a migration that cannot be undone, a refusal that is missing. \
         Read the code the change touches, not just the diff, and run the tests that can show a problem. Return the card \
         for a defect you can name; do not return it for taste.",
    )
}

/// Seeds the roles every install starts with — the allrounder, the reviewers, the planner, the grill, the implementers of
/// two other strengths, the documenter and the tester — where they are missing — never overwrites, like the
/// bundled skills.
/// Returns whether it wrote anything.
///
/// # Errors
///
/// As [`save_role`].
pub fn seed_default_roles(dir: &Path) -> Result<bool> {
    let mut wrote = false;
    for role in [
        default_role(),
        reviewer_role(),
        planner_role(),
        grill_role(),
        implementer_light_role(),
        implementer_heavy_role(),
        documenter_role(),
        tester_role(),
        reviewer_light_role(),
        reviewer_heavy_role(),
    ] {
        if fs::symlink_metadata(dir.join(&role.name)).is_ok() {
            continue;
        }
        save_role(dir, &role)?;
        wrote = true;
    }
    Ok(wrote)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static N: AtomicU32 = AtomicU32::new(0);

    pub(crate) struct Tmp(pub PathBuf);

    impl Tmp {
        pub(crate) fn new() -> Tmp {
            let path = std::env::temp_dir().join(format!(
                "axiomata-roster-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Tmp(path)
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write(dir: &Path, name: &str, text: &str) {
        fs::create_dir_all(dir.join(name)).unwrap();
        fs::write(dir.join(name).join(ROLE_FILE), text).unwrap();
    }

    #[test]
    fn a_missing_directory_is_empty() {
        let tmp = Tmp::new();
        let loaded = load_roles(&tmp.0.join("nope"), Source::User).unwrap();
        assert!(loaded.roles.is_empty() && loaded.skipped.is_empty());
    }

    #[test]
    fn saved_roles_load_sorted_and_stamped_with_their_source() {
        let tmp = Tmp::new();
        let mut b = default_role();
        b.name = "b-role".into();
        save_role(&tmp.0, &b).unwrap();
        save_role(&tmp.0, &default_role()).unwrap();
        let loaded = load_roles(&tmp.0, Source::Project).unwrap();
        let names: Vec<_> = loaded.roles.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["allrounder", "b-role"]);
        assert!(loaded.roles.iter().all(|r| r.source == Source::Project));
        assert!(loaded.skipped.is_empty());
    }

    #[test]
    fn one_bad_file_is_reported_and_does_not_hide_the_good_ones() {
        let tmp = Tmp::new();
        save_role(&tmp.0, &default_role()).unwrap();
        write(&tmp.0, "broken", "no frontmatter");
        write(&tmp.0, "mismatch", "---\nname: other\nkind: plan\n---\n");
        write(&tmp.0, "huge", &"x".repeat(70 * 1024));
        fs::create_dir_all(tmp.0.join("empty")).unwrap();
        fs::write(tmp.0.join("stray.txt"), "ignored").unwrap();
        let loaded = load_roles(&tmp.0, Source::User).unwrap();
        assert_eq!(loaded.roles.len(), 1);
        let skipped: Vec<_> = loaded.skipped.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(skipped, ["broken", "empty", "huge", "mismatch"]);
        assert!(loaded.skipped[3].reason.contains("does not match"));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_directories_and_files_are_never_followed() {
        let tmp = Tmp::new();
        let secret = tmp.0.join("outside");
        fs::create_dir_all(&secret).unwrap();
        fs::write(secret.join(ROLE_FILE), default_role().to_markdown()).unwrap();
        let roles = tmp.0.join("roles");
        fs::create_dir_all(&roles).unwrap();
        std::os::unix::fs::symlink(&secret, roles.join("allrounder")).unwrap();
        fs::create_dir_all(roles.join("linked-file")).unwrap();
        std::os::unix::fs::symlink(
            secret.join(ROLE_FILE),
            roles.join("linked-file").join(ROLE_FILE),
        )
        .unwrap();
        let loaded = load_roles(&roles, Source::User).unwrap();
        assert!(loaded.roles.is_empty());
        assert_eq!(loaded.skipped.len(), 2);
        assert!(matches!(
            save_role(&roles, &default_role()),
            Err(RosterError::Invalid { field: "name", .. })
        ));
        assert!(matches!(
            delete_role(&roles, "allrounder"),
            Err(RosterError::Invalid { field: "name", .. })
        ));
        assert!(
            secret.join(ROLE_FILE).is_file(),
            "the target of the link must survive"
        );
    }

    #[test]
    fn deleting_removes_the_directory_but_keeps_foreign_files() {
        let tmp = Tmp::new();
        save_role(&tmp.0, &default_role()).unwrap();
        assert!(delete_role(&tmp.0, "allrounder").unwrap());
        assert!(!tmp.0.join("allrounder").exists());
        assert!(!delete_role(&tmp.0, "allrounder").unwrap());

        save_role(&tmp.0, &default_role()).unwrap();
        fs::write(tmp.0.join("allrounder").join("notes.md"), "mine").unwrap();
        assert!(delete_role(&tmp.0, "allrounder").unwrap());
        assert!(tmp.0.join("allrounder").join("notes.md").is_file());
        assert!(matches!(
            delete_role(&tmp.0, "../x"),
            Err(RosterError::Invalid { field: "name", .. })
        ));
    }

    #[test]
    fn seeding_never_overwrites() {
        let tmp = Tmp::new();
        assert!(seed_default_roles(&tmp.0).unwrap());
        let mut edited = default_role();
        edited.description = "my own words".into();
        save_role(&tmp.0, &edited).unwrap();
        assert!(!seed_default_roles(&tmp.0).unwrap());
        let loaded = load_roles(&tmp.0, Source::User).unwrap();
        let allrounder = loaded
            .roles
            .iter()
            .find(|r| r.name == "allrounder")
            .unwrap();
        assert_eq!(allrounder.description, "my own words");
    }

    #[test]
    fn the_reviewer_is_seeded_next_to_the_allrounder_and_is_a_valid_role_that_judges() {
        let tmp = Tmp::new();
        assert!(seed_default_roles(&tmp.0).unwrap());
        let loaded = load_roles(&tmp.0, Source::User).unwrap();
        let names: Vec<_> = loaded.roles.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"allrounder") && names.contains(&"reviewer"),
            "{names:?}"
        );
        let reviewer = loaded.roles.iter().find(|r| r.name == "reviewer").unwrap();
        assert_eq!(reviewer.kind, "review");
        assert_eq!(
            reviewer.engine, None,
            "the studio picks an engine that is not the worker's"
        );
        assert!(reviewer.creates.is_empty());
        assert!(
            reviewer
                .permissions
                .iter()
                .all(|rule| rule.starts_with("Bash(git ")),
            "a reviewer reads with git and runs nothing of the work it judges"
        );
        // A role that was seeded earlier gets the other one without losing its own edits.
        fs::remove_dir_all(tmp.0.join("reviewer")).unwrap();
        assert!(seed_default_roles(&tmp.0).unwrap());
        assert!(!seed_default_roles(&tmp.0).unwrap());
    }

    #[test]
    fn the_planner_is_seeded_as_a_valid_role_that_plans_and_that_picks_its_engine_at_every_start() {
        let tmp = Tmp::new();
        assert!(seed_default_roles(&tmp.0).unwrap());
        let loaded = load_roles(&tmp.0, Source::User).unwrap();
        let planner = loaded.roles.iter().find(|r| r.name == "planner").unwrap();
        assert_eq!(planner.kind, "plan");
        assert_eq!(planner.engine, None);
        assert!(planner.instructions.contains("get_plan"));
        // A planner of an install that has the other two gets seeded without touching them.
        fs::remove_dir_all(tmp.0.join("planner")).unwrap();
        assert!(seed_default_roles(&tmp.0).unwrap());
    }

    #[test]
    fn the_grill_is_seeded_as_a_role_that_interviews_and_mails_the_planner_and_picks_its_engine_at_every_start()
     {
        let tmp = Tmp::new();
        assert!(seed_default_roles(&tmp.0).unwrap());
        let loaded = load_roles(&tmp.0, Source::User).unwrap();
        let grill = loaded.roles.iter().find(|r| r.name == "grill").unwrap();
        assert_eq!(grill.kind, "grill");
        assert_eq!(grill.engine, None);
        assert!(grill.creates.is_empty());
        assert!(
            grill
                .permissions
                .iter()
                .all(|rule| rule.starts_with("Bash(git "))
        );
        assert!(
            grill.instructions.contains("propose_goal")
                && grill.instructions.contains("send_message")
                && grill.instructions.contains("`get_plan`")
        );
        // An install that has the others gets the grill without touching them.
        fs::remove_dir_all(tmp.0.join("grill")).unwrap();
        assert!(seed_default_roles(&tmp.0).unwrap());
        assert!(!seed_default_roles(&tmp.0).unwrap());
    }

    #[test]
    fn the_seeded_set_has_the_working_roles_in_their_strengths_and_the_reviewers_to_match() {
        let tmp = Tmp::new();
        assert!(seed_default_roles(&tmp.0).unwrap());
        let loaded = load_roles(&tmp.0, Source::User).unwrap();
        assert!(
            loaded.skipped.is_empty(),
            "every seeded role is a valid one: {:?}",
            loaded.skipped
        );
        let role = |name: &str| loaded.roles.iter().find(|r| r.name == name).unwrap();
        let shape = |name: &str| (role(name).kind.clone(), role(name).tier);
        assert_eq!(
            shape("implementer-light"),
            ("implement".to_owned(), Tier::Light)
        );
        assert_eq!(shape("allrounder"), ("implement".to_owned(), Tier::Medium));
        assert_eq!(
            shape("implementer-heavy"),
            ("implement".to_owned(), Tier::Heavy)
        );
        assert_eq!(shape("documenter"), ("doc".to_owned(), Tier::Light));
        assert_eq!(shape("tester"), ("test".to_owned(), Tier::Medium));
        assert_eq!(shape("reviewer-light"), ("review".to_owned(), Tier::Light));
        assert_eq!(shape("reviewer"), ("review".to_owned(), Tier::Medium));
        assert_eq!(shape("reviewer-heavy"), ("review".to_owned(), Tier::Heavy));
        // None names an engine: the first start asks once and saves it into the role.
        assert!(loaded.roles.iter().all(|r| r.engine.is_none()));
        // The roles that read only (planner, grill, documenter, reviewers) get git reads and nothing that runs the
        // project;
        // the workers may also build and test.
        let permissions = |name: &str| role(name).permissions.clone();
        for name in ["planner", "grill"] {
            assert!(
                permissions(name)
                    .iter()
                    .all(|rule| rule.starts_with("Bash(git ")),
                "{name} only reads"
            );
        }
        assert!(
            permissions("documenter")
                .iter()
                .all(|rule| rule.starts_with("Bash(git "))
        );
        for name in ["implementer-light", "implementer-heavy", "tester"] {
            assert!(
                permissions(name)
                    .iter()
                    .any(|rule| rule == "Bash(cargo test:*)"),
                "{name} runs the tests without asking"
            );
        }
        // A reviewer runs nothing of the work it judges without asking: it only reads with git.
        for name in ["reviewer", "reviewer-light", "reviewer-heavy"] {
            assert!(
                permissions(name)
                    .iter()
                    .all(|rule| rule.starts_with("Bash(git ")),
                "{name} reads with git only"
            );
        }
        // A reviewer keeps the review method and adds what its strength looks for.
        assert!(
            role("reviewer-heavy")
                .instructions
                .contains("review_verdict")
                == role("reviewer").instructions.contains("review_verdict")
        );
        assert!(role("reviewer-heavy").instructions.contains("delicate"));
        assert!(role("reviewer-light").instructions.contains("small"));
    }

    #[test]
    fn saving_validates_first() {
        let tmp = Tmp::new();
        let mut bad = default_role();
        bad.kind = "Not A Slug".into();
        assert!(matches!(
            save_role(&tmp.0, &bad),
            Err(RosterError::Invalid { field: "kind", .. })
        ));
        assert!(
            fs::read_dir(&tmp.0).unwrap().next().is_none(),
            "nothing may be written for an invalid role"
        );
    }

    #[test]
    fn more_role_directories_than_the_limit_are_not_read_and_the_rest_is_reported() {
        let tmp = Tmp::new();
        for n in 0..(MAX_ENTRIES + 6) {
            write(&tmp.0, &format!("r{n:03}"), "not a role");
        }
        let loaded = load_roles(&tmp.0, Source::User).unwrap();
        assert_eq!(
            loaded.skipped.len(),
            MAX_ENTRIES + 1,
            "every read directory, plus the marker"
        );
        assert_eq!(loaded.skipped.last().unwrap().name, TRUNCATED_NAME);
    }

    #[test]
    fn a_fifo_or_other_special_file_in_place_of_a_role_file_is_skipped_without_blocking() {
        let tmp = Tmp::new();
        fs::create_dir_all(tmp.0.join("odd")).unwrap();
        // A named pipe would block a plain `open` forever.
        let fifo = tmp.0.join("odd").join(ROLE_FILE);
        let status = std::process::Command::new("mkfifo").arg(&fifo).status();
        if status.is_ok_and(|s| s.success()) {
            let loaded = load_roles(&tmp.0, Source::User).unwrap();
            assert_eq!(loaded.skipped.len(), 1);
            assert!(
                loaded.skipped[0].reason.contains("not a regular file"),
                "{:?}",
                loaded.skipped[0]
            );
        }
    }

    #[test]
    fn a_stale_temporary_file_that_is_a_symlink_is_replaced_not_written_through() {
        let tmp = Tmp::new();
        let target = tmp.0.join("victim.txt");
        fs::write(&target, "untouched").unwrap();
        let dir = tmp.0.join("roles").join("allrounder");
        fs::create_dir_all(&dir).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, dir.join(format!("{ROLE_FILE}.tmp"))).unwrap();
        save_role(&tmp.0.join("roles"), &default_role()).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "untouched");
        assert!(
            load_roles(&tmp.0.join("roles"), Source::User)
                .unwrap()
                .roles
                .len()
                == 1
        );
    }
}
