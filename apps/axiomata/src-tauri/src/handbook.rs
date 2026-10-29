//! The handbook for the owner (`docs/plans/chat-knows-app.md`, CH4): the Help panel, the spotlight's "Handbuch" group
//! and the chat's `/help <topic>` read the pages through these commands. They are served by name from the copy
//! embedded in this build (`axiomata_core::handbook`) — the webview never names a path, and what it shows is the
//! build's own text even if `~/.axiomata/handbook/` was edited.

use axiomata_core::handbook;
use serde::Serialize;

/// One page as the table of contents lists it.
#[derive(Debug, Clone, Serialize)]
pub struct HandbookEntry {
    pub name: &'static str,
    pub title: &'static str,
    pub summary: String,
}

/// One page with its Markdown.
#[derive(Debug, Clone, Serialize)]
pub struct HandbookPage {
    pub name: &'static str,
    pub title: &'static str,
    pub text: &'static str,
}

fn entries() -> Vec<HandbookEntry> {
    handbook::PAGES
        .iter()
        .map(|page| HandbookEntry {
            name: page.name,
            title: page.title(),
            summary: page.summary(),
        })
        .collect()
}

fn page(name: &str) -> Result<HandbookPage, String> {
    let page = handbook::page(name).ok_or_else(|| format!("the handbook has no page {name:?}"))?;
    Ok(HandbookPage {
        name: page.name,
        title: page.title(),
        text: page.text,
    })
}

/// Every page, in the handbook's order.
#[tauri::command]
pub async fn handbook_pages() -> Result<Vec<HandbookEntry>, String> {
    // Cheap, but kept off the event loop like every other command that reads.
    tokio::task::spawn_blocking(entries)
        .await
        .map_err(|err| format!("the handbook could not be read: {err}"))
}

/// One page by its name; an unknown name is an error, never a file looked up.
#[tauri::command]
pub async fn handbook_page(name: String) -> Result<HandbookPage, String> {
    tokio::task::spawn_blocking(move || page(&name))
        .await
        .map_err(|err| format!("the handbook could not be read: {err}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_are_served_by_name_from_the_build() {
        assert_eq!(entries().len(), handbook::PAGES.len());
        assert_eq!(
            page("kanban").unwrap().text,
            handbook::page("kanban").unwrap().text
        );
        assert!(page("../../config.toml").is_err());
    }
}
