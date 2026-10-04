//! The secret that makes one start of an agent session the only one that can speak for it (`docs/plans/a2a.md`, CP-A5).
//!
//! The MCP server of a session used to believe `AXIOMATA_AGENT_ID` — an environment variable every child process of
//! the harness inherits, so an agent could start `axiomata-cli mcp-serve` itself as the reviewer of its own card.
//! The identity now comes with a **secret issued at every start**: [`issue`] makes 256 random bits and hands them
//! back once, for the session's MCP configuration (which no child process inherits); only their SHA-256 is kept, in
//! the session's channel directory. [`verify`] checks what a starting server presents against it.
//!
//! This keeps out what is cheap — a forged variable, a guessed id, a leftover from an earlier start (every start
//! replaces the hash, so an old secret stops working) — and nothing else. It is not a sandbox: the channel directory
//! and the MCP configuration belong to the same user as the agent, who can read another session's configuration or
//! overwrite its hash file with the hash of a secret of their own choosing. What it changes is that impersonating
//! another session now takes deliberate digging, not one `export`.

use std::fs;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::lifecycle::{Channel, ChannelRoots};
use crate::{IdeError, Result};

/// The file in the session's channel directory that holds the hash.
const HASH_FILE: &str = "mcp-token.sha256";
/// Random bytes in a secret: 256 bits, far beyond guessing.
const SECRET_BYTES: usize = 32;

fn lower_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hash_of(secret: &str) -> String {
    lower_hex(&Sha256::digest(secret.as_bytes()))
}

fn hash_path(roots: &ChannelRoots, agent_id: i64) -> PathBuf {
    Channel::for_agent(roots, agent_id).dir().join(HASH_FILE)
}

/// Makes a new secret for session `agent_id`, stores its hash (replacing the one of the earlier start) and returns the
/// secret. The caller puts it into the session's MCP configuration and nowhere else; it is not stored in clear.
///
/// # Errors
///
/// [`IdeError::Io`] when the channel directory or the hash file cannot be written, [`IdeError::Invalid`] when the
/// system has no random source.
pub fn issue(roots: &ChannelRoots, agent_id: i64) -> Result<String> {
    let mut bytes = [0u8; SECRET_BYTES];
    getrandom::fill(&mut bytes).map_err(|err| IdeError::Invalid {
        field: "session secret",
        reason: format!("no random source: {err}"),
    })?;
    let secret = lower_hex(&bytes);
    Channel::for_agent(roots, agent_id).write_private(HASH_FILE, &hash_of(&secret))?;
    Ok(secret)
}

/// Takes back the secret of session `agent_id`: a server that starts afterwards cannot present one until the next start
/// issues a new secret. A server that is running keeps running — the secret is only asked at its start. Missing is
/// fine.
///
/// # Errors
///
/// [`IdeError::Io`] when the file exists but cannot be removed.
pub fn revoke(roots: &ChannelRoots, agent_id: i64) -> Result<()> {
    let path = hash_path(roots, agent_id);
    match fs::remove_file(&path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            Err(IdeError::Io { path, source: err })
        }
        _ => Ok(()),
    }
}

/// Whether `presented` is the secret issued at the latest start of session `agent_id`. A missing or damaged hash file
/// means no — never a way in.
pub fn verify(roots: &ChannelRoots, agent_id: i64, presented: &str) -> bool {
    let Ok(stored) = fs::read_to_string(hash_path(roots, agent_id)) else {
        return false;
    };
    let stored = stored.trim();
    let candidate = hash_of(presented.trim());
    // Both are 64 characters of hex when the file is intact; comparing every byte regardless keeps the time independent
    // of where they first differ.
    stored.len() == candidate.len()
        && stored
            .bytes()
            .zip(candidate.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn roots() -> ChannelRoots {
        let base = std::env::temp_dir().join(format!(
            "axiomata-token-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        ChannelRoots {
            events: base.join("events"),
            claude_tasks: base.join("tasks"),
            claude_plans: base.join("plans"),
        }
    }

    #[test]
    fn the_issued_secret_verifies_and_nothing_else_does() {
        let roots = roots();
        let secret = issue(&roots, 7).unwrap();
        assert_eq!(secret.len(), SECRET_BYTES * 2);
        assert!(verify(&roots, 7, &secret));
        assert!(
            verify(&roots, 7, &format!("  {secret}\n")),
            "whitespace around it is not a different secret"
        );
        assert!(!verify(&roots, 7, ""));
        assert!(!verify(&roots, 7, "7"));
        assert!(!verify(&roots, 7, &secret.to_uppercase()));
        assert!(
            !verify(&roots, 8, &secret),
            "another session has no hash at all"
        );
    }

    #[test]
    fn a_new_start_replaces_the_secret() {
        let roots = roots();
        let first = issue(&roots, 3).unwrap();
        let second = issue(&roots, 3).unwrap();
        assert_ne!(first, second);
        assert!(
            !verify(&roots, 3, &first),
            "the earlier start's secret must stop working"
        );
        assert!(verify(&roots, 3, &second));
    }

    #[test]
    fn the_secret_is_not_stored_in_clear_and_the_file_is_private() {
        let roots = roots();
        let secret = issue(&roots, 4).unwrap();
        let path = hash_path(&roots, 4);
        let stored = fs::read_to_string(&path).unwrap();
        assert!(!stored.contains(&secret));
        assert_eq!(stored.trim().len(), 64);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn a_revoked_secret_stops_working_and_revoking_twice_is_fine() {
        let roots = roots();
        let secret = issue(&roots, 6).unwrap();
        revoke(&roots, 6).unwrap();
        assert!(!verify(&roots, 6, &secret));
        revoke(&roots, 6).unwrap();
        let again = issue(&roots, 6).unwrap();
        assert!(verify(&roots, 6, &again), "the next start issues a new one");
    }

    #[test]
    fn a_damaged_or_empty_hash_file_lets_nobody_in() {
        let roots = roots();
        let secret = issue(&roots, 5).unwrap();
        let path = hash_path(&roots, 5);
        for damaged in ["", "garbage", "\n"] {
            fs::write(&path, damaged).unwrap();
            assert!(!verify(&roots, 5, &secret), "{damaged:?}");
            assert!(!verify(&roots, 5, damaged), "{damaged:?}");
        }
    }
}
