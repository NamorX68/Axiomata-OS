//! `axiomata-cli inbox move <file> <folder>` — the one move the `inbox-sort` skill makes (glut-round W8, security
//! audit): a note out of the vault's `Inbox/` into a folder that already exists, never overwriting, never through a
//! link. A permission rule can only name a command's first words, so `Bash(mv -n:*)` also let `mv -n -f a b` and any
//! destination through; this command checks the paths itself, and the skill may run nothing else.
//!
//! It runs in the working folder (the vault the skill runs in) and needs neither the core nor its database, so it
//! works in Glut's sandbox, which hides `~/.axiomata`.

use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

/// The vault's inbox folder.
const INBOX: &str = "Inbox";

/// Files a harness reads as instructions or configuration: a note of that name in an area would speak to every
/// later agent there. Compared without case, as APFS finds files.
const INSTRUCTION_FILES: [&str; 6] = [
    "AGENTS.md",
    "CLAUDE.md",
    "AGENT.md",
    "GEMINI.md",
    "CLAUDE.local.md",
    "opencode.json",
];

/// What a note may be; anything else (a script, a page, a configuration) stays in the inbox.
const NOTE_EXTENSIONS: [&str; 11] = [
    "md", "markdown", "txt", "pdf", "canvas", "png", "jpg", "jpeg", "gif", "webp", "heic",
];

/// Top-level folders the app writes and reads itself: a note there would be swept away (`Kanban/`, the board mirror)
/// or read as the app's own data. Compared without case.
const APP_FOLDERS: [&str; 4] = ["Inbox", "Kanban", "News", "Mail"];

/// The router that makes a top-level folder an area (`memory sync` writes it).
const AREA_ROUTER: &str = "AGENTS.md";

#[derive(Debug, Subcommand)]
pub enum InboxAction {
    /// Move `Inbox/<file>` into `<folder>` (relative to the working folder; it must exist). Never overwrites.
    Move {
        /// The note's file name inside `Inbox/`.
        file: String,
        /// The folder to move it into, such as `Arbeit` or `Learning/Rust`.
        folder: String,
    },
}

pub fn run(action: InboxAction) -> Result<()> {
    match action {
        InboxAction::Move { file, folder } => {
            let target = move_note(Path::new("."), &file, &folder)?;
            println!("moved {INBOX}/{file} → {}", target.display());
            Ok(())
        }
    }
}

/// Moves `<vault>/Inbox/<file>` to `<vault>/<folder>/<file>` and returns the target relative to the vault.
///
/// # Errors
///
/// `file` is no plain file name, an instruction file, no note, or no regular file in the inbox; `folder` leaves the
/// vault, names a hidden folder or one of the app's own, lies in no area (a top-level folder with its router), or is
/// no existing folder (a link counts as none); the target exists already.
fn move_note(vault: &Path, file: &str, folder: &str) -> Result<PathBuf> {
    let inbox = vault.join(INBOX);
    // The vault's root: its inbox and its own router (`memory sync`), so a shell that changed folder is no vault.
    if !is_real_dir(&inbox) || !is_file(&vault.join(AREA_ROUTER)) {
        bail!("there is no {INBOX}/ folder and {AREA_ROUTER} here; run this in the vault");
    }
    if file.is_empty() || file.starts_with('.') || file.contains(['/', '\\']) || file == ".." {
        bail!("{file:?} is no file name inside {INBOX}/");
    }
    if INSTRUCTION_FILES.iter().any(|name| same_name(name, file)) {
        bail!("{file} is read as instructions by the agents and stays where it is");
    }
    let is_note = Path::new(file)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            NOTE_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(ext))
        });
    if !is_note {
        bail!(
            "{file} is no note ({}) and stays in {INBOX}/",
            NOTE_EXTENSIONS.join(", ")
        );
    }
    let mut relative = PathBuf::new();
    for component in Path::new(folder).components() {
        let Component::Normal(name) = component else {
            bail!(
                "{folder:?} must be a folder below the vault, written without `..`, `.` or a leading `/`"
            );
        };
        if name.to_string_lossy().starts_with('.') {
            bail!("{folder:?} names a hidden folder");
        }
        relative.push(name);
    }
    let Some(area) = relative
        .components()
        .next()
        .map(|first| first.as_os_str().to_string_lossy().into_owned())
    else {
        bail!("name the folder to move the note into");
    };
    if APP_FOLDERS.iter().any(|name| same_name(name, &area)) {
        bail!("{area}/ is the app's own folder; notes go into an area");
    }
    // From here on every step goes through a folder handle opened without following a link: no link on the way is
    // followed, and a folder swapped after its check is not the one written to. (A handle still follows its folder
    // if someone renames that folder away meanwhile — which needs a second writer in the vault.)
    let root = open_dir(&rustix::fs::CWD, vault).context("the vault cannot be opened")?;
    let inbox_dir = open_dir(&root, Path::new(INBOX)).context("Inbox/ cannot be opened")?;
    let note = match regular_file(&inbox_dir, file) {
        Ok(Some(note)) => note,
        Ok(None) => bail!("{INBOX}/{file} is no regular file"),
        Err(err) if err == rustix::io::Errno::NOENT => bail!("there is no {INBOX}/{file}"),
        Err(err) => return Err(err.into()),
    };
    let mut target_dir = None;
    for (index, component) in relative.components().enumerate() {
        let parent = target_dir.as_ref().unwrap_or(&root);
        let opened = open_dir(parent, Path::new(component.as_os_str())).map_err(|_| {
            anyhow::anyhow!(
                "{} is no existing folder; a move never creates one",
                relative
                    .iter()
                    .take(index + 1)
                    .collect::<PathBuf>()
                    .display()
            )
        })?;
        if index == 0 && !matches!(regular_file(&opened, AREA_ROUTER), Ok(Some(_))) {
            bail!("{area}/ is no area of the vault (it has no {AREA_ROUTER})");
        }
        target_dir = Some(opened);
    }
    let Some(target_dir) = target_dir else {
        bail!("name the folder to move the note into");
    };
    let target = relative.join(file);
    // A link fails when the target exists, so nothing is ever replaced — not even by a file that appeared since a
    // check; the inbox's name goes only once the note is safely in its new place.
    match rustix::fs::linkat(
        &inbox_dir,
        file,
        &target_dir,
        file,
        rustix::fs::AtFlags::empty(),
    ) {
        Ok(()) => {}
        Err(rustix::io::Errno::EXIST) => {
            bail!(
                "{} exists already; the note stays in {INBOX}/",
                target.display()
            )
        }
        Err(err) => return Err(err.into()),
    }
    // The name linked must be the note checked: had it been swapped for another file or a link in between, the new
    // name is taken back (the original stays in the inbox, so nothing is lost).
    if regular_file(&target_dir, file).ok().flatten() != Some(note) {
        let _ = rustix::fs::unlinkat(&target_dir, file, rustix::fs::AtFlags::empty());
        bail!("the note changed during the move; it stays in {INBOX}/");
    }
    // The inbox's name goes only while it still is the note: a file put there since stays (and the note is moved).
    if regular_file(&inbox_dir, file).ok().flatten() == Some(note) {
        rustix::fs::unlinkat(&inbox_dir, file, rustix::fs::AtFlags::empty())?;
    } else {
        bail!(
            "the note is in {} now, but {INBOX}/{file} changed meanwhile and stays",
            target.display()
        );
    }
    Ok(target)
}

/// `path` below `parent` opened as a folder, never through a link.
fn open_dir(
    parent: &impl rustix::fd::AsFd,
    path: &Path,
) -> rustix::io::Result<rustix::fd::OwnedFd> {
    use rustix::fs::{Mode, OFlags};
    rustix::fs::openat(
        parent,
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
}

/// The device and inode of `name` in `dir` when it is a regular file itself (not a link), else `None`.
fn regular_file(dir: &impl rustix::fd::AsFd, name: &str) -> rustix::io::Result<Option<(u64, u64)>> {
    let stat = rustix::fs::statat(dir, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)?;
    let regular =
        rustix::fs::FileType::from_raw_mode(stat.st_mode) == rustix::fs::FileType::RegularFile;
    Ok(regular.then_some((stat.st_dev as u64, stat.st_ino as u64)))
}

/// Whether APFS takes `a` and `b` for one name: compared after Unicode case folding, not ASCII only — `AGENTſ.md` (a
/// long s) opens `AGENTS.md` there (fourth audit). Upper then lower case folds `ſ` to `s` and the Kelvin sign to `k`.
fn same_name(a: &str, b: &str) -> bool {
    let fold = |text: &str| text.to_uppercase().to_lowercase();
    fold(a) == fold(b)
}

/// Whether `path` is a regular file itself, not a link to one.
fn is_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_file())
}

/// Whether `path` is a folder itself, not a link to one.
fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// A vault with `Inbox/note.md`, an area `Arbeit/Projekte` and a file of its own; removed when dropped.
    struct Vault(PathBuf);

    impl Vault {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "axiomata-inbox-{tag}-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
            ));
            fs::create_dir_all(root.join("Inbox")).unwrap();
            fs::create_dir_all(root.join("Arbeit/Projekte")).unwrap();
            fs::write(root.join("Arbeit/AGENTS.md"), "router").unwrap();
            fs::create_dir_all(root.join("Kanban")).unwrap();
            fs::write(root.join("Kanban/AGENTS.md"), "router").unwrap();
            fs::create_dir_all(root.join("Ohne")).unwrap();
            fs::write(root.join("Inbox/note.md"), "note").unwrap();
            fs::write(root.join("AGENTS.md"), "router").unwrap();
            fs::write(root.join("Arbeit/taken.md"), "mine").unwrap();
            Vault(root)
        }
    }

    impl Drop for Vault {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_note_moves_into_an_existing_folder() {
        let vault = Vault::new("ok");
        let target = move_note(&vault.0, "note.md", "Arbeit/Projekte").unwrap();
        assert_eq!(target, PathBuf::from("Arbeit/Projekte/note.md"));
        assert_eq!(fs::read_to_string(vault.0.join(&target)).unwrap(), "note");
        assert!(!vault.0.join("Inbox/note.md").exists());
    }

    #[test]
    fn a_move_never_overwrites_and_keeps_the_note() {
        let vault = Vault::new("taken");
        fs::write(vault.0.join("Inbox/taken.md"), "new").unwrap();
        assert!(move_note(&vault.0, "taken.md", "Arbeit").is_err());
        assert_eq!(
            fs::read_to_string(vault.0.join("Arbeit/taken.md")).unwrap(),
            "mine"
        );
        assert!(vault.0.join("Inbox/taken.md").exists());
    }

    #[test]
    fn the_folder_must_exist_inside_the_vault_and_be_no_link() {
        let vault = Vault::new("folders");
        symlink(std::env::temp_dir(), vault.0.join("Out")).unwrap();
        fs::create_dir_all(vault.0.join(".hidden")).unwrap();
        for folder in [
            "",
            "Neu",
            "../x",
            "/tmp",
            "Arbeit/../..",
            "./Arbeit",
            "Out",
            ".hidden",
            "Inbox",
            "inbox",
            "Kanban",
            "Newſ",
            "Ohne",
        ] {
            assert!(
                move_note(&vault.0, "note.md", folder).is_err(),
                "{folder:?}"
            );
        }
        assert!(vault.0.join("Inbox/note.md").exists());
    }

    #[test]
    fn only_a_regular_file_of_the_inbox_moves_and_never_a_router() {
        let vault = Vault::new("files");
        for name in [
            "AGENTS.md",
            "claude.md",
            "Gemini.MD",
            "AGENTſ.md",
            "run.command",
            "page.html",
            "noext",
        ] {
            fs::write(vault.0.join("Inbox").join(name), "x").unwrap();
        }
        symlink(
            vault.0.join("Arbeit/taken.md"),
            vault.0.join("Inbox/link.md"),
        )
        .unwrap();
        fs::create_dir_all(vault.0.join("Inbox/sub")).unwrap();
        for file in [
            "",
            "..",
            ".env",
            "../Arbeit/taken.md",
            "sub/x.md",
            "AGENTS.md",
            "link.md",
            "sub",
            "none.md",
        ] {
            assert!(move_note(&vault.0, file, "Arbeit").is_err(), "{file:?}");
        }
    }

    #[test]
    fn a_vault_needs_its_own_router_too() {
        let vault = Vault::new("noroot");
        fs::remove_file(vault.0.join("AGENTS.md")).unwrap();
        assert!(move_note(&vault.0, "note.md", "Arbeit").is_err());
    }

    #[test]
    fn a_folder_without_an_inbox_is_no_vault() {
        let vault = Vault::new("noinbox");
        assert!(move_note(&vault.0.join("Arbeit"), "note.md", "Projekte").is_err());
    }
}
