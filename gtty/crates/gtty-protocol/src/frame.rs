use std::error::Error;
use std::fmt::{self, Display, Formatter};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Hard limit for a single newline-delimited JSON frame.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub enum FrameError {
    Empty,
    TooLarge { actual: usize, maximum: usize },
    Json(serde_json::Error),
}

impl Display for FrameError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("IPC frame is empty"),
            Self::TooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "IPC frame is {actual} bytes; maximum is {maximum}"
                )
            }
            Self::Json(error) => write!(formatter, "invalid IPC JSON: {error}"),
        }
    }
}

impl Error for FrameError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Empty | Self::TooLarge { .. } => None,
        }
    }
}

/// Serializes a message as one newline-terminated JSON frame.
pub fn encode_frame<T: Serialize>(message: &T) -> Result<Vec<u8>, FrameError> {
    let mut frame = serde_json::to_vec(message).map_err(FrameError::Json)?;
    if frame.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge {
            actual: frame.len(),
            maximum: MAX_FRAME_BYTES,
        });
    }
    frame.push(b'\n');
    Ok(frame)
}

/// Deserializes one JSON frame. A single trailing newline is optional.
pub fn decode_frame<T: DeserializeOwned>(frame: &[u8]) -> Result<T, FrameError> {
    let frame = frame.strip_suffix(b"\n").unwrap_or(frame);
    let frame = frame.strip_suffix(b"\r").unwrap_or(frame);
    if frame.is_empty() {
        return Err(FrameError::Empty);
    }
    if frame.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge {
            actual: frame.len(),
            maximum: MAX_FRAME_BYTES,
        });
    }
    serde_json::from_slice(frame).map_err(FrameError::Json)
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct Example {
        text: String,
    }

    #[test]
    fn frame_round_trip() {
        let value = Example {
            text: "hello\nworld".into(),
        };
        let encoded = encode_frame(&value).expect("message must encode");
        assert_eq!(encoded.last(), Some(&b'\n'));
        assert_eq!(decode_frame::<Example>(&encoded).unwrap(), value);
    }

    #[test]
    fn oversized_frame_is_rejected_before_json_parsing() {
        let frame = vec![b'x'; MAX_FRAME_BYTES + 1];
        assert!(matches!(
            decode_frame::<Example>(&frame),
            Err(FrameError::TooLarge { .. })
        ));
    }
}
