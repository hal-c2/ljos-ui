//! Who may talk to a server that listens on a socket (ARCHITECTURE.md
//! §19.7): a token made for this start of the process, kept in a file only
//! its owner can read, and the checks that turn a browser away.

use std::collections::VecDeque;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::CapSet;

/// A 256-bit bearer token.
#[derive(Clone, PartialEq, Eq)]
pub struct Token {
    hex: String,
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A token in a log is a token given away.
        f.write_str("Token(<redacted>)")
    }
}

impl Token {
    /// A new token from the operating system's random source.
    pub fn generate() -> Result<Token, TokenFileError> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| TokenFileError::Random(e.to_string()))?;
        let mut hex = String::with_capacity(64);
        for b in bytes {
            hex.push(char::from_digit(u32::from(b >> 4), 16).unwrap_or('0'));
            hex.push(char::from_digit(u32::from(b & 15), 16).unwrap_or('0'));
        }
        Ok(Token { hex })
    }

    /// The token as a client sends it: 64 hexadecimal digits.
    pub fn as_str(&self) -> &str {
        &self.hex
    }

    /// Whether `presented` is this token. The time this takes does not
    /// depend on how much of it is right.
    pub fn matches(&self, presented: &str) -> bool {
        constant_time_eq::constant_time_eq(self.hex.as_bytes(), presented.as_bytes())
    }

    /// Whether an `Authorization` header carries this token.
    pub fn authorizes(&self, header: Option<&str>) -> bool {
        let Some(header) = header else {
            return false;
        };
        let Some((scheme, value)) = header.trim().split_once(' ') else {
            return false;
        };
        scheme.eq_ignore_ascii_case("bearer") && self.matches(value.trim())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TokenFileError {
    #[error("the operating system gave no random bytes: {0}")]
    Random(String),
    #[error("cannot write the token file {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// The file a client reads the address and the token from. It is removed
/// when this is dropped.
#[derive(Debug)]
pub struct TokenFile {
    path: PathBuf,
}

impl TokenFile {
    /// Writes `{"url", "token", "pid", "capabilities"}` to `path`, readable
    /// and writable by the owner alone.
    pub fn write(
        path: &Path,
        url: &str,
        token: &Token,
        caps: CapSet,
    ) -> Result<TokenFile, TokenFileError> {
        let fail = |source| TokenFileError::Write {
            path: path.to_path_buf(),
            source,
        };
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir).map_err(fail)?;
        }
        // A process that died may have left a file with this name. It is
        // removed, not written over: what is there may be a link to a file
        // somebody wants overwritten, and `create_new` follows none.
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(fail(e)),
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            // Given at creation, so the file is never open to others, not
            // even for the moment before a `chmod`.
            options.mode(0o600);
        }
        let mut file = options.open(path).map_err(fail)?;
        let body = serde_json::json!({
            "url": url,
            "token": token.as_str(),
            "pid": std::process::id(),
            "capabilities": caps,
        });
        let mut text = serde_json::to_string_pretty(&body).unwrap_or_default();
        text.push('\n');
        file.write_all(text.as_bytes()).map_err(fail)?;
        Ok(TokenFile {
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Opens a token file for a client to read. It never waits for a
    /// writer, so a pipe in its place is opened and can be turned away;
    /// with `follow` false, a link in its place is not opened at all.
    pub fn open_for_reading(path: &Path, follow: bool) -> std::io::Result<std::fs::File> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            let mut flags = libc::O_NONBLOCK;
            if !follow {
                flags |= libc::O_NOFOLLOW;
            }
            options.custom_flags(flags);
        }
        #[cfg(not(unix))]
        let _ = follow;
        options.open(path)
    }
}

impl Drop for TokenFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// `host` and `port` of `127.0.0.1:8080`, `localhost`, `[::1]:8080`.
fn authority(text: &str) -> Option<(&str, Option<u16>)> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        return match after.strip_prefix(':') {
            Some(port) => Some((host, Some(port.parse().ok()?))),
            None if after.is_empty() => Some((host, None)),
            None => None,
        };
    }
    match text.rsplit_once(':') {
        Some((host, port)) => Some((host, Some(port.parse().ok()?))),
        None => Some((text, None)),
    }
}

fn is_loopback_name(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

/// Whether a `Host` header names this machine (and `port`, when it names a
/// port and one is asked for). A page on another site that was made to
/// resolve to 127.0.0.1 still sends its own name here, and is turned away.
pub fn loopback_host(header: &str, port: Option<u16>) -> bool {
    match authority(header) {
        Some((host, given)) => {
            is_loopback_name(host)
                && match (given, port) {
                    (Some(g), Some(p)) => g == p,
                    _ => true,
                }
        }
        None => false,
    }
}

/// Whether a `Host` header is an address, not a name: what a server bound
/// beyond this machine accepts. A page that made its own name resolve here
/// (DNS rebinding) sends that name, and an address it cannot make its own.
pub fn address_host(header: &str, port: Option<u16>) -> bool {
    match authority(header) {
        Some((host, given)) => {
            (is_loopback_name(host) || host.parse::<std::net::IpAddr>().is_ok())
                && match (given, port) {
                    (Some(g), Some(p)) => g == p,
                    _ => true,
                }
        }
        None => false,
    }
}

/// Whether an `Origin` header is a page served from this machine. A client
/// that is not a browser sends no `Origin` at all.
pub fn loopback_origin(header: &str) -> bool {
    let header = header.trim();
    let Some(rest) = header
        .strip_prefix("http://")
        .or_else(|| header.strip_prefix("https://"))
    else {
        // `null` (a sandboxed page, a file) and every other scheme.
        return false;
    };
    if rest.contains(['/', '?', '#', '@']) {
        return false;
    }
    authority(rest).is_some_and(|(host, _)| is_loopback_name(host))
}

/// At most `max` calls in any `window`.
#[derive(Debug)]
pub struct RateLimit {
    max: usize,
    window: Duration,
    recent: VecDeque<Instant>,
}

impl RateLimit {
    pub fn new(max: usize, window: Duration) -> Self {
        RateLimit {
            max: max.max(1),
            window,
            recent: VecDeque::new(),
        }
    }

    /// 20 calls a second (§19.7).
    pub fn per_second(max: usize) -> Self {
        Self::new(max, Duration::from_secs(1))
    }

    /// Counts a call at `now` and says whether it may go ahead.
    pub fn allow(&mut self, now: Instant) -> bool {
        while let Some(first) = self.recent.front() {
            if now.duration_since(*first) < self.window {
                break;
            }
            self.recent.pop_front();
        }
        if self.recent.len() >= self.max {
            return false;
        }
        self.recent.push_back(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Cap;

    #[test]
    fn a_server_beyond_this_machine_answers_to_an_address_and_not_to_a_name() {
        assert!(address_host("192.168.1.20:7600", Some(7600)));
        assert!(address_host("[fe80::1]:7600", Some(7600)));
        assert!(address_host("localhost:7600", Some(7600)));
        // The port has to be the one listened on, when it is given.
        assert!(!address_host("192.168.1.20:7601", Some(7600)));
        // A page that made its own name resolve here sends that name.
        assert!(!address_host("rebind.example:7600", Some(7600)));
        assert!(!address_host("", Some(7600)));
    }

    #[test]
    fn tokens_are_long_differ_and_compare() {
        let a = Token::generate().unwrap();
        let b = Token::generate().unwrap();
        assert_eq!(a.as_str().len(), 64);
        assert!(a.as_str().bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert!(a.matches(a.as_str()));
        assert!(!a.matches(b.as_str()));
        assert!(!a.matches(""));
        assert!(!a.matches(&a.as_str()[..63]));
        assert_eq!(format!("{a:?}"), "Token(<redacted>)");
    }

    #[test]
    fn the_header_must_be_a_bearer_with_the_token() {
        let t = Token::generate().unwrap();
        assert!(t.authorizes(Some(&format!("Bearer {}", t.as_str()))));
        assert!(t.authorizes(Some(&format!("bearer  {} ", t.as_str()))));
        assert!(!t.authorizes(None));
        assert!(!t.authorizes(Some(t.as_str())));
        assert!(!t.authorizes(Some(&format!("Basic {}", t.as_str()))));
        assert!(!t.authorizes(Some("Bearer")));
        assert!(!t.authorizes(Some("Bearer wrong")));
    }

    #[test]
    fn hosts_of_this_machine_are_told_from_others() {
        for ok in [
            "127.0.0.1:4000",
            "localhost:4000",
            "[::1]:4000",
            "LOCALHOST",
            "127.0.0.1",
        ] {
            assert!(loopback_host(ok, Some(4000)), "{ok}");
        }
        for bad in [
            "example.com",
            "example.com:4000",
            "127.0.0.1:4001",
            "127.0.0.1.example.com:4000",
            "localhost.example.com",
            "127.0.0.2:4000",
            "[::1",
            "127.0.0.1:port",
            "",
        ] {
            assert!(!loopback_host(bad, Some(4000)), "{bad}");
        }
        assert!(loopback_host("localhost:9", None));
    }

    #[test]
    fn origins_of_this_machine_are_told_from_others() {
        for ok in [
            "http://localhost:6274",
            "http://127.0.0.1",
            "https://localhost",
            "http://[::1]:1",
        ] {
            assert!(loopback_origin(ok), "{ok}");
        }
        for bad in [
            "https://example.com",
            "null",
            "http://localhost.example.com",
            "http://example.com@localhost",
            "http://localhost/path",
            "file://localhost",
            "localhost",
            "",
        ] {
            assert!(!loopback_origin(bad), "{bad}");
        }
    }

    #[test]
    fn the_rate_is_held_and_given_back() {
        let mut r = RateLimit::per_second(3);
        let t0 = Instant::now();
        assert!(r.allow(t0));
        assert!(r.allow(t0 + Duration::from_millis(10)));
        assert!(r.allow(t0 + Duration::from_millis(20)));
        assert!(!r.allow(t0 + Duration::from_millis(30)));
        assert!(!r.allow(t0 + Duration::from_millis(999)));
        // A refused call does not count against the next second.
        assert!(r.allow(t0 + Duration::from_millis(1005)));
        assert!(r.allow(t0 + Duration::from_millis(1015)));
        assert!(!r.allow(t0 + Duration::from_millis(1016)));
    }

    #[cfg(unix)]
    #[test]
    fn the_token_file_is_its_owners_alone_and_goes_with_the_server() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("mcp-test.json");
        let token = Token::generate().unwrap();
        // Something is in the way: a file anyone may read.
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let file = TokenFile::write(
            &path,
            "http://127.0.0.1:4000/mcp",
            &token,
            CapSet::of([Cap::Inspect]),
        )
        .unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let body: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(body["url"], "http://127.0.0.1:4000/mcp");
        assert_eq!(body["token"], token.as_str());
        assert_eq!(body["pid"], std::process::id());
        assert_eq!(body["capabilities"], serde_json::json!(["inspect"]));
        drop(file);
        assert!(!path.exists());
    }
}
