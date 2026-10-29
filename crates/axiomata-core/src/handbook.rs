//! The app's handbook (`docs/plans/chat-knows-app.md`, CH1): one German Markdown page per area of Axiomata, kept in
//! the project (`docs/handbook/`) and compiled into the build, so the pages always describe the binary that carries
//! them.
//!
//! Every page is embedded with `include_str!`: a page missing from `docs/handbook/` fails the build, and a page the
//! list below does not name fails [`tests::every_file_in_the_folder_is_a_listed_page`]. At the app's start
//! [`install`] writes the pages to `~/.axiomata/handbook/` — and replaces any copy that differs from the build,
//! unlike the bundled skills, which are seeded once and then the owner's (Q5). The CLI's `app handbook` and the app's
//! Help panel read the embedded copy, never the folder: what an agent or the owner reads is the build's own text even
//! if the folder was edited.

use std::fs;
use std::path::Path;

use crate::error::AxiomataError;
use crate::paths;

/// One page of the handbook: its name (the file name without `.md`, the only key anybody asks for a page by) and its
/// Markdown text, embedded at build time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    pub name: &'static str,
    pub text: &'static str,
}

/// Embeds `docs/handbook/<name>.md` for every name, in the given order.
macro_rules! pages {
    ($($name:literal),* $(,)?) => {
        &[$(Page {
            name: $name,
            text: include_str!(concat!("../../../docs/handbook/", $name, ".md")),
        }),*]
    };
}

/// The pages in the order of the plan's §3 table — the table of contents keeps it. A new area of the app gets a page
/// here and a file in `docs/handbook/` in the same commit (AGENTS.md); the frontend's map from ring entries and
/// settings tabs to pages (`core/handbook.ts`) names only pages of this list.
pub const PAGES: &[Page] = pages![
    "index",
    "orbit",
    "second-brain",
    "notizen",
    "editor",
    "tags-links",
    "studio",
    "flow",
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

/// The page named `name`, if the build has one. The only lookup there is: a name is matched against the list, never
/// turned into a path, so no caller can reach a file through it.
pub fn page(name: &str) -> Option<&'static Page> {
    let name = name.trim().trim_end_matches(".md");
    PAGES.iter().find(|page| page.name == name)
}

/// The table of contents: one line per page, `` - `name` — Title: summary ``, in [`PAGES`] order. The chat's prompt
/// carries it instead of the pages themselves (Q2).
pub fn contents() -> String {
    PAGES
        .iter()
        .map(|page| format!("- `{}` — {}: {}\n", page.name, page.title(), page.summary()))
        .collect()
}

/// Brings `dir` to the build's handbook: every page and the README written where the file is missing or differs (an
/// owner's edit included — the folder is the build's, Q5), and any other `.md` there removed, so a page renamed or
/// dropped in a newer build does not linger. Returns how many files it wrote or removed.
///
/// Errors: [`AxiomataError::Io`] at the first file that cannot be read, written or removed.
pub fn install_into(dir: &Path) -> Result<usize, AxiomataError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| AxiomataError::Io { path, source }
    };
    fs::create_dir_all(dir).map_err(io(dir))?;
    let wanted: Vec<(String, &str)> = PAGES
        .iter()
        .map(|page| (page.file_name(), page.text))
        .chain(std::iter::once((README_NAME.to_owned(), README)))
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
    const PLANNED: [&str; 20] = [
        "index",
        "orbit",
        "second-brain",
        "notizen",
        "editor",
        "tags-links",
        "studio",
        "flow",
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
        assert_eq!(install_into(&dir).unwrap(), PAGES.len() + 1);
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
