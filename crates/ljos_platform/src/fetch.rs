//! Remote resources: what a `FontLoader` (and later an `Image`) with an
//! `http:` source reads. Ljós has no network stack of its own; a host that
//! has one installs a [`Fetcher`]. Without one every fetch fails, after the
//! fact, as a fetch that could not connect does.

use std::fmt;

use thiserror::Error;

/// Why a fetch has no bytes.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum FetchError {
    #[error("this application cannot fetch {0}: it has no network access")]
    NoNetwork(String),
    #[error("{0}")]
    Failed(String),
}

/// The method of a [`Request`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Method {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        })
    }
}

/// A full request: method, URL, headers and body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn get(url: impl Into<String>) -> Self {
        Self::new(Method::Get, url)
    }

    pub fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn body(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.body = bytes.into();
        self
    }

    /// Sets the body to `text` and, unless a content-type is already
    /// present (case-insensitive), says it is JSON.
    pub fn json(mut self, text: &str) -> Self {
        self.body = text.as_bytes().to_vec();
        // A caller that already named a content type meant it.
        if !self
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        {
            self.headers
                .push(("content-type".to_owned(), "application/json".to_owned()));
        }
        self
    }
}

/// A full response: status, headers and body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The first value of `name`, matched case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Reads a URL. Called on a thread of its own, so it may block.
pub trait Fetcher: Send + Sync {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError>;

    /// Sends a full [`Request`]. Called on its own thread, so it may
    /// block. An HTTP status is a [`Response`], not an `Err`: `Err` is
    /// for transport failures (no connection, a timeout, a body that
    /// could not be read) and for a request this fetcher cannot send.
    ///
    /// The default sends only `GET`, through [`Fetcher::fetch`], and
    /// refuses every other method by name.
    fn request(&self, request: &Request) -> Result<Response, FetchError> {
        match request.method {
            Method::Get => self.fetch(&request.url).map(|body| Response {
                status: 200,
                headers: Vec::new(),
                body,
            }),
            method => Err(FetchError::Failed(format!(
                "this application cannot send {method} requests"
            ))),
        }
    }
}

/// The fetcher of an application without network access.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoNetwork;

impl Fetcher for NoNetwork {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        Err(FetchError::NoNetwork(url.to_owned()))
    }

    fn request(&self, request: &Request) -> Result<Response, FetchError> {
        // The default would call a POST unsendable; here the network
        // itself is what is missing, whatever the method.
        Err(FetchError::NoNetwork(request.url.clone()))
    }
}

/// Whether a URL names something to fetch rather than a local file.
pub fn is_remote(url: &str) -> bool {
    let scheme = url.split_once(':').map(|(s, _)| s.to_ascii_lowercase());
    matches!(scheme.as_deref(), Some("http" | "https" | "ftp"))
}

/// The reason phrase HTTP gives a status, for saying why a reply was
/// refused when the reply's own phrase is not at hand: the error a QML
/// type logs names it after the status, as Qt's does.
pub fn reason_phrase(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        407 => "Proxy Authentication Required",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        411 => "Length Required",
        412 => "Precondition Failed",
        413 => "Content Too Large",
        414 => "URI Too Long",
        415 => "Unsupported Media Type",
        416 => "Range Not Satisfiable",
        417 => "Expectation Failed",
        418 => "I'm a teapot",
        421 => "Misdirected Request",
        422 => "Unprocessable Content",
        425 => "Too Early",
        426 => "Upgrade Required",
        428 => "Precondition Required",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        451 => "Unavailable For Legal Reasons",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        505 => "HTTP Version Not Supported",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_has_its_reason_phrase() {
        assert_eq!(reason_phrase(404), "Not Found");
        assert_eq!(reason_phrase(500), "Internal Server Error");
        assert_eq!(reason_phrase(799), "Unknown");
    }

    #[test]
    fn only_network_schemes_are_remote() {
        assert!(is_remote("http://127.0.0.1:9/x.ttf"));
        assert!(is_remote("HTTPS://example.org/a"));
        assert!(!is_remote("file:///a.ttf"));
        assert!(!is_remote("a.ttf"));
        assert!(!is_remote("qrc:/a.ttf"));
    }

    #[test]
    fn method_names_and_defaults() {
        assert_eq!(Method::default(), Method::Get);
        assert_eq!(Method::Get.to_string(), "GET");
        assert_eq!(Method::Post.to_string(), "POST");
        assert_eq!(Method::Put.to_string(), "PUT");
        assert_eq!(Method::Patch.to_string(), "PATCH");
        assert_eq!(Method::Delete.to_string(), "DELETE");
    }

    #[test]
    fn request_builders() {
        let r = Request::get("http://x/");
        assert_eq!(r.method, Method::Get);
        assert_eq!(r.url, "http://x/");
        assert!(r.headers.is_empty());
        assert!(r.body.is_empty());

        let r = Request::new(Method::Put, "http://x/a")
            .header("X-A", "1")
            .body(vec![1, 2, 3]);
        assert_eq!(r.method, Method::Put);
        assert_eq!(r.headers, vec![("X-A".to_owned(), "1".to_owned())]);
        assert_eq!(r.body, vec![1, 2, 3]);
    }

    #[test]
    fn json_sets_content_type_once() {
        let r = Request::new(Method::Post, "http://x/").json("{}");
        assert_eq!(r.body, b"{}");
        assert_eq!(
            r.headers,
            vec![("content-type".to_owned(), "application/json".to_owned())]
        );

        // A second json() replaces the body but not the header.
        let r = r.json("[]");
        assert_eq!(r.body, b"[]");
        assert_eq!(r.headers.len(), 1);

        // An existing content type, in any case, is kept.
        let r = Request::new(Method::Post, "http://x/")
            .header("Content-Type", "application/vnd.x+json")
            .json("{}");
        assert_eq!(
            r.headers,
            vec![(
                "Content-Type".to_owned(),
                "application/vnd.x+json".to_owned()
            )]
        );
    }

    #[test]
    fn response_helpers() {
        let r = Response {
            status: 201,
            headers: vec![
                ("Content-Type".to_owned(), "text/plain".to_owned()),
                ("X-Dup".to_owned(), "first".to_owned()),
                ("x-dup".to_owned(), "second".to_owned()),
            ],
            body: b"hi \xff".to_vec(),
        };
        assert!(r.ok());
        assert_eq!(r.header("content-type"), Some("text/plain"));
        assert_eq!(r.header("X-DUP"), Some("first"));
        assert_eq!(r.header("missing"), None);
        assert_eq!(r.text(), "hi \u{fffd}");

        for (status, ok) in [
            (199, false),
            (200, true),
            (299, true),
            (300, false),
            (404, false),
        ] {
            let r = Response {
                status,
                headers: Vec::new(),
                body: Vec::new(),
            };
            assert_eq!(r.ok(), ok, "{status}");
        }
    }

    struct Bytes;

    impl Fetcher for Bytes {
        fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
            Ok(url.as_bytes().to_vec())
        }
    }

    #[test]
    fn default_request_wraps_get_and_refuses_other_methods_by_name() {
        let got = Bytes.request(&Request::get("http://x/")).unwrap();
        assert_eq!(got.status, 200);
        assert!(got.headers.is_empty());
        assert_eq!(got.body, b"http://x/");

        for method in [Method::Post, Method::Put, Method::Patch, Method::Delete] {
            let err = Bytes
                .request(&Request::new(method, "http://x/"))
                .unwrap_err();
            assert_eq!(
                err,
                FetchError::Failed(format!("this application cannot send {method} requests"))
            );
        }
    }

    #[test]
    fn no_network_refuses_every_method_as_no_network() {
        for method in [
            Method::Get,
            Method::Post,
            Method::Put,
            Method::Patch,
            Method::Delete,
        ] {
            let err = NoNetwork
                .request(&Request::new(method, "http://x/"))
                .unwrap_err();
            assert_eq!(err, FetchError::NoNetwork("http://x/".to_owned()));
        }
    }
}
