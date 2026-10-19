//! `axiomata-cli inbox move <file> <folder>` — the one move the `inbox-sort` skill makes (glut-round W8, security
//! audit): a note out of the vault's `Inbox/` into a folder that already exists, never overwriting, never through a
//! link. A permission rule can only name a command's first words, so `Bash(mv -n:*)` also let `mv -n -f a b` and any
//! destination through; this command checks the paths itself, and the skill may run nothing else.
//!
//! It runs in the working folder (the vault the skill runs in) and needs neither the core nor its database, so it
//! works in Glut's sandbox, which hides `~/.axiomata`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};
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
    if !is_real_dir(&inbox) {
        bail!("there is no {INBOX}/ folder here; run this in the vault");
    }
    if file.is_empty() || file.starts_with('.') || file.contains(['/', '\\']) || file == ".." {
        bail!("{file:?} is no file name inside {INBOX}/");
    }
    if INSTRUCTION_FILES
        .iter()
        .any(|name| name.eq_ignore_ascii_case(file))
    {
        bail!("{file} is read as instructions by the agents and stays where it is");
    }
    let note = Path::new(file)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            NOTE_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(ext))
        });
    if !note {
        bail!(
            "{file} is no note ({}) and stays in {INBOX}/",
            NOTE_EXTENSIONS.join(", ")
        );
    }
    let source = inbox.join(file);
    match fs::symlink_metadata(&source) {
        Ok(meta) if meta.file_type().is_file() => {}
        Ok(_) => bail!("{INBOX}/{file} is no regular file"),
        Err(err) if err.kind() == ErrorKind::NotFound => bail!("there is no {INBOX}/{file}"),
        Err(err) => return Err(err.into()),
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
        // Each step must be a real folder: a link would carry the note out of the vault.
        if !is_real_dir(&vault.join(&relative)) {
            bail!(
                "{} is no existing folder; a move never creates one",
                relative.display()
            );
        }
    }
    let Some(area) = relative
        .components()
        .next()
        .map(|first| first.as_os_str().to_string_lossy().into_owned())
    else {
        bail!("name the folder to move the note into");
    };
    if APP_FOLDERS
        .iter()
        .any(|name| name.eq_ignore_ascii_case(&area))
    {
        bail!("{area}/ is the app's own folder; notes go into an area");
    }
    match fs::symlink_metadata(vault.join(&area).join(AREA_ROUTER)) {
        Ok(meta) if meta.file_type().is_file() => {}
        _ => bail!("{area}/ is no area of the vault (it has no {AREA_ROUTER})"),
    }
    let target = relative.join(file);
    // A hard link fails when the target exists, so nothing is ever replaced — not even by a file that appeared since
    // a check; the inbox's name goes only once the note is safely in its new place.
    match fs::hard_link(&source, vault.join(&target)) {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            bail!(
                "{} exists already; the note stays in {INBOX}/",
                target.display()
            )
        }
        Err(err) => return Err(err.into()),
    }
    fs::remove_file(&source)?;
    Ok(target)
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
    fn a_folder_without_an_inbox_is_no_vault() {
        let vault = Vault::new("noinbox");
        assert!(move_note(&vault.0.join("Arbeit"), "note.md", "Projekte").is_err());
    }
}
