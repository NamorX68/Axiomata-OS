//! The owner's confirmation of a project's `tasks.json`.
//!
//! A cloned repository can bring a `tasks.json` that runs anything, so the file runs only after the owner
//! saw its commands and said yes — for **exactly that content**: the stored value is the SHA-256 of the
//! file's bytes, so any change asks again. Kept in one file under `~/.axiomata`, written from Rust only
//! (the webview has no command that writes a hash it chose; it can only ask to confirm what is on disk now).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// SHA-256 of `bytes`, lower-case hex.
pub fn hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    files: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
pub struct TrustStore {
    files: BTreeMap<String, String>,
    path: Option<PathBuf>,
}

fn key(project: &Path) -> String {
    project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

impl TrustStore {
    /// A store that remembers nothing on disk (tests, and the answer when the file cannot be read).
    pub fn in_memory() -> TrustStore {
        TrustStore::default()
    }

    /// Reads `path`; a missing or unreadable file means nothing is trusted yet — never the other way round.
    pub fn load(path: &Path) -> TrustStore {
        let files = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
            .map(|s| s.files)
            .unwrap_or_default();
        TrustStore {
            files,
            path: Some(path.to_path_buf()),
        }
    }

    pub fn is_trusted(&self, project: &Path, hash: &str) -> bool {
        self.files
            .get(&key(project))
            .is_some_and(|stored| stored == hash)
    }

    /// Remembers `hash` for `project` and writes the file (atomically, readable by the owner only).
    pub fn trust(&mut self, project: &Path, hash: &str) -> std::io::Result<()> {
        self.files.insert(key(project), hash.to_string());
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_vec_pretty(&Stored {
            files: self.files.clone(),
        })
        .map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        {
            let mut file = std::fs::File::create(&tmp)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            }
            file.write_all(&text)?;
            file.sync_all()?;
        }
        std::fs::rename(&tmp, path)
    }
}
