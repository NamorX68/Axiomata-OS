//! Identifiers that become file names, branch names or table keys.

use crate::error::{Result, RosterError};

/// Longest identifier accepted. A role name becomes a directory and later part of a worktree and branch name.
pub const MAX_IDENT_LEN: usize = 48;

/// Checks a lower-case slug: ASCII letters and digits, with `-` or `_` between them.
///
/// Lower-case only, not politeness: the name becomes a directory under a case-insensitive macOS file system, so
/// `Builder` and `builder` would be two roles pointing at one folder (the same reasoning as `ide_agents.name`).
///
/// # Errors
///
/// [`RosterError::Invalid`] naming `field`.
pub fn check_slug(field: &'static str, value: &str) -> Result<()> {
    if value.is_empty() || value.len() > MAX_IDENT_LEN {
        return Err(RosterError::invalid(
            field,
            format!("must be 1 to {MAX_IDENT_LEN} characters"),
        ));
    }
    let bytes = value.as_bytes();
    let edge_ok = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let inner_ok = |b: u8| edge_ok(b) || b == b'-' || b == b'_';
    if !edge_ok(bytes[0])
        || !edge_ok(bytes[bytes.len() - 1])
        || !bytes.iter().copied().all(inner_ok)
    {
        return Err(RosterError::invalid(
            field,
            "use lower-case letters and digits, with - or _ between them",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_lower_case_words_that_are_safe_as_a_directory() {
        for ok in ["a", "builder", "impl-light", "review_2", "x9"] {
            assert!(check_slug("name", ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "Builder",
            "-a",
            "a-",
            "a b",
            "a/b",
            "..",
            "a.b",
            "ä",
            &"a".repeat(49),
        ] {
            assert!(
                matches!(
                    check_slug("name", bad),
                    Err(RosterError::Invalid { field: "name", .. })
                ),
                "{bad:?}"
            );
        }
    }
}
