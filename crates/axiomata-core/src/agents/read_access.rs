//! What a session that may only look runs without asking (`docs/plans/harness-choice.md`, "Read access"): the same
//! lists for every harness, so a reviewer on Glut and one on Claude Code read alike.

/// The read-only git commands that reach beyond the checkout: `--no-index` reads any file, `--output` writes one —
/// matched in their shortest unambiguous forms, which git takes too (`--outp=f`). Denied after [`READ_COMMANDS`]
/// allow them, on every harness (`docs/plans/harness-choice.md`, "Read access").
pub(crate) const READ_DENIED_COMMANDS: [&str; 2] = ["git * --no-i*", "git * --ou*"];

/// The commands a reader runs without asking: git's read-only ones and a look at the folder it is in. Nothing that
/// takes a program or a path to write to as an argument (`find -exec`, `rg --pre`, a pager), which a pattern cannot
/// tell apart, and `ls` without a path, so it does not list folders outside the checkout. Shared by every harness: a
/// trailing ` *` means "with any arguments".
pub(crate) const READ_COMMANDS: [&str; 13] = [
    "git status",
    "git status *",
    "git log",
    "git log *",
    "git diff",
    "git diff *",
    "git show",
    "git show *",
    "git rev-parse *",
    "git branch --show-current",
    "ls",
    "ls -la",
    "pwd",
];
