//! A web server of the scenario's own, on the loopback interface: what an
//! application fetches over HTTP comes from here, never from the internet.
//! Both backends reach it (`qml6` is another process), so it listens on a
//! real port and answers on a thread of its own. It stops with the
//! scenario.
//!
//! The QML names it as `$SERVER`, which a scenario's files have replaced
//! by `http://127.0.0.1:<port>` before they are loaded.
//!
//! `/echo` answers any method with what it was sent, for a script that
//! sends requests of its own (XMLHttpRequest): the method, the
//! content type and the `x-` headers, one per line by name, a blank line, then
//! the body.

use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

/// What the QML writes for the server's address.
pub const PLACEHOLDER: &str = "$SERVER";

type Files = Arc<Mutex<HashMap<String, Body>>>;

#[derive(Clone, Debug)]
enum Body {
    Bytes(Vec<u8>),
    Repeated(usize),
}

#[derive(Debug)]
pub struct WebServer {
    port: u16,
    files: Files,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl WebServer {
    /// Starts listening on a port the system picks.
    ///
    /// # Errors
    /// No port could be had.
    pub fn start() -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("the scenario's web server cannot listen: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("the scenario's web server has no address: {e}"))?
            .port();
        let files = Files::default();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (files, stop) = (Arc::clone(&files), Arc::clone(&stop));
            std::thread::Builder::new()
                .name("ljos-bdd-web".to_owned())
                .spawn(move || {
                    for stream in listener.incoming() {
                        if stop.load(Ordering::Acquire) {
                            break;
                        }
                        // One at a time: a scenario's few requests are
                        // small, and an answer is written whole at once.
                        if let Ok(stream) = stream {
                            answer(stream, &files);
                        }
                    }
                })
                .map_err(|e| format!("the scenario's web server cannot start: {e}"))?
        };
        Ok(Self {
            port,
            files,
            stop,
            thread: Some(thread),
        })
    }

    /// `http://127.0.0.1:<port>`, what [`PLACEHOLDER`] stands for.
    pub fn address(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Streams a synthetic body without allocating its total length.
    pub fn stream_bytes(&self, name: &str, count: usize) {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                format!("/{}", name.trim_start_matches('/')),
                Body::Repeated(count),
            );
    }

    /// From now on `/name` answers `bytes`.
    pub fn serve(&self, name: &str, bytes: Vec<u8>) {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                format!("/{}", name.trim_start_matches('/')),
                Body::Bytes(bytes),
            );
    }
}

impl Drop for WebServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // The thread waits in `accept`: one more connection wakes it to
        // see that it is to stop.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Reads one request and answers it: the file, what `/echo` was sent, or
/// 404 for a name nobody served. The connection closes after it.
fn answer(stream: TcpStream, files: &Files) {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let mut words = line.split_whitespace();
    let method = words.next().unwrap_or("GET").to_owned();
    let path = words.next().unwrap_or("/");
    let path = path.split(['?', '#']).next().unwrap_or(path).to_owned();
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) | Err(_) => break,
            Ok(_) if header.trim().is_empty() => break,
            Ok(_) => {
                if let Some((name, value)) = header.split_once(':') {
                    headers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
                }
            }
        }
    }
    let length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<u64>().ok())
        .unwrap_or(0);
    let mut sent = Vec::new();
    if (&mut reader).take(length).read_to_end(&mut sent).is_err() {
        return;
    }
    let (status, content_type, body) = if path == "/echo" {
        let mut text = format!("{method}\n");
        // By name: clients send headers in orders of their own.
        headers.sort();
        for (name, value) in &headers {
            if name == "content-type" || name.starts_with("x-") {
                text.push_str(&format!("{name}: {value}\n"));
            }
        }
        text.push('\n');
        let mut body = text.into_bytes();
        body.extend_from_slice(&sent);
        ("200 OK", "text/plain", body)
    } else {
        let served = files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&path)
            .cloned();
        if let Some(Body::Repeated(count)) = served {
            let mut stream = &stream;
            let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(5)));
            if stream.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: video/mp4\r\nConnection: close\r\n\r\n").is_err() {
                return;
            }
            let piece = [0u8; 8192];
            let mut remaining = count;
            while remaining > 0 {
                let n = remaining.min(piece.len());
                if write!(stream, "{n:x}\r\n")
                    .and_then(|()| stream.write_all(&piece[..n]))
                    .and_then(|()| stream.write_all(b"\r\n"))
                    .is_err()
                {
                    return;
                }
                remaining -= n;
            }
            let _ = stream.write_all(b"0\r\n\r\n");
            return;
        }
        match served {
            Some(Body::Bytes(bytes)) => ("200 OK", content_type_of(&path), bytes),
            Some(Body::Repeated(_)) => unreachable!(),
            None => ("404 Not Found", "text/plain", b"Not Found".to_vec()),
        }
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: {content_type}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut stream = &stream;
    let _ = stream.write_all(head.as_bytes());
    if method != "HEAD" {
        let _ = stream.write_all(&body);
    }
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
}

/// What a file is said to be, by its suffix: text a script reads by its
/// type, and bytes for the rest (images and media are told by their bytes).
fn content_type_of(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, suffix)| suffix) {
        Some("json") => "application/json",
        Some("txt") => "text/plain",
        Some("xml") => "text/xml",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(server: &WebServer, path: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        reply
    }

    #[test]
    fn serves_what_it_was_given_and_nothing_else() {
        let server = WebServer::start().unwrap();
        server.serve("a.txt", b"hello".to_vec());
        let reply = get(&server, "/a.txt");
        assert!(reply.starts_with("HTTP/1.1 200 OK\r\n"), "{reply}");
        assert!(reply.ends_with("\r\n\r\nhello"), "{reply}");
        assert!(get(&server, "/b.txt").starts_with("HTTP/1.1 404 Not Found\r\n"));
    }

    #[test]
    fn echo_answers_with_what_it_was_sent() {
        let server = WebServer::start().unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        write!(
            stream,
            "PUT /echo HTTP/1.1\r\nHost: x\r\nX-Probe: yes\r\nContent-Length: 2\r\n\r\nhi"
        )
        .unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        assert!(
            reply.ends_with("\r\n\r\nPUT\nx-probe: yes\n\nhi"),
            "{reply}"
        );
    }
}
