//! A skill's reply that promised one JSON object (the connector digests): finding it in what the model wrote, and the
//! one repair turn a harness sends when it found none — shared by Glut and Claude Code, so both judge a reply alike
//! and alike with the dashboard's `firstJsonObject` (`core/skillRun.ts`).

/// What a skill that promised one JSON object is told when its reply held none
/// (a small model sometimes ends on prose about the JSON instead of the JSON).
/// It goes to the same session, so the data the skill collected is still there.
pub(crate) const JSON_REPAIR_PROMPT: &str = "Your last reply contained no JSON object. Do not call any tools \
    and do not explain anything: reply now with exactly the one JSON object the task asked for, \
    built from what you have already collected — nothing before it and nothing after it.";

/// How long the repair turn may take. It only has to write out what is known,
/// so it gets far less than a skill's own limit.
pub(crate) const JSON_REPAIR_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// The first JSON object in `text`, following the dashboard's `firstJsonObject`
/// (`core/skillRun.ts`) so both sides agree on whether a reply is usable: the
/// first balanced `{…}` that parses as a JSON object. A balanced `{…}` that is
/// not JSON (braces in the model's prose) is stepped over; an unterminated one
/// — a truncated reply — ends the search. Where the two differ, it is on the
/// safe side: a reply that ends in an unterminated object is `None` here, so the
/// repair runs, while the dashboard would hand back the prose braces and then
/// reject them as JSON.
pub(crate) fn find_json_object(text: &str) -> Option<&str> {
    let mut from = 0;
    while let Some(offset) = text[from..].find('{') {
        let start = from + offset;
        let end = balanced_object_end(text, start)?;
        let candidate = &text[start..=end];
        if matches!(
            serde_json::from_str::<serde_json::Value>(candidate),
            Ok(serde_json::Value::Object(_))
        ) {
            return Some(candidate);
        }
        from = end + 1;
    }
    None
}

/// The byte index of the `}` closing the object that opens at `start`, or
/// `None` when the text ends first. Braces inside JSON strings do not count.
fn balanced_object_end(text: &str, start: usize) -> Option<usize> {
    let (mut depth, mut in_string, mut escaped) = (0_usize, false, false);
    for (offset, byte) in text.as_bytes()[start..].iter().enumerate() {
        if in_string {
            match (escaped, byte) {
                (true, _) => escaped = false,
                (false, b'\\') => escaped = true,
                (false, b'"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + offset);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_json_object_is_found_behind_prose_and_before_a_truncated_copy() {
        assert_eq!(
            find_json_object("Now I have the data.\n{\"emails\": []}"),
            Some("{\"emails\": []}")
        );
        assert_eq!(find_json_object("{\"a\": 1}{\"a\": 2"), Some("{\"a\": 1}"));
        assert_eq!(
            find_json_object("{\"s\": \"a } and a {\"}"),
            Some("{\"s\": \"a } and a {\"}")
        );
    }

    #[test]
    fn a_balanced_brace_pair_in_prose_is_stepped_over() {
        let reply = "The format is {emails: [...]}, here it is:\n{\"emails\": [{\"id\": \"1\"}]}";
        assert_eq!(
            find_json_object(reply),
            Some("{\"emails\": [{\"id\": \"1\"}]}")
        );
    }

    #[test]
    fn prose_about_the_json_and_a_truncated_object_are_no_json_object() {
        // Live, run 988 (2026-09-28): the model described the JSON instead of writing it.
        assert_eq!(
            find_json_object("The mail digest has been compiled. The JSON output was produced."),
            None
        );
        assert_eq!(find_json_object("{\"emails\": [{\"id\": \"1\"}"), None);
        assert_eq!(find_json_object(""), None);
    }
}
