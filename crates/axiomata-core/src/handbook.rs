//! The app's handbook (`docs/plans/chat-knows-app.md`, CH1): one Markdown page per area of Axiomata, in German
//! (`docs/handbook/`) and in English (`docs/handbook/en/`, same file names — `docs/plans/i18n.md`, Q4, Q22), kept in
//! the project and compiled into the build, so the pages always describe the binary that carries them. Both languages
//! come from one list of names: a page missing in either folder fails the build.
//!
//! Every page is embedded with `include_str!`: a page missing from `docs/handbook/` fails the build, and a page the
//! list below does not name fails [`tests::every_file_in_the_folder_is_a_listed_page`]. At the app's start
//! [`install`] writes the pages to `~/.axiomata/handbook/` — and replaces any copy that differs from the build,
//! unlike the bundled skills, which are seeded once and then the owner's (Q5). The CLI's `app handbook` and the app's
//! Help panel read the embedded copy, never the folder: what an agent or the owner reads is the build's own text even
//! if the folder was edited.

use std::fs;
use std::path::Path;

use crate::bridge::OwnerLanguage;
use crate::error::AxiomataError;
use crate::paths;

/// One page of the handbook: its name (the file name without `.md`, the only key anybody asks for a page by) and its
/// Markdown text, embedded at build time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    pub name: &'static str,
    pub text: &'static str,
}

/// Embeds `docs/handbook/<dir><name>.md` for every name, in the given order.
macro_rules! pages {
    ($dir:literal; $($name:literal),* $(,)?) => {
        &[$(Page {
            name: $name,
            text: include_str!(concat!("../../../docs/handbook/", $dir, $name, ".md")),
        }),*]
    };
}

/// Both languages' pages from the one list of names below.
macro_rules! handbook {
    ($($name:literal),* $(,)?) => {
        /// The German pages in the order of the plan's §3 table — the table of contents keeps it. A new area of the
        /// app gets a page in both folders in the same commit (AGENTS.md); the frontend's map from ring entries and
        /// settings tabs to pages (`core/handbook.ts`) names only pages of this list.
        pub const PAGES: &[Page] = pages!["";
            $($name),*
        ];
        /// The same pages in English (`docs/handbook/en/`).
        pub const PAGES_EN: &[Page] = pages!["en/";
            $($name),*
        ];
    };
}

handbook![
    "index",
    "orbit",
    "second-brain",
    "notizen",
    "editor",
    "tags-links",
    "studio",
    "flow",
    "modelle-rollen",
    "kanban",
    "chat",
    "spotlight",
    "skills",
    "routinen",
    "verbindungen",
    "terminal",
    "glut",
    "kosten",
    "einstellungen",
    "provider",
    "storage",
];

/// The folder's own note, written beside the pages: the folder belongs to the build and is replaced, so it is no place
/// for the owner's notes (those belong in the vault). Not a page: it is in no table of contents.
pub const README: &str = include_str!("../../../docs/handbook/README.md");
/// The same note for the English folder.
pub const README_EN: &str = include_str!("../../../docs/handbook/en/README.md");
/// The English pages' folder inside the handbook's folder.
const EN_DIR: &str = "en";
const README_NAME: &str = "README.md";

/// Longest summary sentence the table of contents carries; a longer first paragraph is cut there.
const MAX_SUMMARY_CHARS: usize = 300;

impl Page {
    /// The page's title: its first line without the `# `. Empty only for a page that breaks the convention, which the
    /// tests refuse.
    pub fn title(&self) -> &'static str {
        self.text
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("# "))
            .map(str::trim)
            .unwrap_or_default()
    }

    /// The sentence below the title, as one line — what the table of contents shows for the page. Empty when the
    /// first paragraph after the title is no plain text (a heading, a list, a table, code).
    pub fn summary(&self) -> String {
        let paragraph: Vec<&str> = self
            .text
            .lines()
            .skip(1)
            .skip_while(|line| line.trim().is_empty())
            .take_while(|line| !line.trim().is_empty())
            .map(str::trim)
            .collect();
        let Some(first) = paragraph.first() else {
            return String::new();
        };
        if first.starts_with(['#', '|', '-', '*', '>', '`', '<', '!']) {
            return String::new();
        }
        paragraph
            .join(" ")
            .chars()
            .take(MAX_SUMMARY_CHARS)
            .collect()
    }

    fn file_name(&self) -> String {
        format!("{}.md", self.name)
    }
}

/// The pages in `language`.
pub fn pages(language: OwnerLanguage) -> &'static [Page] {
    match language {
        OwnerLanguage::De => PAGES,
        OwnerLanguage::En => PAGES_EN,
    }
}

/// The German page named `name`, if the build has one. The only lookup there is: a name is matched against the list,
/// never turned into a path, so no caller can reach a file through it.
pub fn page(name: &str) -> Option<&'static Page> {
    page_in(name, OwnerLanguage::De)
}

/// The page named `name` in `language` — the same names in both.
pub fn page_in(name: &str, language: OwnerLanguage) -> Option<&'static Page> {
    let name = name.trim().trim_end_matches(".md");
    pages(language).iter().find(|page| page.name == name)
}

/// The German table of contents (see [`contents_in`]).
pub fn contents() -> String {
    contents_in(OwnerLanguage::De)
}

/// The table of contents in `language`: one line per page, `` - `name` — Title: summary ``, in [`PAGES`] order. The
/// chat's prompt carries it instead of the pages themselves (Q2).
pub fn contents_in(language: OwnerLanguage) -> String {
    pages(language)
        .iter()
        .map(|page| format!("- `{}` — {}: {}\n", page.name, page.title(), page.summary()))
        .collect()
}

/// The language the owner reads the app in, for a reader outside the app's window — the CLI's `app handbook` and the
/// chat's app tool: the "language" the app stored in `dashboard.json` (`system`, `de`, `en`), "system" resolved as
/// Glut resolves it (`glut::lang::detect`); English when the app never stored one and the system is not German.
pub fn owner_language() -> OwnerLanguage {
    let stored = fs::read_to_string(paths::dashboard_state_path())
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|state| state["settings"]["language"].as_str().map(str::to_owned));
    match stored.as_deref() {
        Some("de") => OwnerLanguage::De,
        Some("en") => OwnerLanguage::En,
        _ => system_language(),
    }
}

/// The system's language as Glut reads it — once per process: on a Mac it asks `defaults`, which may take a moment.
fn system_language() -> OwnerLanguage {
    static SYSTEM: std::sync::OnceLock<OwnerLanguage> = std::sync::OnceLock::new();
    *SYSTEM.get_or_init(|| {
        if glut::lang::detect() == glut::lang::Lang::De {
            OwnerLanguage::De
        } else {
            OwnerLanguage::En
        }
    })
}

/// Brings `dir` to the build's handbook: every page and the README written where the file is missing or differs (an
/// owner's edit included — the folder is the build's, Q5), and any other `.md` there removed, so a page renamed or
/// dropped in a newer build does not linger. Returns how many files it wrote or removed.
///
/// Errors: [`AxiomataError::Io`] at the first file that cannot be read, written or removed.
pub fn install_into(dir: &Path) -> Result<usize, AxiomataError> {
    Ok(install_pages(dir, PAGES, README)? + install_pages(&dir.join(EN_DIR), PAGES_EN, README_EN)?)
}

/// [`install_into`] for one language's folder.
fn install_pages(dir: &Path, pages: &[Page], readme: &'static str) -> Result<usize, AxiomataError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| AxiomataError::Io { path, source }
    };
    fs::create_dir_all(dir).map_err(io(dir))?;
    let wanted: Vec<(String, &str)> = pages
        .iter()
        .map(|page| (page.file_name(), page.text))
        .chain(std::iter::once((README_NAME.to_owned(), readme)))
        .collect();
    let mut changed = 0;
    for (name, text) in &wanted {
        let path = dir.join(name);
        // Bytes, not a stored hash: an edit made in the folder is caught too, and 21 small files cost nothing to read.
        if fs::read(&path).ok().as_deref() == Some(text.as_bytes()) {
            continue;
        }
        write_replacing(&path, text).map_err(io(&path))?;
        changed += 1;
    }
    for entry in fs::read_dir(dir).map_err(io(dir))?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let stale = name.ends_with(".md") && !wanted.iter().any(|(wanted, _)| *wanted == name);
        if stale && entry.file_type().is_ok_and(|kind| kind.is_file()) {
            let path = entry.path();
            fs::remove_file(&path).map_err(io(&path))?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// [`install_into`] for `~/.axiomata/handbook/`, at every start of the app (not the CLI's). Best-effort: a handbook
/// that could not be written leaves the old copy (or none) until the next start; the app and the CLI read the embedded
/// pages anyway.
pub fn install() {
    match install_into(&paths::handbook_dir()) {
        Ok(0) => {}
        Ok(changed) => tracing::info!(changed, "brought the handbook to this build"),
        Err(err) => tracing::warn!(%err, "could not write the handbook"),
    }
}

/// Writes through a temporary file and a rename, so a reader never sees half a page.
fn write_replacing(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("md.{}.tmp", std::process::id()));
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;

    /// The pages of `docs/plans/chat-knows-app.md` §3, in its order.
    const PLANNED: [&str; 21] = [
        "index",
        "orbit",
        "second-brain",
        "notizen",
        "editor",
        "tags-links",
        "studio",
        "flow",
        "modelle-rollen",
        "kanban",
        "chat",
        "spotlight",
        "skills",
        "routinen",
        "verbindungen",
        "terminal",
        "glut",
        "kosten",
        "einstellungen",
        "provider",
        "storage",
    ];

    #[test]
    fn the_build_carries_every_planned_page() {
        let names: Vec<&str> = PAGES.iter().map(|page| page.name).collect();
        assert_eq!(names, PLANNED);
    }

    #[test]
    fn every_page_is_there_in_english_with_the_same_name_title_and_summary() {
        let german: Vec<&str> = PAGES.iter().map(|page| page.name).collect();
        let english: Vec<&str> = PAGES_EN.iter().map(|page| page.name).collect();
        assert_eq!(english, german);
        for page in PAGES_EN {
            assert!(
                !page.title().is_empty(),
                "en/{}.md must open with `# Title`",
                page.name
            );
            assert!(
                page.summary().chars().count() >= 10,
                "en/{}.md needs a summary sentence",
                page.name
            );
        }
        assert!(README_EN.starts_with("# "));
        assert_eq!(
            page_in("kanban", OwnerLanguage::En).map(|p| p.text),
            Some(PAGES_EN[9].text)
        );
        assert_ne!(
            contents_in(OwnerLanguage::En),
            contents_in(OwnerLanguage::De)
        );
    }

    #[test]
    fn the_owners_language_is_the_one_the_app_stored() {
        let _guard = crate::test_support::ENV_MUTEX
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let home = unique_temp_dir("axiomata-test-handbook-lang");
        fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe { std::env::set_var(paths::AXIOMATA_HOME_ENV, &home) };
        let store = |language: &str| {
            fs::write(
                paths::dashboard_state_path(),
                format!(r#"{{"settings":{{"language":"{language}"}}}}"#),
            )
            .unwrap()
        };
        store("de");
        assert_eq!(owner_language(), OwnerLanguage::De);
        store("en");
        assert_eq!(owner_language(), OwnerLanguage::En);
        unsafe { std::env::remove_var(paths::AXIOMATA_HOME_ENV) };
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn every_page_has_a_title_and_a_summary_sentence() {
        for page in PAGES {
            assert!(
                page.text.starts_with("# ") && !page.title().is_empty(),
                "{}.md must open with `# Title`",
                page.name
            );
            let summary = page.summary();
            assert!(
                summary.chars().count() >= 10,
                "{}.md needs one summary sentence under its title, got {summary:?}",
                page.name
            );
        }
        assert!(README.starts_with("# "), "README.md opens with a title");
    }

    #[test]
    fn every_file_in_the_english_folder_is_a_listed_page() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/handbook/en");
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".md") else {
                continue;
            };
            assert!(
                name == README_NAME || page_in(stem, OwnerLanguage::En).is_some(),
                "docs/handbook/en/{name} is in no table of contents"
            );
        }
    }

    #[test]
    fn every_file_in_the_folder_is_a_listed_page() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/handbook");
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".md") else {
                continue;
            };
            assert!(
                name == README_NAME || page(stem).is_some(),
                "docs/handbook/{name} is in no table of contents: add it to handbook::PAGES"
            );
        }
    }

    #[test]
    fn pages_are_found_by_name_only() {
        assert_eq!(page("studio").map(|p| p.name), Some("studio"));
        assert_eq!(page("studio.md").map(|p| p.name), Some("studio"));
        assert!(page("../config.toml").is_none());
        assert!(page("README").is_none());
        assert!(page("").is_none());
    }

    #[test]
    fn contents_lists_every_page_once() {
        let toc = contents();
        assert_eq!(toc.lines().count(), PAGES.len());
        assert!(toc.contains("- `kanban` — "));
    }

    #[test]
    fn summary_takes_the_first_paragraph_as_one_line() {
        let page = Page {
            name: "x",
            text: "# X\n\nFirst line\nwraps here.\n\nNext paragraph.\n",
        };
        assert_eq!(page.title(), "X");
        assert_eq!(page.summary(), "First line wraps here.");
        let heading = Page {
            name: "y",
            text: "# Y\n\n## Not a sentence\n",
        };
        assert_eq!(heading.summary(), "");
    }

    #[test]
    fn install_replaces_what_differs_and_removes_stale_pages() {
        let dir = unique_temp_dir("axiomata-test-handbook");
        assert_eq!(
            install_into(&dir).unwrap(),
            2 * (PAGES.len() + 1),
            "both languages"
        );
        assert_eq!(
            fs::read_to_string(dir.join("en/studio.md")).unwrap(),
            page_in("studio", OwnerLanguage::En).unwrap().text
        );
        assert_eq!(
            install_into(&dir).unwrap(),
            0,
            "a matching copy is left alone"
        );
        fs::write(dir.join("studio.md"), "my own edit").unwrap();
        fs::write(dir.join("old-page.md"), "from an older build").unwrap();
        fs::write(dir.join("notes.txt"), "not a page").unwrap();
        assert_eq!(install_into(&dir).unwrap(), 2);
        assert_eq!(
            fs::read_to_string(dir.join("studio.md")).unwrap(),
            page("studio").unwrap().text
        );
        assert!(!dir.join("old-page.md").exists());
        assert!(dir.join("notes.txt").exists(), "only pages are the build's");
        assert_eq!(fs::read_to_string(dir.join(README_NAME)).unwrap(), README);
        let _ = fs::remove_dir_all(&dir);
    }
}
