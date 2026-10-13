//! Remote resources: what a `FontLoader` (and later an `Image`) with an
//! `http:` source reads. Ljós has no network stack of its own; a host that
//! has one installs a [`Fetcher`]. Without one every fetch fails, after the
//! fact, as a fetch that could not connect does.

use thiserror::Error;

/// Why a fetch has no bytes.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum FetchError {
    #[error("this application cannot fetch {0}: it has no network access")]
    NoNetwork(String),
    #[error("{0}")]
    Failed(String),
}

/// Reads a URL. Called on a thread of its own, so it may block.
pub trait Fetcher: Send + Sync {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError>;
}

/// The fetcher of an application without network access.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoNetwork;

impl Fetcher for NoNetwork {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        Err(FetchError::NoNetwork(url.to_owned()))
    }
}

/// Whether a URL names something to fetch rather than a local file.
pub fn is_remote(url: &str) -> bool {
    let scheme = url.split_once(':').map(|(s, _)| s.to_ascii_lowercase());
    matches!(scheme.as_deref(), Some("http" | "https" | "ftp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_network_schemes_are_remote() {
        assert!(is_remote("http://127.0.0.1:9/x.ttf"));
        assert!(is_remote("HTTPS://example.org/a"));
        assert!(!is_remote("file:///a.ttf"));
        assert!(!is_remote("a.ttf"));
        assert!(!is_remote("qrc:/a.ttf"));
    }
}
