//! The fetcher of a host with a network: blocking HTTP and HTTPS through
//! `ureq` (rustls, with the webpki roots). A host turns on the `http`
//! feature and installs it with [`Platform::set_fetcher`](crate::Platform::set_fetcher).

use std::io::Read as _;
use std::time::Duration;

use crate::fetch::{FetchError, Fetcher, Incoming, Request, Response};

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

/// The fetcher of a host with a network: blocking HTTP and HTTPS
/// (rustls, with the webpki roots) through `ureq`. `Platform::fetch` calls it on a
/// thread of its own and delivers the answer on the Engine thread,
/// so blocking here blocks nothing that draws, and no async runtime
/// rides along.
pub struct HttpFetcher {
    agent: ureq::Agent,
    timeout: Duration,
    max_bytes: u64,
}

impl std::fmt::Debug for HttpFetcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpFetcher")
            .field("max_bytes", &self.max_bytes)
            .finish_non_exhaustive()
    }
}

impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpFetcher {
    pub fn new() -> Self {
        Self::with_limits(FETCH_TIMEOUT, FETCH_MAX_BYTES, FETCH_MAX_REDIRECTS)
    }

    pub fn with_limits(timeout: Duration, max_bytes: u64, max_redirects: u32) -> Self {
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
            timeout,
            max_bytes,
        }
    }
}

impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        let display_url = ljos_engine::diagnostic_url(url);
        let failed =
            |why: String| FetchError::Failed(format!("fetching {display_url} failed: {why}"));
        let scheme = url.split_once(':').map(|(s, _)| s.to_ascii_lowercase());
        if !matches!(scheme.as_deref(), Some("http" | "https")) {
            return Err(FetchError::Failed(format!(
                "cannot fetch {display_url}: only http and https URLs can be fetched"
            )));
        }
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| transport(url, e, failed))?;
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

    fn stream(&self, url: &str) -> Result<Incoming, FetchError> {
        let display_url = ljos_engine::diagnostic_url(url);
        let failed =
            |why: String| FetchError::Failed(format!("fetching {display_url} failed: {why}"));
        let scheme = url.split_once(':').map(|(s, _)| s.to_ascii_lowercase());
        if !matches!(scheme.as_deref(), Some("http" | "https")) {
            return Err(FetchError::Failed(format!(
                "cannot fetch {display_url}: only http and https URLs can be fetched"
            )));
        }
        // A stream may never end, so only reaching the server and its
        // first answer are timed; the body takes as long as it takes.
        let response = self
            .agent
            .get(url)
            .config()
            .timeout_global(None)
            .timeout_connect(Some(self.timeout))
            .timeout_recv_response(Some(self.timeout))
            .build()
            .call()
            .map_err(|e| transport(url, e, failed))?;
        let status = response.status();
        if !status.is_success() {
            return Err(failed(format!("HTTP {status}")));
        }
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(|v| v.trim().to_ascii_lowercase());
        Ok(Incoming {
            content_type,
            body: Box::new(response.into_body().into_reader()),
        })
    }

    fn request(&self, request: &Request) -> Result<Response, FetchError> {
        let url = &request.url;
        let method = request.method;
        let display_url = ljos_engine::diagnostic_url(url);
        let failed =
            |why: String| FetchError::Failed(format!("{method} {display_url} failed: {why}"));
        let scheme = url.split_once(':').map(|(s, _)| s.to_ascii_lowercase());
        if !matches!(scheme.as_deref(), Some("http" | "https")) {
            return Err(FetchError::Failed(format!(
                "cannot fetch {display_url}: only http and https URLs can be fetched"
            )));
        }
        if request.body.len() as u64 > self.max_bytes {
            return Err(failed(format!(
                "the request body is larger than {} bytes",
                self.max_bytes
            )));
        }
        // The agent already runs with http_status_as_error(false), so a
        // 4xx/5xx arrives here as a response: status is the caller's
        // data, and only transport failures are errors.
        let mut builder = ureq::http::Request::builder()
            .method(method.to_string().as_str())
            .uri(url.as_str());
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        // An empty body stays no body at all, so a plain GET or DELETE
        // does not grow a Content-Length it never had.
        let response = if request.body.is_empty() {
            let built = builder.body(()).map_err(|e| failed(e.to_string()))?;
            self.agent.run(built)
        } else {
            let built = builder
                .body(request.body.as_slice())
                .map_err(|e| failed(e.to_string()))?;
            self.agent.run(built)
        }
        .map_err(|e| transport(url, e, failed))?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_owned(),
                    String::from_utf8_lossy(value.as_bytes()).into_owned(),
                )
            })
            .collect();
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
        Ok(Response {
            status,
            headers,
            body: bytes,
        })
    }
}

/// Why no reply came, in the words Qt's network access has for the
/// failures an application meets most (the server is not listening, the
/// host has no address), so that what QML logs reads as it does on Qt.
/// Anything else is said by `failed`.
fn transport(url: &str, err: ureq::Error, failed: impl Fn(String) -> FetchError) -> FetchError {
    match &err {
        ureq::Error::BadUri(_) => failed("invalid request URL".to_owned()),
        ureq::Error::RequireHttpsOnly(_) => failed("HTTPS is required".to_owned()),
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::ConnectionRefused => {
            FetchError::Failed("Connection refused".to_owned())
        }
        ureq::Error::HostNotFound => {
            let display_url = ljos_engine::diagnostic_url(url);
            let parsed = display_url.parse::<ureq::http::Uri>().ok();
            let host = parsed
                .as_ref()
                .and_then(|url| url.host())
                .unwrap_or("[invalid host]");
            FetchError::Failed(format!("Host {host} not found"))
        }
        ureq::Error::Rustls(tls) => handshake(tls),
        ureq::Error::Io(io) => match io.get_ref().and_then(|e| e.downcast_ref::<rustls::Error>()) {
            Some(tls) => handshake(tls),
            None => failed(err.to_string()),
        },
        _ => failed(err.to_string()),
    }
}

/// A TLS failure in Qt 6.11's words for the certificate faults it names
/// alike; any other is said in rustls's.
fn handshake(err: &rustls::Error) -> FetchError {
    use rustls::CertificateError as C;
    let why = match err {
        rustls::Error::InvalidCertificate(cert) => match cert {
            C::Expired | C::ExpiredContext { .. } => "The certificate has expired".to_owned(),
            C::NotValidForName | C::NotValidForNameContext { .. } => {
                "The host name did not match any of the valid hosts for this certificate".to_owned()
            }
            C::UnknownIssuer => {
                "The issuer certificate of a locally looked up certificate could not be found"
                    .to_owned()
            }
            other => other.to_string(),
        },
        other => other.to_string(),
    };
    FetchError::Failed(format!("SSL handshake failed: {why}"))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::Write as _;
    use std::net::{TcpListener, TcpStream};
    use std::rc::Rc;
    use std::sync::Arc;

    use ljos_engine::Engine;

    use super::*;
    use crate::Platform;

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

    /// Reads one whole request, head and body, as text.
    fn read_whole_request(stream: &mut TcpStream) -> String {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut seen = Vec::new();
        let mut byte = [0u8; 1];
        while !seen.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(0) | Err(_) => return String::from_utf8_lossy(&seen).into_owned(),
                Ok(_) => seen.push(byte[0]),
            }
        }
        let head = String::from_utf8_lossy(&seen).into_owned();
        let length = head
            .lines()
            .find_map(|l| {
                let (name, value) = l.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())?
            })
            .unwrap_or(0);
        let mut body = vec![0u8; length];
        let mut read = 0;
        while read < length {
            match stream.read(&mut body[read..]) {
                Ok(0) | Err(_) => break,
                Ok(n) => read += n,
            }
        }
        format!("{head}{}", String::from_utf8_lossy(&body[..read]))
    }

    /// Like [`serve`], and keeps every request it was sent.
    fn serve_capturing(
        responses: Vec<String>,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/first", listener.local_addr().unwrap());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let keep = std::sync::Arc::clone(&seen);
        std::thread::spawn(move || {
            for response in responses {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let request = read_whole_request(&mut stream);
                keep.lock().unwrap().push(request);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (url, seen)
    }

    fn ok(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn diagnostic_failures_never_expose_url_credentials() {
        for (response, cap, expected) in [
            (
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_owned(),
                1024,
                "HTTP 404",
            ),
            (ok("too large"), 2, "larger than 2 bytes"),
            ("not HTTP\r\n\r\n".to_owned(), 1024, "failed"),
        ] {
            let plain = serve(vec![response]);
            let url = plain.replace("http://", "http://private-user:private-password@")
                + "?token=private-token#private-fragment";
            let err = HttpFetcher::with_limits(Duration::from_secs(5), cap, 0)
                .fetch(&url)
                .unwrap_err()
                .to_string();
            assert!(err.contains(expected), "{err}");
            assert!(err.contains(&plain), "{err}");
            for secret in [
                "private-user",
                "private-password",
                "private-token",
                "private-fragment",
            ] {
                assert!(!err.contains(secret), "{err}");
            }
        }
        let url =
            "https://private-user:private-password@example.com/file?private-token#private-fragment";
        let (plain, requests) = serve_capturing(vec![ok("ok")]);
        let requested = plain + "?token=private-token";
        assert_eq!(HttpFetcher::new().fetch(&requested).unwrap(), b"ok");
        assert!(requests.lock().unwrap()[0].contains("/first?token=private-token"));
        let request = Request::get(url).body(vec![0; 3]);
        let err = HttpFetcher::with_limits(Duration::from_secs(5), 2, 0)
            .request(&request)
            .unwrap_err()
            .to_string();
        assert!(err.contains("https://example.com/file"), "{err}");
        assert!(!err.contains("private-"), "{err}");
        assert_eq!(request.url, url);
    }

    #[test]
    fn diagnostic_sanitizing_does_not_change_the_request_target() {
        let (plain, seen) = serve_capturing(vec![ok("hello")]);
        let url = plain.replace("http://", "http://private-user:private-password@")
            + "?token=private-token#private-fragment";
        HttpFetcher::with_limits(Duration::from_secs(5), 1024, 0)
            .fetch(&url)
            .unwrap();
        assert!(seen.lock().unwrap()[0].starts_with("GET /first?token=private-token HTTP/1.1"));
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
    fn a_stream_is_read_as_it_comes_whatever_its_size() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/live", listener.local_addr().unwrap());
        let (sent, told) = std::sync::mpsc::channel();
        let (more, go_on) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            let head = "HTTP/1.1 200 OK\r\nContent-Type: Multipart/X-Mixed-Replace; boundary=b\r\nConnection: close\r\n\r\n";
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(b"first").unwrap();
            sent.send(()).unwrap();
            // The rest only once the first part was read: what came
            // was handed over without waiting for the end.
            go_on.recv().unwrap();
            stream.write_all(&[7; 64]).unwrap();
        });
        // A cap of 8 bytes, which a stream is not held to.
        let fetcher = HttpFetcher::with_limits(Duration::from_secs(5), 8, 0);
        let mut incoming = fetcher.stream(&url).unwrap();
        assert_eq!(
            incoming.content_type.as_deref(),
            Some("multipart/x-mixed-replace")
        );
        told.recv().unwrap();
        let mut first = [0; 5];
        incoming.body.read_exact(&mut first).unwrap();
        assert_eq!(&first, b"first");
        more.send(()).unwrap();
        let mut rest = Vec::new();
        incoming.body.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, [7; 64]);
    }

    #[test]
    fn a_stream_that_is_refused_says_why() {
        let url = serve(vec![
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        ]);
        let err = HttpFetcher::new().stream(&url).unwrap_err();
        assert!(err.to_string().contains("HTTP 404"), "{err}");
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

    #[test]
    fn a_certificate_fault_is_said_in_qts_words() {
        let expired = rustls::Error::InvalidCertificate(rustls::CertificateError::Expired);
        let wrapped = ureq::Error::Io(std::io::Error::other(expired));
        assert_eq!(
            transport("https://a.test/", wrapped, FetchError::Failed).to_string(),
            "SSL handshake failed: The certificate has expired"
        );
        let unknown = rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer);
        assert_eq!(
            transport(
                "https://a.test/",
                ureq::Error::Rustls(unknown),
                FetchError::Failed
            )
            .to_string(),
            "SSL handshake failed: The issuer certificate of a locally looked up certificate \
             could not be found"
        );
    }

    #[test]
    fn a_server_that_is_not_listening_refuses_in_qts_words() {
        // Bound and dropped: nothing listens on the port any more.
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let url = format!("http://127.0.0.1:{port}/x.png");
        let refused = FetchError::Failed("Connection refused".to_owned());
        assert_eq!(HttpFetcher::new().fetch(&url).unwrap_err(), refused);
        assert_eq!(
            HttpFetcher::new().request(&Request::get(&url)).unwrap_err(),
            refused
        );
    }

    #[test]
    fn a_post_sends_its_body_and_headers_and_the_created_status_comes_back() {
        use crate::Method;

        let (url, seen) = serve_capturing(vec![
            "HTTP/1.1 201 Created\r\nContent-Length: 7\r\nX-Made: yes\r\nConnection: close\r\n\r\ncreated"
                .to_owned(),
        ]);
        let response = HttpFetcher::new()
            .request(&Request::new(Method::Post, url).json("{\"a\":1}"))
            .unwrap();
        assert_eq!(response.status, 201);
        assert!(response.ok());
        assert_eq!(response.text(), "created");
        assert_eq!(response.header("x-made"), Some("yes"));

        let sent = seen.lock().unwrap().clone();
        assert_eq!(sent.len(), 1);
        let sent = &sent[0];
        assert!(sent.starts_with("POST /first"), "{sent}");
        let lower = sent.to_ascii_lowercase();
        assert!(lower.contains("content-type: application/json"), "{sent}");
        let body = sent.split("\r\n\r\n").nth(1).unwrap_or("");
        assert_eq!(body, "{\"a\":1}");
    }

    #[test]
    fn a_status_via_request_is_a_response_not_an_error() {
        let url = serve(vec![
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        ]);
        let response = HttpFetcher::new().request(&Request::get(&url)).unwrap();
        assert_eq!(response.status, 404);
        assert!(!response.ok());
    }

    #[test]
    fn put_and_delete_travel() {
        use crate::Method;

        let (url, seen) = serve_capturing(vec![ok("put"), ok("gone")]);
        let fetcher = HttpFetcher::new();
        let put = fetcher
            .request(&Request::new(Method::Put, &url).body(b"payload".to_vec()))
            .unwrap();
        assert_eq!(put.text(), "put");
        let deleted = fetcher
            .request(&Request::new(Method::Delete, &url))
            .unwrap();
        assert_eq!(deleted.text(), "gone");

        let sent = seen.lock().unwrap().clone();
        assert_eq!(sent.len(), 2);
        assert!(sent[0].starts_with("PUT /first"), "{}", sent[0]);
        assert_eq!(sent[0].split("\r\n\r\n").nth(1).unwrap_or(""), "payload");
        assert!(sent[1].starts_with("DELETE /first"), "{}", sent[1]);
    }

    #[test]
    fn a_request_body_larger_than_the_cap_is_refused_before_it_travels() {
        use crate::Method;

        let fetcher = HttpFetcher::with_limits(Duration::from_secs(5), 4, 10);
        let err = fetcher
            .request(&Request::new(Method::Post, "http://127.0.0.1:9/x").body(b"12345".to_vec()))
            .unwrap_err();
        assert!(
            matches!(&err, FetchError::Failed(why) if why.contains("request body is larger than 4 bytes")),
            "{err}"
        );
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
