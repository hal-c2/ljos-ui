//! What is said to an inspector and what it says, with nothing that says
//! it (ARCHITECTURE.md §19.7): the JSON-RPC requests, a tool's answer, the
//! token file's format and the calls a second it may state. Compiled
//! whatever the features, so that a program with a transport of its own, a
//! page in a browser say, shares them without linking `rmcp` or `tokio`.

use serde_json::{Value as Json, json};

use crate::{Cap, CapSet};

/// The protocol revision this client speaks; the inspector answers with
/// the one it will.
pub const PROTOCOL: &str = "2025-06-18";

/// A JSON-RPC request.
pub fn request(id: u64, method: &str, params: &Json) -> Json {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

/// A JSON-RPC notification, which nothing answers.
pub fn notification(method: &str) -> Json {
    json!({ "jsonrpc": "2.0", "method": method })
}

/// The parameters of `initialize` from the client `name`.
pub fn initialize_params(name: &str, version: &str) -> Json {
    json!({
        "protocolVersion": PROTOCOL,
        "capabilities": {},
        "clientInfo": { "name": name, "version": version },
    })
}

/// The parameters of `tools/call`.
pub fn tool_call_params(name: &str, args: &Json) -> Json {
    json!({ "name": name, "arguments": args })
}

/// The parameters of `tools/list`, from `cursor` when it is not the first
/// page.
pub fn tool_list_params(cursor: Option<&str>) -> Json {
    match cursor {
        Some(cursor) => json!({ "cursor": cursor }),
        None => json!({}),
    }
}

/// A picture a tool answered with, as it came: its MIME type and its
/// bytes, decoded from the base64 the protocol carries them in.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    pub mime: String,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The bytes of a screenshot would drown any log.
        f.debug_struct("Image")
            .field("mime", &self.mime)
            .field("bytes", &self.data.len())
            .finish()
    }
}

/// An image block whose data does not decode.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("an image ({mime}) that is not base64")]
pub struct BadImage {
    pub mime: String,
}

/// A tool's whole answer: what a model reads, the pictures, and the
/// structured value beside them, as the inspector gave them.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    /// `structuredContent`; `Null` when the tool gave none.
    pub structured: Json,
    /// The text blocks, one line apart; `None` when there were none.
    pub text: Option<String>,
    pub images: Vec<Image>,
}

impl ToolResult {
    /// The `result` of a `tools/call`.
    ///
    /// # Errors
    /// An image in it does not decode.
    pub fn of(result: &Json) -> Result<ToolResult, BadImage> {
        let mut texts = Vec::new();
        let mut images = Vec::new();
        for block in result["content"].as_array().into_iter().flatten() {
            match block["type"].as_str() {
                Some("text") => texts.push(block["text"].as_str().unwrap_or("")),
                Some("image") => {
                    let mime = block["mimeType"].as_str().unwrap_or("").to_owned();
                    let Some(data) = block["data"].as_str().and_then(crate::base64_decode) else {
                        return Err(BadImage { mime });
                    };
                    images.push(Image { mime, data });
                }
                // Audio and resources are kept in the result, not read here.
                _ => {}
            }
        }
        Ok(ToolResult {
            structured: result
                .get("structuredContent")
                .cloned()
                .unwrap_or(Json::Null),
            text: (!texts.is_empty()).then(|| texts.join("\n")),
            images,
        })
    }
}

/// What a tool that ran and refused (`isError`) said.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    /// The word the tool gave (`stale_ref`, `capability_denied`, …).
    pub code: Option<String>,
    pub message: String,
    pub details: Option<Json>,
    /// Everything the refusal carried.
    pub result: ToolResult,
}

impl Refusal {
    /// The `result` of a `tools/call` whose `isError` is true.
    ///
    /// # Errors
    /// An image in it does not decode.
    pub fn of(result: &Json) -> Result<Refusal, BadImage> {
        let parsed = ToolResult::of(result)?;
        let error = &result["structuredContent"]["error"];
        Ok(Refusal {
            code: error["code"].as_str().map(str::to_owned),
            message: error["message"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| parsed.text.clone())
                .unwrap_or_else(|| "refused".to_owned()),
            details: error.get("details").cloned(),
            result: parsed,
        })
    }
}

/// The calls a second a token is answered when its launcher says nothing
/// (§19.7).
pub const DEFAULT_CALLS_PER_SECOND: u32 = 20;

/// The calls a second a launcher may grant. At least one, or nothing is
/// answered at all. At most 1000: a program driving its child as a person
/// works makes a few calls a gesture, which a few hundred cover with room
/// to spare, and the limit keeps one call's mark per call in its window,
/// so 1000 is still a few kilobytes. Past that it is no longer a bound.
pub const CALLS_PER_SECOND: std::ops::RangeInclusive<u32> = 1..=1000;

/// A rate outside [`CALLS_PER_SECOND`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{given:?} is not a rate the inspector answers at: give the calls a second as a whole \
     number from {} to {}",
    CALLS_PER_SECOND.start(),
    CALLS_PER_SECOND.end()
)]
pub struct RateError {
    pub given: String,
}

/// `text` as calls a second, when it is a whole number in
/// [`CALLS_PER_SECOND`].
pub fn parse_calls_per_second(text: &str) -> Result<u32, RateError> {
    text.trim()
        .parse::<u32>()
        .ok()
        .and_then(|n| check_calls_per_second(n).ok())
        .ok_or_else(|| RateError {
            given: text.to_owned(),
        })
}

/// `n` when it is in [`CALLS_PER_SECOND`].
pub fn check_calls_per_second(n: u32) -> Result<u32, RateError> {
    if CALLS_PER_SECOND.contains(&n) {
        Ok(n)
    } else {
        Err(RateError {
            given: n.to_string(),
        })
    }
}

/// What a token file holds. Whether to believe it, and getting it to and
/// from a disk, is for whoever has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenFileBody {
    pub url: String,
    /// 64 hexadecimal digits.
    pub token: String,
    pub pid: Option<u32>,
    pub capabilities: CapSet,
    /// `None` from an inspector that states none.
    pub calls_per_second: Option<u32>,
}

/// Why a text is not a token file.
#[derive(Debug, thiserror::Error)]
pub enum TokenFormatError {
    #[error("not JSON: {0}")]
    NotJson(#[from] serde_json::Error),
    #[error("no {0:?}")]
    Missing(&'static str),
    #[error("no inspector's token")]
    NotAToken,
    #[error(
        "a rate no inspector answers at: calls_per_second is from {} to {}",
        CALLS_PER_SECOND.start(),
        CALLS_PER_SECOND.end()
    )]
    BadRate,
}

impl TokenFileBody {
    /// The file's text, one line to a key and a newline at the end.
    pub fn to_text(&self) -> String {
        let mut body = json!({
            "url": self.url,
            "token": self.token,
            "capabilities": self.capabilities,
        });
        if let Some(pid) = self.pid {
            body["pid"] = json!(pid);
        }
        if let Some(rate) = self.calls_per_second {
            body["calls_per_second"] = json!(rate);
        }
        let mut text = serde_json::to_string_pretty(&body).unwrap_or_default();
        text.push('\n');
        text
    }

    /// What `text` says.
    ///
    /// A capability this build does not know is left out: it is a later
    /// inspector's, and what it grants is judged by the inspector anyway.
    ///
    /// # Errors
    /// It is not JSON, a field is missing, or the token or rate is not what
    /// an inspector writes.
    pub fn parse(text: &str) -> Result<TokenFileBody, TokenFormatError> {
        let body: Json = serde_json::from_str(text)?;
        let field = |field: &'static str| {
            body[field]
                .as_str()
                .map(str::to_owned)
                .ok_or(TokenFormatError::Missing(field))
        };
        let url = field("url")?;
        let token = field("token")?;
        // An inspector's token is 256 random bits in hex; anything else was
        // not written by one.
        if token.len() != 64 || !token.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(TokenFormatError::NotAToken);
        }
        let calls_per_second = match &body["calls_per_second"] {
            Json::Null => None,
            rate => Some(
                rate.as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .and_then(|n| check_calls_per_second(n).ok())
                    .ok_or(TokenFormatError::BadRate)?,
            ),
        };
        Ok(TokenFileBody {
            url,
            token,
            pid: body["pid"].as_u64().and_then(|p| u32::try_from(p).ok()),
            capabilities: body["capabilities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Json::as_str)
                .filter_map(|cap| cap.parse::<Cap>().ok())
                .collect(),
            calls_per_second,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn body() -> TokenFileBody {
        TokenFileBody {
            url: "http://127.0.0.1:1/mcp".to_owned(),
            token: TOKEN.to_owned(),
            pid: Some(7),
            capabilities: CapSet::of([Cap::Inspect, Cap::Input]),
            calls_per_second: Some(20),
        }
    }

    #[test]
    fn a_token_file_reads_back_as_it_was_written() {
        let text = body().to_text();
        assert!(text.ends_with('\n'));
        assert_eq!(TokenFileBody::parse(&text).unwrap(), body());
    }

    #[test]
    fn what_a_token_file_does_not_state_is_absent() {
        let text = json!({ "url": "unix:/s", "token": TOKEN }).to_string();
        let read = TokenFileBody::parse(&text).unwrap();
        assert_eq!(read.pid, None);
        assert_eq!(read.calls_per_second, None);
        assert_eq!(read.capabilities, CapSet::EMPTY);
    }

    #[test]
    fn a_capability_of_a_later_inspector_is_left_out() {
        let text = json!({ "url": "unix:/s", "token": TOKEN, "capabilities": ["input", "new"] })
            .to_string();
        assert_eq!(
            TokenFileBody::parse(&text).unwrap().capabilities,
            CapSet::of([Cap::Input])
        );
    }

    #[test]
    fn what_no_inspector_writes_is_not_a_token_file() {
        assert!(matches!(
            TokenFileBody::parse("{"),
            Err(TokenFormatError::NotJson(_))
        ));
        assert!(matches!(
            TokenFileBody::parse(&json!({ "token": TOKEN }).to_string()),
            Err(TokenFormatError::Missing("url"))
        ));
        assert!(matches!(
            TokenFileBody::parse(&json!({ "url": "u", "token": "t" }).to_string()),
            Err(TokenFormatError::NotAToken)
        ));
        assert!(matches!(
            TokenFileBody::parse(
                &json!({ "url": "u", "token": TOKEN, "calls_per_second": 0 }).to_string()
            ),
            Err(TokenFormatError::BadRate)
        ));
    }

    #[test]
    fn a_refusal_gives_its_code_message_and_details() {
        let refusal = Refusal::of(&json!({
            "isError": true,
            "content": [{ "type": "text", "text": "no" }],
            "structuredContent": { "error": { "code": "stale_ref", "message": "gone", "details": 1 } },
        }))
        .unwrap();
        assert_eq!(refusal.code.as_deref(), Some("stale_ref"));
        assert_eq!(refusal.message, "gone");
        assert_eq!(refusal.details, Some(json!(1)));
        assert_eq!(refusal.result.text.as_deref(), Some("no"));
    }

    #[test]
    fn an_image_that_is_not_base64_is_told() {
        let bad = ToolResult::of(&json!({
            "content": [{ "type": "image", "mimeType": "image/png", "data": "*" }],
        }))
        .unwrap_err();
        assert_eq!(bad.mime, "image/png");
    }

    #[test]
    fn requests_are_the_protocols() {
        assert_eq!(
            request(3, "tools/list", &tool_list_params(Some("c"))),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": { "cursor": "c" } })
        );
        assert_eq!(
            tool_call_params("app_click", &json!({ "at": [1, 2] })),
            json!({ "name": "app_click", "arguments": { "at": [1, 2] } })
        );
        assert_eq!(
            initialize_params("n", "1")["protocolVersion"],
            json!(PROTOCOL)
        );
        assert!(notification("x").get("id").is_none());
    }
}
