//! What a tool answers when its handler may wait for another thread: the
//! structured value, an optional text for the model in place of the JSON
//! mirror, and pictures.

use std::{future::Future, pin::Pin};

use serde_json::Value;

use crate::{HARD_RESULT_BYTES, ResourceContent, SurfaceError, cap_result};

/// A picture in a tool's answer.
#[derive(Clone, PartialEq, Eq)]
pub struct ToolImage {
    pub mime: &'static str,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for ToolImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The bytes of a screenshot would drown any log.
        f.debug_struct("ToolImage")
            .field("mime", &self.mime)
            .field("bytes", &self.data.len())
            .finish()
    }
}

/// A tool's answer. A plain JSON value converts into one.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolReply {
    /// `structuredContent`: always a JSON object.
    pub structured: Value,
    /// What the model reads. `None`: the JSON of `structured`.
    pub text: Option<String>,
    pub images: Vec<ToolImage>,
}

impl ToolReply {
    #[must_use]
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    #[must_use]
    pub fn with_image(mut self, mime: &'static str, data: Vec<u8>) -> Self {
        self.images.push(ToolImage { mime, data });
        self
    }

    /// The answer with each copy, the structured value and the text, at
    /// most `per_copy` bytes (never more than half of
    /// [`HARD_RESULT_BYTES`]): what a surface that runs its tools itself
    /// does to every answer, as the registry does to its handlers'.
    /// Pictures are counted apart.
    #[must_use]
    pub fn capped(mut self, per_copy: usize) -> Self {
        let per_copy = per_copy.min(HARD_RESULT_BYTES / 2);
        self.structured = cap_result(self.structured, per_copy);
        if let Some(text) = &mut self.text {
            cut_text(text, per_copy);
        }
        self
    }
}

impl From<Value> for ToolReply {
    fn from(structured: Value) -> Self {
        ToolReply {
            structured,
            text: None,
            images: Vec::new(),
        }
    }
}

/// Cuts `text` to at most `max` bytes on a character boundary, saying so
/// at its end.
fn cut_text(text: &mut String, max: usize) {
    if text.len() <= max {
        return;
    }
    let note = format!("\n… cut at {max} bytes; ask for less");
    let mut cut = max.saturating_sub(note.len());
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
    if note.len() <= max {
        text.push_str(&note);
    }
}

/// The future of [`crate::McpSurface::call_tool`].
pub type ToolFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ToolReply, SurfaceError>> + Send + 'a>>;

/// The future of [`crate::McpSurface::read_resource_async`].
pub type ResourceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<ResourceContent>, SurfaceError>> + Send + 'a>>;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding, as MCP carries image data. Written here
/// because the default build has no other use for a base64 crate.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// The inverse of [`base64_encode`]. `None` for anything that is not
/// padded standard base64.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let last = bytes.len() / 4;
    for (i, chunk) in bytes.chunks(4).enumerate() {
        let pad = chunk.iter().rev().take_while(|c| **c == b'=').count();
        if pad > 2 || (pad > 0 && i + 1 != last) {
            return None;
        }
        let mut n = 0u32;
        for c in &chunk[..4 - pad] {
            n = (n << 6) | val(*c)?;
        }
        n <<= 6 * pad as u32;
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capped_reply_fits_both_copies() {
        let reply = ToolReply::from(serde_json::json!({
            "outer": { "inner": { "list": (0..5000).map(|i| format!("row-{i}")).collect::<Vec<_>>() } },
        }))
        .with_text("é".repeat(50_000))
        .capped(4096);
        assert!(reply.structured.to_string().len() <= 4096);
        let text = reply.text.unwrap();
        assert!(text.len() <= 4096);
        assert!(text.ends_with("ask for less"));
        assert_eq!(reply.structured["truncated"], true);
    }

    #[test]
    fn a_reply_that_fits_is_left_alone() {
        let reply = ToolReply::from(serde_json::json!({ "a": [1, 2] })).with_text("short");
        assert_eq!(reply.clone().capped(4096), reply);
    }

    #[test]
    fn a_capped_reply_never_passes_half_the_hard_cap() {
        let reply = ToolReply::from(serde_json::json!({})).with_text("x".repeat(200_000));
        let text = reply.capped(usize::MAX).text.unwrap();
        assert!(text.len() <= HARD_RESULT_BYTES / 2);
    }

    #[test]
    fn base64_round_trips_every_padding() {
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_encode(plain.as_bytes()), coded);
            assert_eq!(base64_decode(coded).unwrap(), plain.as_bytes());
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(base64_decode(&base64_encode(&all)).unwrap(), all);
    }

    #[test]
    fn base64_refuses_what_is_not_base64() {
        assert_eq!(base64_decode("Zg="), None);
        assert_eq!(base64_decode("Z=g="), None);
        assert_eq!(base64_decode("Zg==Zg=="), None);
        assert_eq!(base64_decode("Zm9*"), None);
    }
}
