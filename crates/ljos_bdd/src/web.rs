//! A web server of the scenario's own, on the loopback interface: what an
//! application fetches over HTTP comes from here, never from the internet.
//! Both backends reach it (`qml6` is another process), so it listens on a
//! real port and answers on a thread of its own. It stops with the
//! scenario.
//!
//! The QML names it as `$SERVER`, which a scenario's files have replaced
//! by `http://127.0.0.1:<port>` before they are loaded.

use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

/// What the QML writes for the server's address.
pub const PLACEHOLDER: &str = "$SERVER";

type Files = Arc<Mutex<HashMap<String, Vec<u8>>>>;

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

    /// From now on `/name` answers `bytes`.
    pub fn serve(&self, name: &str, bytes: Vec<u8>) {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(format!("/{}", name.trim_start_matches('/')), bytes);
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

/// Reads one request and answers it: the file, or 404 for a name nobody
/// served. The connection closes after it.
fn answer(stream: TcpStream, files: &Files) {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let path = line.split_whitespace().nth(1).unwrap_or("/");
    let path = path.split(['?', '#']).next().unwrap_or(path).to_owned();
    // The headers say nothing this server needs.
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) | Err(_) => break,
            Ok(_) if header.trim().is_empty() => break,
            Ok(_) => {}
        }
    }
    let body = files
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&path)
        .cloned();
    let (status, body) = match body {
        Some(bytes) => ("200 OK", bytes),
        None => ("404 Not Found", b"Not Found".to_vec()),
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut stream = &stream;
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
}

#[cfg(test)]
mod tests {
    use std::io::Read as _;

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
}
