//! DAP's wire format: `Content-Length: N\r\n\r\n` and then N bytes of JSON, both directions.

use std::io::{BufRead, Write};

use serde_json::Value;

use crate::DapError;

/// The most one message may weigh — a variables or stack answer is far smaller; more is a broken or hostile peer.
pub const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;

/// Reads one message. `Ok(None)` at a clean end of the stream.
pub fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>, DapError> {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        if reader
            .read_line(&mut line)
            .map_err(|e| DapError::Io(e.to_string()))?
            == 0
        {
            return if length.is_none() {
                Ok(None)
            } else {
                Err(DapError::Protocol(
                    "the adapter closed in the middle of a header".into(),
                ))
            };
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = Some(value.trim().parse().map_err(|_| {
                DapError::Protocol(format!("bad Content-Length “{}”", value.trim()))
            })?);
        }
        // Other headers (Content-Type) carry nothing we use.
    }
    let length =
        length.ok_or_else(|| DapError::Protocol("a message without Content-Length".into()))?;
    if length > MAX_MESSAGE_BYTES {
        return Err(DapError::Protocol(format!(
            "a message of {length} bytes is over the limit"
        )));
    }
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|e| DapError::Io(e.to_string()))?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| DapError::Protocol(format!("not JSON: {e}")))
}

/// Writes one message with its header.
pub fn write_message(writer: &mut impl Write, message: &Value) -> Result<(), DapError> {
    let body = serde_json::to_vec(message).map_err(|e| DapError::Protocol(e.to_string()))?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())
        .map_err(|e| DapError::Io(e.to_string()))?;
    writer
        .write_all(&body)
        .map_err(|e| DapError::Io(e.to_string()))?;
    writer.flush().map_err(|e| DapError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_message_round_trips_and_two_in_a_row_are_read_one_by_one() {
        let mut wire = Vec::new();
        write_message(
            &mut wire,
            &json!({"seq": 1, "type": "request", "command": "initialize"}),
        )
        .unwrap();
        write_message(
            &mut wire,
            &json!({"seq": 2, "type": "event", "event": "initialized"}),
        )
        .unwrap();
        let mut reader = std::io::BufReader::new(&wire[..]);
        assert_eq!(
            read_message(&mut reader).unwrap().unwrap()["command"],
            "initialize"
        );
        assert_eq!(
            read_message(&mut reader).unwrap().unwrap()["event"],
            "initialized"
        );
        assert!(read_message(&mut reader).unwrap().is_none());
    }

    #[test]
    fn the_length_counts_bytes_not_characters() {
        let mut wire = Vec::new();
        write_message(&mut wire, &json!({"text": "Größe ✓"})).unwrap();
        let mut reader = std::io::BufReader::new(&wire[..]);
        assert_eq!(
            read_message(&mut reader).unwrap().unwrap()["text"],
            "Größe ✓"
        );
    }

    #[test]
    fn broken_input_is_an_error_not_a_hang_or_a_panic() {
        let cases: [&[u8]; 4] = [
            b"Content-Length: abc\r\n\r\n",
            b"X-Other: 1\r\n\r\n{}",
            b"Content-Length: 5\r\n\r\n{",
            b"Content-Length: 4\r\n\r\nnope",
        ];
        for case in cases {
            assert!(
                read_message(&mut std::io::BufReader::new(case)).is_err(),
                "{:?}",
                String::from_utf8_lossy(case)
            );
        }
        let huge = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1);
        assert!(read_message(&mut std::io::BufReader::new(huge.as_bytes())).is_err());
    }

    #[test]
    fn extra_headers_are_ignored() {
        let wire = b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\nContent-Length: 2\r\n\r\n{}";
        assert!(
            read_message(&mut std::io::BufReader::new(&wire[..]))
                .unwrap()
                .is_some()
        );
    }
}
