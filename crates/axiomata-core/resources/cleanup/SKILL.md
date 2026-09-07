---
name: cleanup
description: "General workspace-maintenance skill for tidying up files/entries the app itself doesn't manage the lifecycle of. Job 1 deletes Mail summary notes older than 10 days (by the email's date in the filename); Job 2 removes ToDo.md Done entries older than 10 days (by their completion-date stamp). Always writes a dated Markdown report of what it did, in addition to its reply. More jobs get their own numbered section as they're added; keep the name generic (cleanup), not tied to any one job."
backend: claude-code
allowed_tools: Bash(rm:*) Edit Write
---

# Cleanup

General workspace maintenance. Each job below is independent — do only the
job you were asked to run; if asked to "run cleanup" with no further detail,
do all jobs listed below, each exactly as scoped. Whatever you did, always
finish with the **Reporting** step at the end of this file — never skip it,
even if a job found nothing to delete.

## Job 1: old Mail summary notes

Delete old Mail summary notes so they don't accumulate forever. Deliberately
narrow — **only** files directly under `Mail/` matching the dated-note
pattern below. Do not touch anything else in the workspace, including
`Mail/.topics.md` (the owner's configured topics file — never a dated note,
but called out explicitly so it's never mistaken for one).

Do exactly this and nothing more:

1. List the files directly in the `Mail/` folder (your current directory is
   the workspace root) whose name matches `Mail/YYYY-MM-DD-*.md` — a 4-digit
   year, 2-digit month, 2-digit day, a dash, then anything, e.g.
   `Mail/2026-09-01-invoice-from-acme-3f9a1b2c.md`. Skip any file in `Mail/`
   that does not match this exact shape (in particular `Mail/.topics.md`,
   and anything without a leading date) — leave those alone entirely.
2. Compute today's date and the cutoff date 10 days ago, both as
   `YYYY-MM-DD`, e.g.: `date +%Y-%m-%d` and `date -v-10d +%Y-%m-%d` (this is
   macOS, so use the `-v` BSD `date` flag, not GNU's `-d`).
3. For each matching file, compare its `YYYY-MM-DD` date prefix against the
   cutoff from step 2 as plain strings (this works correctly — ISO dates
   sort lexicographically the same as chronologically). A file's date is
   the **email's own date** encoded in its name, not when the note file
   itself was created or last modified — never use `mtime`/`ls -t` or
   similar to decide age.
4. Delete every file whose date is strictly older than the cutoff (i.e.
   `file_date < cutoff_date`) with `rm`. Keep everything from the cutoff
   date onward.
5. Note for the report: how many notes were found, how many were deleted
   (their filenames), and how many were kept. If `Mail/` doesn't exist or
   has no matching files, note that plainly instead of treating it as an
   error.

## Job 2: old ToDo.md Done entries

Remove old completed entries from the workspace's `ToDo.md` so its `## Done`
section doesn't grow forever. Deliberately narrow — only entries under the
`## Done` heading that carry a completion date; everything else in the file
is left byte-for-byte untouched.

Do exactly this and nothing more:

1. Read `ToDo.md` at the workspace root. If it doesn't exist, note that in
   the report and stop this job — not an error.
2. Find the `## Done` heading. Everything from that heading to the end of
   the file (or the next heading, if there somehow is one) is the Done
   section. If there is no `## Done` heading, or it has no entries at all,
   note that and stop this job.
3. Within the Done section, only lines matching exactly
   `- [x] <text> (done: YYYY-MM-DD)` carry a known date. A line
   `- [x] <text>` with **no** `(done: ...)` suffix has no known completion
   date — never remove it, its age can't be judged. Leave it exactly as is.
4. Compute today's date and the cutoff 10 days ago the same way as Job 1
   (`date +%Y-%m-%d` / `date -v-10d +%Y-%m-%d`), and compare each dated
   entry's date against the cutoff the same way (plain `YYYY-MM-DD` string
   comparison).
5. Remove — delete the entire line — every dated Done entry whose date is
   strictly older than the cutoff. Keep every other line in the file
   exactly as it was: the open list above `## Done`, the heading itself,
   every dated entry from the cutoff date onward, and every undated entry.
6. Write the file back over `ToDo.md` with only those lines removed.
7. Note for the report: how many dated Done entries were found, how many
   were removed (quote their text), and how many were kept (in-cutoff or
   undated).

## Reporting

Whichever job(s) you ran, always do both of these at the end, in order:

1. Write a Markdown report to `Cleanup/YYYY-MM-DD-report.md` (today's date,
   `Cleanup/` is a plain top-level workspace folder — create it if it
   doesn't exist yet) with one section per job you ran, each listing what
   was found/removed/kept per that job's own notes above. Running this
   skill more than once on the same day overwrites that day's report, which
   is fine. If a job you didn't run has an earlier report entry from a
   previous day, don't touch previous reports — each day's report is its
   own file.
2. Reply with a short plain-text summary of what happened, ending with the
   report's path so the owner can open it (e.g. "See Cleanup/2026-09-06-report.md.").
