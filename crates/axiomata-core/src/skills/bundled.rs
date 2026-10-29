//! The bundled skills against their installed copies: up to date, outdated, missing, or the owner's own edit.
//!
//! The seed is seed-if-absent ([`super::seed_default_skills`]), so a newer bundled `SKILL.md` never reaches an install
//! by itself. To tell "an old copy nobody touched" from "the owner tuned it", every write of a bundled skill records
//! the SHA-256 of what it wrote in `~/.axiomata/bundled-skills.json` ([`paths::bundled_skills_record_path`]); the
//! installed copy is then compared with both the bundled text and that record. Only names in
//! [`super::DEFAULT_SKILLS`] are ever read or written here — the owner's own skills are not this module's.

use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Write};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{DEFAULT_SKILLS, write_skill};
use crate::error::AxiomataError;
use crate::paths;

/// Where an installed bundled skill stands against the version bundled with this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundledState {
    /// The installed `SKILL.md` is the bundled one, byte for byte.
    UpToDate,
    /// The installed copy is what was seeded earlier, untouched, and this build bundles a newer one.
    Outdated,
    /// No `SKILL.md` is installed under the skill's name.
    Missing,
    /// The installed copy differs from both the bundled version and what was seeded: the owner edited it.
    Changed,
    /// The installed copy differs from the bundled version and nothing records what was seeded (a seed from before
    /// the record existed): it may be the owner's own edit, so it is treated like [`Self::Changed`].
    Differs,
}

impl BundledState {
    /// Whether an update would replace something the owner may have written, and so must be confirmed first.
    pub fn is_owners(self) -> bool {
        matches!(self, Self::Changed | Self::Differs)
    }

    /// Whether "update all outdated" brings this one up to date: only what holds nothing of the owner's.
    pub fn needs_update(self) -> bool {
        matches!(self, Self::Outdated | Self::Missing)
    }

    /// The short word the CLI prints.
    pub fn label(self) -> &'static str {
        match self {
            Self::UpToDate => "up to date",
            Self::Outdated => "outdated",
            Self::Missing => "missing",
            Self::Changed => "changed by you",
            Self::Differs => "differs (may be yours)",
        }
    }
}

/// One bundled skill and its state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundledSkill {
    pub name: String,
    pub state: BundledState,
}

/// SHA-256 of `bytes`, lower-case hex.
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The pure decision, apart from the disk: `installed` is `None` when no copy is there, `recorded` when nothing
/// records the seed. All three are hashes.
fn classify(bundled: &str, installed: Option<&str>, recorded: Option<&str>) -> BundledState {
    match (installed, recorded) {
        (None, _) => BundledState::Missing,
        (Some(installed), _) if installed == bundled => BundledState::UpToDate,
        (Some(installed), Some(recorded)) if installed == recorded => BundledState::Outdated,
        (Some(_), Some(_)) => BundledState::Changed,
        (Some(_), None) => BundledState::Differs,
    }
}

/// What is installed under `name`: `None` when there is no `SKILL.md`. A skill folder or manifest that is a link, or
/// a manifest that cannot be read, reads as a hash no bundled text can have: it shows as the owner's (asked before it
/// is replaced, a linked folder never written through) instead of failing the whole list.
fn installed_hash(name: &str) -> Option<String> {
    let dir = paths::global_skills_dir().join(name);
    let manifest = dir.join("SKILL.md");
    let linked = |path: &std::path::Path| {
        fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
    };
    if linked(&dir) || linked(&manifest) {
        return Some("link".to_owned());
    }
    match fs::read(&manifest) {
        Ok(bytes) => Some(hash(&bytes)),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => {
            tracing::warn!(path = %manifest.display(), %err, "cannot read an installed bundled skill");
            Some("unreadable".to_owned())
        }
    }
}

/// The record of what each bundled skill was last written with, by name.
///
/// The app, a `cargo tauri dev` and the CLI may all write it, so a save re-reads the file and lays only this
/// process's own notes on top, rather than writing back a copy that may be stale by then.
#[derive(Debug, Default)]
pub(crate) struct Record {
    hashes: BTreeMap<String, String>,
    noted: BTreeMap<String, String>,
}

/// Reads the record file: `Ok(empty)` when there is none or it does not parse — every skill then reads as seeded
/// before the record, which errs on the owner's side ("may be yours"), never towards an overwrite.
fn read_record() -> Result<BTreeMap<String, String>, AxiomataError> {
    let path = paths::bundled_skills_record_path();
    match fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_else(|err| {
            tracing::warn!(path = %path.display(), %err, "ignoring an unparsable bundled-skills record");
            BTreeMap::new()
        })),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(source) => Err(AxiomataError::Io { path, source }),
    }
}

impl Record {
    /// Reads the record for deciding states; a file that cannot be read is logged and reads as empty (see
    /// [`read_record`]). A save then reads it again and fails rather than drop the other skills' hashes.
    pub(crate) fn load() -> Self {
        let hashes = read_record().unwrap_or_else(|err| {
            tracing::warn!(%err, "cannot read the bundled-skills record");
            BTreeMap::new()
        });
        Self {
            hashes,
            noted: BTreeMap::new(),
        }
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.noted
            .get(name)
            .or_else(|| self.hashes.get(name))
            .map(String::as_str)
    }

    /// Records that `content` was just written as `name`.
    pub(crate) fn note(&mut self, name: &str, content: &str) {
        let wanted = hash(content.as_bytes());
        if self.get(name) != Some(wanted.as_str()) {
            self.noted.insert(name.to_owned(), wanted);
        }
    }

    /// Records `content` for `name` when the installed copy is exactly it — then it is untouched for sure.
    pub(crate) fn note_if_current(&mut self, name: &str, content: &str) {
        if installed_hash(name).is_some_and(|installed| installed == hash(content.as_bytes())) {
            self.note(name, content);
        }
    }

    /// Writes this process's notes into the record, if there are any: the file is read again, the notes go on top,
    /// and an `O_EXCL` temp file (named by the process, so two writers never take each other's) is renamed over the
    /// old one — a crash never leaves half a record and a link planted at the temp path is not followed.
    pub(crate) fn save_if_changed(&mut self) -> Result<(), AxiomataError> {
        if self.noted.is_empty() {
            return Ok(());
        }
        let mut hashes = read_record()?;
        hashes.extend(self.noted.clone());
        let path = paths::bundled_skills_record_path();
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        let io = |path: &std::path::Path| {
            let path = path.to_path_buf();
            move |source| AxiomataError::Io { path, source }
        };
        // A temp file left by a crashed write of this pid would wedge the save on `create_new`; removing it removes
        // a planted link itself, never what it points to.
        let _ = fs::remove_file(&tmp);
        let bytes = serde_json::to_vec_pretty(&hashes).map_err(|err| AxiomataError::Io {
            path: path.clone(),
            source: std::io::Error::other(err),
        })?;
        let written = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .and_then(|mut file| file.write_all(&bytes));
        if let Err(source) = written {
            let _ = fs::remove_file(&tmp);
            return Err(io(&tmp)(source));
        }
        fs::rename(&tmp, &path).map_err(|source| {
            let _ = fs::remove_file(&tmp);
            io(&path)(source)
        })?;
        self.hashes = hashes;
        self.noted.clear();
        Ok(())
    }
}

fn state_of(name: &str, content: &str, record: &Record) -> BundledState {
    classify(
        &hash(content.as_bytes()),
        installed_hash(name).as_deref(),
        record.get(name),
    )
}

/// Every bundled skill with its state, in the bundled order. A copy that cannot be read is listed as the owner's
/// rather than failing the list, so one broken file never hides the others' updates.
///
/// Errors:
///     None today; kept a `Result` so a future reader of the disk can report one without changing the API.
pub fn status() -> Result<Vec<BundledSkill>, AxiomataError> {
    let record = Record::load();
    Ok(DEFAULT_SKILLS
        .iter()
        .map(|(name, content)| BundledSkill {
            name: (*name).to_owned(),
            state: state_of(name, content, &record),
        })
        .collect())
}

/// Brings one bundled skill up to date: writes the bundled `SKILL.md` over the installed one and records its hash.
///
/// `replace_own_edit` must be set when the installed copy is [`BundledState::Changed`] or [`BundledState::Differs`]
/// — the app asks the owner first, and this is the guard behind that question. Returns the skill's new state.
///
/// Errors:
///     [`AxiomataError::BundledSkill`] if `name` is no bundled skill, or the copy is the owner's and
///     `replace_own_edit` is not set, or its folder is a link; [`AxiomataError::Io`] if the write or the record
///     fails.
pub fn update(name: &str, replace_own_edit: bool) -> Result<BundledSkill, AxiomataError> {
    let refused = |reason: &str| AxiomataError::BundledSkill {
        name: name.to_owned(),
        reason: reason.to_owned(),
    };
    let (name, content) = DEFAULT_SKILLS
        .iter()
        .find(|(bundled, _)| *bundled == name)
        .ok_or_else(|| refused("not a bundled skill; the owner's own skills are never replaced"))?;
    let mut record = Record::load();
    let state = state_of(name, content, &record);
    if state.is_owners() && !replace_own_edit {
        return Err(refused(
            "the installed copy may hold your own edits; confirm to replace it",
        ));
    }
    if state != BundledState::UpToDate {
        write_skill(name, content)?;
    }
    record.note(name, content);
    record.save_if_changed()?;
    Ok(BundledSkill {
        name: (*name).to_owned(),
        state: BundledState::UpToDate,
    })
}

/// Brings every [`BundledState::Outdated`] and [`BundledState::Missing`] bundled skill up to date — never one that
/// may hold the owner's edits — and returns the fresh state of all of them. A skill that turned into the owner's
/// between the list and its update (an edit meanwhile, a link) is passed by; the next list shows it as such.
///
/// Errors:
///     [`AxiomataError::Io`] if a write or the record fails; aborts there (what was written stays, with its hash).
pub fn update_outdated() -> Result<Vec<BundledSkill>, AxiomataError> {
    for skill in status()? {
        if skill.state.needs_update() {
            match update(&skill.name, false) {
                Ok(_) | Err(AxiomataError::BundledSkill { .. }) => {}
                Err(err) => return Err(err),
            }
        }
    }
    status()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::env;
    use std::path::PathBuf;

    /// A scratch `AXIOMATA_HOME` for one test, held with the environment lock.
    struct Home {
        dir: PathBuf,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl Home {
        fn new(tag: &str) -> Self {
            let guard = ENV_MUTEX
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let dir = unique_temp_dir(tag);
            fs::create_dir_all(&dir).unwrap();
            // SAFETY: serialized by the guard held for the test's lifetime, see `paths::tests`.
            unsafe { env::set_var(paths::AXIOMATA_HOME_ENV, &dir) };
            Self { dir, _guard: guard }
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            // SAFETY: still serialized by the guard.
            unsafe { env::remove_var(paths::AXIOMATA_HOME_ENV) };
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn manifest(name: &str) -> PathBuf {
        paths::global_skills_dir().join(name).join("SKILL.md")
    }

    fn state(name: &str) -> BundledState {
        status()
            .unwrap()
            .into_iter()
            .find(|skill| skill.name == name)
            .unwrap()
            .state
    }

    fn bundled(name: &str) -> &'static str {
        DEFAULT_SKILLS.iter().find(|(n, _)| *n == name).unwrap().1
    }

    /// Makes the record claim `name` was seeded with `content` — what an older build would have left behind.
    fn record_as_seeded(name: &str, content: &str) {
        let mut record = Record::load();
        record.note(name, content);
        record.save_if_changed().unwrap();
    }

    #[test]
    fn classify_covers_every_state() {
        assert_eq!(classify("b", None, Some("b")), BundledState::Missing);
        assert_eq!(classify("b", Some("b"), None), BundledState::UpToDate);
        assert_eq!(
            classify("b", Some("b"), Some("old")),
            BundledState::UpToDate
        );
        assert_eq!(
            classify("b", Some("old"), Some("old")),
            BundledState::Outdated
        );
        assert_eq!(
            classify("b", Some("mine"), Some("old")),
            BundledState::Changed
        );
        assert_eq!(classify("b", Some("mine"), None), BundledState::Differs);
    }

    #[test]
    fn a_fresh_seed_is_up_to_date_and_recorded() {
        let _home = Home::new("axiomata-test-bundled-fresh");
        super::super::seed_default_skills().unwrap();
        assert!(
            status()
                .unwrap()
                .iter()
                .all(|s| s.state == BundledState::UpToDate)
        );
        let record = Record::load();
        for (name, content) in DEFAULT_SKILLS {
            assert_eq!(
                record.get(name),
                Some(hash(content.as_bytes()).as_str()),
                "{name}"
            );
        }
    }

    #[test]
    fn an_untouched_old_copy_is_outdated_and_an_edited_one_is_the_owners() {
        let _home = Home::new("axiomata-test-bundled-states");
        super::super::seed_default_skills().unwrap();
        // An older build seeded this text, and the owner never touched it.
        let old = "---\nname: mail-digest\ndescription: old\n---\n";
        fs::write(manifest("mail-digest"), old).unwrap();
        record_as_seeded("mail-digest", old);
        assert_eq!(state("mail-digest"), BundledState::Outdated);

        // The owner edits a copy after its seed.
        fs::write(
            manifest("cleanup"),
            "---\nname: cleanup\ndescription: mine\n---\n",
        )
        .unwrap();
        assert_eq!(state("cleanup"), BundledState::Changed);

        // A copy from before the record: nothing says what was seeded.
        let path = paths::bundled_skills_record_path();
        let mut hashes: BTreeMap<String, String> =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        hashes.remove("news-rss");
        fs::write(&path, serde_json::to_vec(&hashes).unwrap()).unwrap();
        fs::write(
            manifest("news-rss"),
            "---\nname: news-rss\ndescription: ?\n---\n",
        )
        .unwrap();
        assert_eq!(state("news-rss"), BundledState::Differs);

        fs::remove_dir_all(paths::global_skills_dir().join("inbox-sort")).unwrap();
        assert_eq!(state("inbox-sort"), BundledState::Missing);
    }

    #[test]
    fn the_seed_backfills_the_record_only_for_a_copy_equal_to_the_bundled_one() {
        let _home = Home::new("axiomata-test-bundled-backfill");
        super::super::seed_default_skills().unwrap();
        fs::remove_file(paths::bundled_skills_record_path()).unwrap();
        fs::write(
            manifest("cleanup"),
            "---\nname: cleanup\ndescription: mine\n---\n",
        )
        .unwrap();

        super::super::seed_default_skills().unwrap();
        let record = Record::load();
        assert!(
            record.get("mail-digest").is_some(),
            "an unchanged copy is recorded on the next start"
        );
        assert_eq!(
            record.get("cleanup"),
            None,
            "an edited copy is not claimed as seeded"
        );
        assert_eq!(state("cleanup"), BundledState::Differs);
    }

    #[test]
    fn update_replaces_an_outdated_copy_and_records_its_hash() {
        let _home = Home::new("axiomata-test-bundled-update");
        super::super::seed_default_skills().unwrap();
        let old = "---\nname: mail-digest\ndescription: old\n---\n";
        fs::write(manifest("mail-digest"), old).unwrap();
        record_as_seeded("mail-digest", old);

        let updated = update("mail-digest", false).unwrap();
        assert_eq!(updated.state, BundledState::UpToDate);
        assert_eq!(
            fs::read_to_string(manifest("mail-digest")).unwrap(),
            bundled("mail-digest")
        );
        assert_eq!(
            Record::load().get("mail-digest"),
            Some(hash(bundled("mail-digest").as_bytes()).as_str())
        );
    }

    #[test]
    fn update_asks_before_replacing_the_owners_edit() {
        let _home = Home::new("axiomata-test-bundled-confirm");
        super::super::seed_default_skills().unwrap();
        let mine = "---\nname: cleanup\ndescription: mine\n---\n";
        fs::write(manifest("cleanup"), mine).unwrap();

        let refused = update("cleanup", false).unwrap_err();
        assert!(
            matches!(refused, AxiomataError::BundledSkill { .. }),
            "{refused}"
        );
        assert_eq!(fs::read_to_string(manifest("cleanup")).unwrap(), mine);

        update("cleanup", true).unwrap();
        assert_eq!(
            fs::read_to_string(manifest("cleanup")).unwrap(),
            bundled("cleanup")
        );
        assert_eq!(state("cleanup"), BundledState::UpToDate);
    }

    #[test]
    fn update_outdated_skips_the_owners_edits_and_never_touches_own_skills() {
        let _home = Home::new("axiomata-test-bundled-all");
        super::super::seed_default_skills().unwrap();
        let old = "---\nname: mail-digest\ndescription: old\n---\n";
        fs::write(manifest("mail-digest"), old).unwrap();
        record_as_seeded("mail-digest", old);
        let mine = "---\nname: cleanup\ndescription: mine\n---\n";
        fs::write(manifest("cleanup"), mine).unwrap();
        fs::remove_dir_all(paths::global_skills_dir().join("news-web")).unwrap();
        let own_dir = paths::global_skills_dir().join("my-own");
        fs::create_dir_all(&own_dir).unwrap();
        let own = "---\nname: my-own\ndescription: mine\n---\n";
        fs::write(own_dir.join("SKILL.md"), own).unwrap();

        let after = update_outdated().unwrap();
        let of = |name: &str| after.iter().find(|s| s.name == name).unwrap().state;
        assert_eq!(of("mail-digest"), BundledState::UpToDate);
        assert_eq!(of("news-web"), BundledState::UpToDate);
        assert_eq!(of("cleanup"), BundledState::Changed);
        assert_eq!(fs::read_to_string(manifest("cleanup")).unwrap(), mine);
        assert_eq!(fs::read_to_string(own_dir.join("SKILL.md")).unwrap(), own);
        assert!(after.iter().all(|s| s.name != "my-own"));

        let refused = update("my-own", true).unwrap_err();
        assert!(matches!(refused, AxiomataError::BundledSkill { .. }));
        assert_eq!(fs::read_to_string(own_dir.join("SKILL.md")).unwrap(), own);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_skill_folder_reads_as_the_owners_and_is_not_written_through() {
        let _home = Home::new("axiomata-test-bundled-link");
        super::super::seed_default_skills().unwrap();
        let outside = paths::axiomata_home().join("elsewhere");
        fs::create_dir_all(&outside).unwrap();
        let mine = "---\nname: cleanup\ndescription: elsewhere\n---\n";
        fs::write(outside.join("SKILL.md"), mine).unwrap();
        let dir = paths::global_skills_dir().join("cleanup");
        fs::remove_dir_all(&dir).unwrap();
        std::os::unix::fs::symlink(&outside, &dir).unwrap();

        assert!(state("cleanup").is_owners());
        assert!(update("cleanup", true).is_err());
        assert_eq!(fs::read_to_string(outside.join("SKILL.md")).unwrap(), mine);
    }

    #[test]
    fn a_forced_reseed_records_what_it_wrote() {
        let _home = Home::new("axiomata-test-bundled-reseed");
        super::super::seed_default_skills().unwrap();
        fs::write(
            manifest("cleanup"),
            "---\nname: cleanup\ndescription: mine\n---\n",
        )
        .unwrap();
        fs::remove_file(paths::bundled_skills_record_path()).unwrap();

        super::super::reseed_default_skills(true).unwrap();
        assert!(
            status()
                .unwrap()
                .iter()
                .all(|s| s.state == BundledState::UpToDate)
        );
        assert_eq!(
            Record::load().get("cleanup"),
            Some(hash(bundled("cleanup").as_bytes()).as_str())
        );
    }

    #[test]
    fn a_save_keeps_what_another_writer_recorded_meanwhile() {
        let _home = Home::new("axiomata-test-bundled-merge");
        let mut mine = Record::load();
        mine.note("cleanup", bundled("cleanup"));
        // Another process records a skill after this one loaded.
        let mut theirs = Record::load();
        theirs.note("mail-digest", bundled("mail-digest"));
        theirs.save_if_changed().unwrap();

        mine.save_if_changed().unwrap();
        let record = Record::load();
        assert!(record.get("cleanup").is_some());
        assert!(
            record.get("mail-digest").is_some(),
            "the other writer's hash survived"
        );
    }

    #[test]
    fn an_unparsable_record_reads_as_empty_and_is_rewritten_by_the_next_save() {
        let _home = Home::new("axiomata-test-bundled-corrupt");
        super::super::seed_default_skills().unwrap();
        fs::write(paths::bundled_skills_record_path(), b"{ not json").unwrap();
        fs::write(
            manifest("cleanup"),
            "---\nname: cleanup\ndescription: mine\n---\n",
        )
        .unwrap();
        assert_eq!(
            state("cleanup"),
            BundledState::Differs,
            "no record errs on the owner's side"
        );

        super::super::seed_default_skills().unwrap();
        let record = Record::load();
        assert!(record.get("mail-digest").is_some());
        assert_eq!(record.get("cleanup"), None);
    }
}
