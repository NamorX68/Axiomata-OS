//! Which sessions are alive, as far as a process other than the app can tell (`docs/plans/a2a.md`, CP-A4).
//!
//! The mailbox must know which sessions can receive a message (A29: mail to an ended session comes back as
//! undeliverable). The app knows — it owns the panes — but the MCP server is a **separate process** that cannot ask it.
//! What the server can do is *be* the evidence: it is started by the session's harness and lives exactly as long as the
//! harness does. So it holds an exclusive advisory lock on `<events>/<id>/mcp.lock` for its whole life, and anyone who
//! wants to know whether session `id` is alive tries the same lock — if it is taken, the session is. The operating
//! system drops a lock when its process dies, crash or not, so there is no heartbeat to go stale and no file that has
//! to be cleaned up (an old lock file that nobody holds means "not alive").
//!
//! Nothing here is authority: the lock proves a process is there, not who it is. Anyone of the same user could hold it
//! to look alive, which only gets that process mail addressed to a session id it is not.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::lifecycle::{Channel, ChannelRoots};
use crate::{IdeError, Result};

/// The lock file's name inside a session's status channel directory.
const LOCK_FILE: &str = "mcp.lock";
/// How often [`hold`] tries again when the lock looks taken (100 × 20 ms = 2 s). A probe ([`live_sessions`]) takes the
/// lock for a moment, and a server that starts exactly then must not conclude that another server is already there.
const HOLD_ATTEMPTS: u32 = 100;
const HOLD_PAUSE: Duration = Duration::from_millis(20);

/// Proof that this process vouches for a session: dropping it (or exiting) releases the lock.
#[derive(Debug)]
pub struct Presence {
    _file: File,
}

fn io(path: &Path, source: std::io::Error) -> IdeError {
    IdeError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn lock_path(roots: &ChannelRoots, agent_id: i64) -> PathBuf {
    Channel::for_agent(roots, agent_id).dir().join(LOCK_FILE)
}

/// Opens the lock file without following a symlink at its place: the channel directory sits where the agent's own
/// shell can write, and a link planted there must not make the server create or lock some other file.
fn open_lock(path: &Path, create: bool) -> Result<Option<File>> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_file() => {
            return Err(io(
                path,
                std::io::Error::other("the lock path is not a regular file"),
            ));
        }
        Ok(_) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => {
            if !create {
                return Ok(None);
            }
        }
        Err(err) => return Err(io(path, err)),
    }
    if create && let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(create)
        .truncate(false)
        .open(path)
        .map_err(|e| io(path, e))?;
    // The check above and the open are two steps. Looking again *after* the open, and requiring that the path still
    // names the very file we hold, catches a symlink swapped in between: it would lead to another inode.
    let held = file.metadata().map_err(|e| io(path, e))?;
    let named = fs::symlink_metadata(path).map_err(|e| io(path, e))?;
    if !held.is_file() || !named.is_file() || (held.dev(), held.ino()) != (named.dev(), named.ino())
    {
        return Err(io(
            path,
            std::io::Error::other("the lock path changed while it was opened"),
        ));
    }
    Ok(Some(file))
}

/// Takes the presence lock of session `agent_id`.
///
/// `None` means another process already holds it — a second server for the same session, which is fine: that one
/// vouches. The caller keeps the returned value for as long as the session should count as alive.
///
/// # Errors
///
/// [`IdeError::Io`] if the file cannot be opened or locked for a reason other than being held, or is not a regular
/// file.
pub fn hold(roots: &ChannelRoots, agent_id: i64) -> Result<Option<Presence>> {
    let path = lock_path(roots, agent_id);
    let Some(file) = open_lock(&path, true)? else {
        return Ok(None);
    };
    for attempt in 0..HOLD_ATTEMPTS {
        match file.try_lock() {
            Ok(()) => return Ok(Some(Presence { _file: file })),
            Err(TryLockError::WouldBlock) => {
                if attempt + 1 < HOLD_ATTEMPTS {
                    thread::sleep(HOLD_PAUSE);
                }
            }
            Err(TryLockError::Error(err)) => return Err(io(&path, err)),
        }
    }
    Ok(None)
}

/// Like [`hold`], but waits as long as it takes for another holder to let go, and always ends up holding the lock.
///
/// For the case where [`hold`] found the lock taken: that holder may be a server the harness is replacing, and when it
/// exits this one has to take over, or the session would count as ended for the rest of its life although a server
/// runs. Blocks the calling thread — run it on one of its own.
///
/// # Errors
///
/// [`IdeError::Io`] as for [`hold`].
pub fn hold_blocking(roots: &ChannelRoots, agent_id: i64) -> Result<Presence> {
    let path = lock_path(roots, agent_id);
    let Some(file) = open_lock(&path, true)? else {
        return Err(io(&path, std::io::Error::other("the lock file vanished")));
    };
    file.lock().map_err(|e| io(&path, e))?;
    Ok(Presence { _file: file })
}

/// Whether session `agent_id` has a live server right now.
///
/// # Errors
///
/// [`IdeError::Io`] for an unreadable or irregular lock file; a missing one is simply "not alive".
pub fn is_live(roots: &ChannelRoots, agent_id: i64) -> Result<bool> {
    let path = lock_path(roots, agent_id);
    let Some(file) = open_lock(&path, false)? else {
        return Ok(false);
    };
    match file.try_lock_shared() {
        // We got it, so nobody holds the exclusive lock; it is released again when `file` drops.
        Ok(()) => Ok(false),
        Err(TryLockError::WouldBlock) => Ok(true),
        Err(TryLockError::Error(err)) => Err(io(&path, err)),
    }
}

/// The ids among `candidates` that are alive, in the order given.
///
/// A candidate whose lock cannot be examined (somebody put a directory or a link where the file belongs) counts as
/// **not alive** instead of failing the whole question: one damaged channel directory must not take the mailbox of
/// every other session down with it.
pub fn live_sessions(roots: &ChannelRoots, candidates: &[i64]) -> Vec<i64> {
    candidates
        .iter()
        .copied()
        .filter(|id| is_live(roots, *id).unwrap_or(false))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn roots() -> ChannelRoots {
        let base = std::env::temp_dir().join(format!(
            "axiomata-presence-test-{}-{}",
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
    fn a_session_is_alive_exactly_while_its_lock_is_held() {
        let roots = roots();
        assert!(!is_live(&roots, 1).unwrap(), "no file, not alive");

        let presence = hold(&roots, 1).unwrap().expect("first holder");
        assert!(is_live(&roots, 1).unwrap());
        assert!(
            !is_live(&roots, 2).unwrap(),
            "another session is not affected"
        );
        assert_eq!(live_sessions(&roots, &[2, 1, 3]), vec![1]);

        drop(presence);
        assert!(
            !is_live(&roots, 1).unwrap(),
            "released with the holder; an old file means not alive"
        );
    }

    #[test]
    fn a_second_server_for_the_same_session_defers_to_the_first() {
        let roots = roots();
        let first = hold(&roots, 7).unwrap();
        assert!(first.is_some());
        assert!(hold(&roots, 7).unwrap().is_none(), "already vouched for");
        drop(first);
        assert!(hold(&roots, 7).unwrap().is_some(), "free again");
    }

    #[test]
    fn a_planted_symlink_is_refused() {
        let roots = roots();
        let dir = Channel::for_agent(&roots, 3).dir().to_path_buf();
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("elsewhere");
        fs::write(&target, "x").unwrap();
        std::os::unix::fs::symlink(&target, dir.join(LOCK_FILE)).unwrap();
        assert!(matches!(hold(&roots, 3), Err(IdeError::Io { .. })));
        assert!(matches!(is_live(&roots, 3), Err(IdeError::Io { .. })));
    }

    #[test]
    fn one_damaged_lock_path_does_not_hide_the_other_sessions() {
        let roots = roots();
        let _alive = hold(&roots, 1).unwrap().unwrap();
        let broken = Channel::for_agent(&roots, 2).dir().join(LOCK_FILE);
        fs::create_dir_all(&broken).unwrap();
        assert_eq!(live_sessions(&roots, &[2, 1]), vec![1]);
    }

    #[test]
    fn a_blocking_hold_takes_over_when_the_first_holder_lets_go() {
        let roots = roots();
        let first = hold(&roots, 5).unwrap().unwrap();
        let waiter = {
            let roots = roots.clone();
            thread::spawn(move || hold_blocking(&roots, 5).unwrap())
        };
        thread::sleep(Duration::from_millis(100));
        assert!(
            !waiter.is_finished(),
            "still waiting while the first one holds it"
        );
        drop(first);
        let second = waiter.join().unwrap();
        assert!(
            is_live(&roots, 5).unwrap(),
            "the successor vouches for the session"
        );
        drop(second);
        assert!(!is_live(&roots, 5).unwrap());
    }
}
