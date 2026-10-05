//! What a session has used so far: tokens and steps, read out of what its harness leaves behind
//! (`docs/plans/a2a.md`, A9 and A33).
//!
//! Nothing here enforces anything or knows a price: it only counts. Two sources, one per harness, both read
//! tolerantly — a field that is missing counts 0, a line that is not JSON is skipped — because both formats belong to
//! somebody else and change without notice. A source that yields nothing reads as "nothing used", which is why the
//! caller also limits **steps**: those come from the same files, but a tool call is the one thing every format has.
//!
//! * **Claude Code** writes a transcript (`~/.claude/projects/<folder>/<session id>.jsonl`), one line per content block
//!   of a reply, and every line of one reply repeats that reply's `usage`. Counting lines would count a reply as many
//!   times as it has blocks, so a reply is counted once by its message id ([`ClaudeTally`]).
//! * **Opencode** returns its messages over the API; an assistant message carries `tokens` and its tool calls as
//!   content parts of the type `tool` ([`opencode_usage`]).
//!
//! Tokens are input, output and cache *writes*; cache *reads* are left out (A27) — they cost a tenth and, on a long
//! session, would be most of the count without saying anything about the work.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

/// What a session has used.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    /// Input tokens: what was read fresh plus what was written into the cache.
    pub input_tokens: u64,
    /// Output tokens, thinking included.
    pub output_tokens: u64,
    /// Tool calls.
    pub steps: u32,
}

impl Usage {
    /// Input and output together — what a token limit is measured against.
    pub fn tokens(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }

    /// The larger of two measurements, field by field: a figure that fell (a rewritten file, a window that moved on) is
    /// not a session that used less.
    pub fn at_least(self, other: Usage) -> Usage {
        Usage {
            input_tokens: self.input_tokens.max(other.input_tokens),
            output_tokens: self.output_tokens.max(other.output_tokens),
            steps: self.steps.max(other.steps),
        }
    }

    /// The sum of two measurements (a session that was started again keeps counting).
    pub fn plus(self, other: Usage) -> Usage {
        Usage {
            input_tokens: self.input_tokens.saturating_add(other.input_tokens),
            output_tokens: self.output_tokens.saturating_add(other.output_tokens),
            steps: self.steps.saturating_add(other.steps),
        }
    }
}

/// The most one look reads of a transcript. The file is written by the harness but sits where the session's own tools can
/// reach it, so a look must cost the same however large somebody made it; a transcript longer than this is caught up on
/// over the next looks.
const MAX_READ_PER_LOOK: u64 = 8 * 1024 * 1024;
/// A line longer than this is not a transcript line; it is skipped instead of waited for.
const MAX_LINE_BYTES: u64 = 1024 * 1024;
/// The most replies and tool calls remembered: far beyond any real session, and a bound on what a forged file can make
/// the app hold.
const MAX_ENTRIES: usize = 200_000;

fn number(value: Option<&Value>) -> u64 {
    value.and_then(Value::as_u64).unwrap_or(0)
}

/// A Claude Code transcript, read from where the last look stopped.
///
/// Holds one entry per reply (not per line) and the ids of the tool calls it saw, so a second look only reads what was
/// appended since and still adds up to the same figure as one look at the whole file.
#[derive(Debug, Default)]
pub struct ClaudeTally {
    /// How far the file was consumed: always the end of a complete line.
    offset: u64,
    /// Reply id → (input, output). A later line of the same reply overwrites with the larger figures: a reply that
    /// was still streaming when its first line was written has fewer output tokens there than at its end.
    replies: HashMap<String, (u64, u64)>,
    tools: HashSet<String>,
    /// What an earlier reading of a file that has since shrunk had counted: truncating its own transcript must not
    /// zero what a session used.
    floor: Usage,
}

impl ClaudeTally {
    /// Everything counted so far.
    pub fn usage(&self) -> Usage {
        let (input, output) = self
            .replies
            .values()
            .fold((0u64, 0u64), |(i, o), (ri, ro)| {
                (i.saturating_add(*ri), o.saturating_add(*ro))
            });
        Usage {
            input_tokens: input,
            output_tokens: output,
            steps: u32::try_from(self.tools.len()).unwrap_or(u32::MAX),
        }
        .at_least(self.floor)
    }

    /// Reads what was appended to the transcript at `path` since the last call, at most [`MAX_READ_PER_LOOK`].
    ///
    /// A file that shrank was replaced or cut: it is read from the start again, and what was counted before stays as
    /// the floor. A last line without its newline is left for the next call, the harness may be in the middle of
    /// writing it — unless it is longer than any line can be, then it is skipped. A link is not followed.
    ///
    /// # Errors
    ///
    /// Whatever opening or reading the file says, and [`io::ErrorKind::InvalidInput`] for something that is not a
    /// plain file; a file that does not exist yet is the caller's to expect.
    pub fn advance(&mut self, path: &Path) -> io::Result<()> {
        if !std::fs::symlink_metadata(path)?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a transcript is a plain file",
            ));
        }
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();
        if len < self.offset {
            *self = ClaudeTally {
                floor: self.usage(),
                ..ClaudeTally::default()
            };
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut chunk = Vec::new();
        file.take(MAX_READ_PER_LOOK).read_to_end(&mut chunk)?;
        let complete = chunk
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |at| at + 1);
        self.feed(&String::from_utf8_lossy(&chunk[..complete]));
        self.offset += complete as u64;
        if complete == 0 && chunk.len() as u64 >= MAX_LINE_BYTES {
            self.offset += chunk.len() as u64;
        }
        Ok(())
    }

    /// Counts the lines of `text`. Public for a caller that holds the text already.
    pub fn feed(&mut self, text: &str) {
        for line in text.lines() {
            let Ok(entry) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if entry.get("type").and_then(Value::as_str) != Some("assistant") {
                continue;
            }
            let Some(message) = entry.get("message") else {
                continue;
            };
            if let Some(id) = message.get("id").and_then(Value::as_str)
                && let Some(usage) = message.get("usage")
            {
                let input = number(usage.get("input_tokens"))
                    .saturating_add(number(usage.get("cache_creation_input_tokens")));
                let output = number(usage.get("output_tokens"));
                if self.replies.len() < MAX_ENTRIES || self.replies.contains_key(id) {
                    let reply = self.replies.entry(id.to_owned()).or_default();
                    reply.0 = reply.0.max(input);
                    reply.1 = reply.1.max(output);
                }
            }
            for block in message
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if block.get("type").and_then(Value::as_str) == Some("tool_use")
                    && let Some(id) = block.get("id").and_then(Value::as_str)
                    && self.tools.len() < MAX_ENTRIES
                {
                    self.tools.insert(id.to_owned());
                }
            }
        }
    }
}

/// What the messages of an Opencode session add up to. `messages` is the whole session, in any order: a figure is taken
/// per message, so the order does not matter.
pub fn opencode_usage(messages: &[Value]) -> Usage {
    let mut usage = Usage::default();
    for message in messages {
        if message.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        if let Some(tokens) = message.get("tokens") {
            let input =
                number(tokens.get("input")).saturating_add(number(tokens.pointer("/cache/write")));
            let output =
                number(tokens.get("output")).saturating_add(number(tokens.get("reasoning")));
            usage.input_tokens = usage.input_tokens.saturating_add(input);
            usage.output_tokens = usage.output_tokens.saturating_add(output);
        }
        let calls = message
            .get("content")
            .and_then(Value::as_array)
            .map_or(0, |parts| {
                parts
                    .iter()
                    .filter(|part| part.get("type").and_then(Value::as_str) == Some("tool"))
                    .count()
            });
        usage.steps = usage
            .steps
            .saturating_add(u32::try_from(calls).unwrap_or(u32::MAX));
    }
    usage
}

/// A new random session id in the spelling Claude Code's `--session-id` wants (a version 4 UUID), so the transcript's
/// name is known before the harness starts.
///
/// # Errors
///
/// The system's source of randomness failing.
pub fn new_claude_session_id() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|err| io::Error::other(err.to_string()))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

/// Whether `text` is a session id of the shape [`new_claude_session_id`] makes. A file name is built from it.
pub fn is_claude_session_id(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(at, byte)| match at {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => matches!(byte, b'0'..=b'9' | b'a'..=b'f'),
        })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use serde_json::json;

    use super::*;

    fn reply(id: &str, input: u64, cache_write: u64, output: u64, block: Value) -> String {
        json!({"type": "assistant", "message": {
            "id": id,
            "usage": {"input_tokens": input, "cache_creation_input_tokens": cache_write,
                      "cache_read_input_tokens": 90_000, "output_tokens": output},
            "content": [block],
        }})
        .to_string()
    }

    fn tool(id: &str) -> Value {
        json!({"type": "tool_use", "id": id})
    }

    #[test]
    fn a_reply_written_as_several_lines_is_counted_once() {
        let mut tally = ClaudeTally::default();
        let text = [
            reply("m1", 2, 1_000, 300, json!({"type": "thinking"})),
            reply("m1", 2, 1_000, 300, json!({"type": "text"})),
            reply("m1", 2, 1_000, 300, tool("t1")),
        ]
        .join("\n");
        tally.feed(&text);
        assert_eq!(
            tally.usage(),
            Usage {
                input_tokens: 1_002,
                output_tokens: 300,
                steps: 1
            }
        );
    }

    #[test]
    fn cache_reads_are_not_counted_and_cache_writes_are() {
        let mut tally = ClaudeTally::default();
        tally.feed(&reply("m1", 10, 500, 5, tool("t1")));
        assert_eq!(tally.usage().input_tokens, 510);
    }

    #[test]
    fn a_reply_still_streaming_in_its_first_line_ends_with_its_larger_figures() {
        let mut tally = ClaudeTally::default();
        let text = [
            reply("m1", 2, 100, 1, json!({"type": "text"})),
            reply("m1", 2, 100, 250, tool("t1")),
        ]
        .join("\n");
        tally.feed(&text);
        assert_eq!(tally.usage().output_tokens, 250);
    }

    #[test]
    fn different_replies_and_tool_calls_add_up_and_a_repeated_tool_id_does_not() {
        let mut tally = ClaudeTally::default();
        let text = [
            reply("m1", 1, 100, 10, tool("t1")),
            reply("m2", 1, 200, 20, tool("t2")),
            reply("m2", 1, 200, 20, tool("t2")),
        ]
        .join("\n");
        tally.feed(&text);
        let usage = tally.usage();
        assert_eq!(
            (usage.input_tokens, usage.output_tokens, usage.steps),
            (302, 30, 2)
        );
    }

    #[test]
    fn lines_that_are_not_replies_or_not_json_are_skipped() {
        let mut tally = ClaudeTally::default();
        tally.feed("not json\n{\"type\":\"user\",\"message\":{}}\n{\"type\":\"assistant\"}\n");
        tally.feed("{\"type\":\"assistant\",\"message\":{\"id\":\"m\",\"usage\":{}}}\n");
        assert_eq!(tally.usage(), Usage::default());
    }

    fn temp_file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("axiomata-usage-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn a_second_look_reads_only_what_was_appended_and_adds_up_to_the_same_figure() {
        let path = temp_file("append.jsonl");
        let first = reply("m1", 1, 100, 10, tool("t1"));
        let second = reply("m2", 1, 200, 20, tool("t2"));
        fs::write(&path, format!("{first}\n")).unwrap();
        let mut tally = ClaudeTally::default();
        tally.advance(&path).unwrap();
        assert_eq!(tally.usage().steps, 1);
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{second}").unwrap();
        tally.advance(&path).unwrap();
        let mut whole = ClaudeTally::default();
        whole.advance(&path).unwrap();
        assert_eq!(tally.usage(), whole.usage());
        assert_eq!(tally.usage().steps, 2);
    }

    #[test]
    fn a_line_still_being_written_is_left_for_the_next_look() {
        let path = temp_file("partial.jsonl");
        let line = reply("m1", 1, 100, 10, tool("t1"));
        let (head, rest) = line.split_at(20);
        fs::write(&path, head).unwrap();
        let mut tally = ClaudeTally::default();
        tally.advance(&path).unwrap();
        assert_eq!(tally.usage(), Usage::default());
        fs::write(&path, format!("{head}{rest}\n")).unwrap();
        tally.advance(&path).unwrap();
        assert_eq!(tally.usage().steps, 1);
    }

    #[test]
    fn a_file_that_shrank_is_read_again_but_never_counts_for_less_than_before() {
        let path = temp_file("shrink.jsonl");
        fs::write(
            &path,
            format!(
                "{}\n{}\n",
                reply("m1", 1, 100, 10, tool("t1")),
                reply("m2", 1, 100, 10, tool("t2"))
            ),
        )
        .unwrap();
        let mut tally = ClaudeTally::default();
        tally.advance(&path).unwrap();
        let before = tally.usage();
        fs::write(&path, format!("{}\n", reply("m9", 1, 7, 3, tool("t9")))).unwrap();
        tally.advance(&path).unwrap();
        assert_eq!(
            tally.usage(),
            before,
            "cutting its own transcript does not zero a session"
        );
    }

    #[test]
    fn a_line_longer_than_any_line_can_be_is_skipped_not_waited_for() {
        let path = temp_file("long.jsonl");
        let mut text = "x".repeat(usize::try_from(MAX_LINE_BYTES).unwrap() + 10);
        fs::write(&path, &text).unwrap();
        let mut tally = ClaudeTally::default();
        tally.advance(&path).unwrap();
        text.push('\n');
        text.push_str(&reply("m1", 1, 100, 10, tool("t1")));
        text.push('\n');
        fs::write(&path, &text).unwrap();
        tally.advance(&path).unwrap();
        assert_eq!(
            tally.usage().steps,
            1,
            "the file goes on being read after the skipped line"
        );
    }

    #[test]
    fn a_link_is_not_a_transcript() {
        let target = temp_file("target.jsonl");
        fs::write(&target, format!("{}\n", reply("m1", 1, 1, 1, tool("t1")))).unwrap();
        let link = temp_file("link.jsonl");
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut tally = ClaudeTally::default();
        assert_eq!(
            tally.advance(&link).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn a_missing_transcript_is_an_error_for_the_caller_to_expect() {
        let mut tally = ClaudeTally::default();
        assert!(tally.advance(&temp_file("missing.jsonl")).is_err());
    }

    #[test]
    fn opencode_messages_add_up_tokens_and_tool_parts() {
        let messages = vec![
            json!({"type": "user", "content": []}),
            json!({"type": "assistant",
                   "tokens": {"input": 362, "output": 102, "reasoning": 28, "cache": {"read": 28_544, "write": 10}},
                   "content": [{"type": "reasoning"}, {"type": "tool", "id": "a"}, {"type": "tool", "id": "b"}]}),
            json!({"type": "assistant", "tokens": {"input": 5, "output": 1}, "content": [{"type": "text"}]}),
            json!({"type": "idle", "outcome": "succeeded"}),
        ];
        assert_eq!(
            opencode_usage(&messages),
            Usage {
                input_tokens: 377,
                output_tokens: 131,
                steps: 2
            }
        );
    }

    #[test]
    fn an_assistant_message_without_tokens_counts_nothing_but_its_tools() {
        let messages = vec![json!({"type": "assistant", "content": [{"type": "tool"}]})];
        assert_eq!(
            opencode_usage(&messages),
            Usage {
                input_tokens: 0,
                output_tokens: 0,
                steps: 1
            }
        );
    }

    #[test]
    fn a_made_session_id_is_one_claude_accepts_and_this_module_recognises() {
        let id = new_claude_session_id().unwrap();
        assert!(is_claude_session_id(&id), "{id}");
        assert_eq!(&id[14..15], "4");
        assert_ne!(id, new_claude_session_id().unwrap());
        assert!(!is_claude_session_id("../../etc/passwd"));
        assert!(!is_claude_session_id(&id.to_uppercase()));
    }

    #[test]
    fn two_measurements_add_up() {
        let a = Usage {
            input_tokens: 1,
            output_tokens: 2,
            steps: 3,
        };
        assert_eq!(a.plus(a).tokens(), 6);
        assert_eq!(a.plus(a).steps, 6);
    }
}
