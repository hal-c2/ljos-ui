//! The desktop's platform services (ARCHITECTURE.md §15): the system
//! clipboard and dialogs, an HTTPS fetcher, and the window system's
//! name for `Qt.platform`.
//!
//! Files dragged in from other applications are not handled here: winit's
//! file events reach each window's egui input, and the window's `QmlView`
//! hands them to its scene's DropAreas (`ljos_egui`). Only the view knows
//! where its scene is and which tree it shows, and one path delivers a drop
//! once.

use std::io::Read as _;
use std::sync::Arc;
use std::time::Duration;

use ljos_engine::Engine;
use ljos_platform::{FetchError, Fetcher, Platform};

/// Gives a desktop application the system's services. Only the shell's
/// own loop does this: an application shown in another host (eframe, a
/// test) keeps the in-process ones.
pub(crate) fn install(engine: &Engine, wake: impl Fn() + Send + Sync + 'static) {
    let platform = Platform::of(engine);
    // An answer from a dialog's thread wakes the loop; nothing waits for
    // one by looking.
    platform.set_waker(wake);
    if let Some(clipboard) = ljos_platform::native::SystemClipboard::connect() {
        platform.set_clipboard(std::rc::Rc::new(clipboard));
    }
    platform.set_dialogs(std::rc::Rc::new(
        ljos_platform::native::SystemDialogs::default(),
    ));
    platform.set_fetcher(Arc::new(HttpFetcher::new()));
    platform.set_plugin_name(plugin_name());
}

/// How long one fetch — DNS, connect, redirects and the body — may
/// take before it is given up as failed.
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
/// The largest reply a fetch hands back. A larger one is refused
/// whole rather than handed over cut short: the caller asked for a
/// resource (a font, a picture, a feed), and half of one is no use.
const FETCH_MAX_BYTES: u64 = 16 * 1024 * 1024;
/// Redirects followed before giving up; the reply that runs out is
/// an error, not the redirect itself.
const FETCH_MAX_REDIRECTS: u32 = 10;

/// The desktop's fetcher: blocking HTTP and HTTPS (rustls, with the
/// webpki roots) through `ureq`. `Platform::fetch` calls it on a
/// thread of its own and delivers the answer on the Engine thread,
/// so blocking here blocks nothing that draws, and no async runtime
/// rides along.
pub(crate) struct HttpFetcher {
    agent: ureq::Agent,
    max_bytes: u64,
}

impl HttpFetcher {
    fn new() -> Self {
        Self::with_limits(FETCH_TIMEOUT, FETCH_MAX_BYTES, FETCH_MAX_REDIRECTS)
    }

    fn with_limits(timeout: Duration, max_bytes: u64, max_redirects: u32) -> Self {
        // The status is this fetcher's to judge, so it can be named
        // in the error; ureq's own status error would only repeat it.
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .max_redirects(max_redirects)
            .max_redirects_will_error(true)
            .http_status_as_error(false)
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
            max_bytes,
        }
    }
}

impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        let failed = |why: String| FetchError::Failed(format!("fetching {url} failed: {why}"));
        let scheme = url.split_once(':').map(|(s, _)| s.to_ascii_lowercase());
        if !matches!(scheme.as_deref(), Some("http" | "https")) {
            return Err(FetchError::Failed(format!(
                "cannot fetch {url}: only http and https URLs can be fetched"
            )));
        }
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| failed(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(failed(format!("HTTP {status}")));
        }
        // One byte past the cap is read, to tell a reply of exactly
        // the cap from a larger one cut at it.
        let mut bytes = Vec::new();
        response
            .into_body()
            .into_reader()
            .take(self.max_bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| failed(e.to_string()))?;
        if bytes.len() as u64 > self.max_bytes {
            return Err(failed(format!(
                "the reply is larger than {} bytes",
                self.max_bytes
            )));
        }
        Ok(bytes)
    }
}

/// What Qt calls the window system: the name of its platform plugin.
fn plugin_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "cocoa"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some_and(|d| !d.is_empty()) {
        // winit prefers Wayland when the session has one, as Qt does.
        "wayland"
    } else {
        "xcb"
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::Write as _;
    use std::net::{TcpListener, TcpStream};
    use std::rc::Rc;

    use super::*;

    /// Serves each of `responses` once, in order, on a loopback port
    /// of the system's choosing, and gives the URL of the first.
    /// Nothing here leaves the machine.
    fn serve(responses: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/first", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for response in responses {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                read_request(&mut stream);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        url
    }

    /// Reads one request's head, so the reply is not written into a
    /// socket the client has not finished with.
    fn read_request(stream: &mut TcpStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut seen = Vec::new();
        let mut byte = [0u8; 1];
        while !seen.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(0) | Err(_) => return,
                Ok(_) => seen.push(byte[0]),
            }
        }
    }

    fn ok(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn a_fetch_hands_back_the_reply() {
        let url = serve(vec![ok("hello from loopback")]);
        assert_eq!(
            HttpFetcher::new().fetch(&url).unwrap(),
            b"hello from loopback"
        );
    }

    #[test]
    fn a_fetch_follows_a_redirect() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}/first");
        std::thread::spawn(move || {
            for response in [
                format!(
                    "HTTP/1.1 302 Found\r\nContent-Length: 0\r\nLocation: http://{addr}/next\r\nConnection: close\r\n\r\n"
                ),
                ok("there"),
            ] {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                read_request(&mut stream);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        assert_eq!(HttpFetcher::new().fetch(&url).unwrap(), b"there");
    }

    #[test]
    fn a_status_that_is_no_success_says_so() {
        let url = serve(vec![
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        ]);
        let err = HttpFetcher::new().fetch(&url).err().unwrap().to_string();
        assert!(err.contains("HTTP 404"), "{err}");
        assert!(err.contains(&url), "{err}");
    }

    #[test]
    fn a_reply_larger_than_the_cap_is_refused_whole() {
        let url = serve(vec![ok("12345678")]);
        let fetcher = HttpFetcher::with_limits(Duration::from_secs(5), 4, 10);
        let err = fetcher.fetch(&url).err().unwrap().to_string();
        assert!(err.contains("larger than 4 bytes"), "{err}");
    }

    #[test]
    fn only_http_and_https_can_be_fetched() {
        let err = HttpFetcher::new()
            .fetch("ftp://127.0.0.1/x")
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("only http and https"), "{err}");
    }

    #[test]
    fn a_server_that_never_answers_times_out() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/slow", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                read_request(&mut stream);
                // Far past the fetcher's limit below; the socket is
                // gone by the time this write happens.
                std::thread::sleep(Duration::from_secs(2));
                let _ = stream.write_all(ok("too late").as_bytes());
            }
        });
        let fetcher = HttpFetcher::with_limits(Duration::from_millis(200), 1024, 10);
        let err = fetcher.fetch(&url).err().unwrap().to_string();
        assert!(err.contains("failed"), "{err}");
        assert!(err.contains(&url), "{err}");
    }

    /// The path an application takes: `Platform::fetch` runs this
    /// fetcher on its own thread and the answer runs on the Engine
    /// thread (`finish` stands in for the frame that delivers it).
    #[test]
    fn the_platform_delivers_a_fetch_on_the_engine_thread() {
        let url = serve(vec![ok("delivered")]);
        let engine = Engine::new();
        let platform = Platform::of(&engine);
        platform.set_fetcher(Arc::new(HttpFetcher::new()));
        type Delivered = Rc<RefCell<Option<Result<Vec<u8>, FetchError>>>>;
        let got: Delivered = Rc::new(RefCell::new(None));
        let put = Rc::clone(&got);
        platform.fetch(&url, move |_, result| *put.borrow_mut() = Some(result));
        platform.finish(&engine);
        assert_eq!(
            got.borrow().as_ref().unwrap().as_ref().unwrap(),
            b"delivered"
        );
    }
}
