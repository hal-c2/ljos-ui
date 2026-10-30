//! One JSON-RPC message posted to an inspector, and the one client thread
//! that posts them for code without a runtime of its own.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt as _, Full};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite};

use super::token::{Address, Target};

/// The inspector could not be asked at all, or gave no answer to read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Unreached(pub String);

impl From<String> for Unreached {
    fn from(why: String) -> Self {
        Unreached(why)
    }
}

/// The longest `Retry-After` that is waited for: a server asking for more
/// is answered with its refusal.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(5);

/// What the inspector sent back: its status and the body, as it came.
#[derive(Debug)]
pub struct Answer {
    pub status: u16,
    pub body: Vec<u8>,
    /// When the inspector turned the call away (429), how long its
    /// `Retry-After` says to wait.
    pub retry_after: Option<Duration>,
}

impl Answer {
    /// The JSON-RPC messages of the body: the `data:` lines of a stream,
    /// or the body itself when it is plain JSON.
    pub fn messages(&self) -> Vec<&str> {
        let text = std::str::from_utf8(&self.body).unwrap_or("");
        let data: Vec<&str> = text
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        match (data.is_empty(), text.trim()) {
            (true, "") => Vec::new(),
            (true, plain) => vec![plain],
            (false, _) => data,
        }
    }
}

/// Posts `message` (one JSON-RPC message as text) to `target`; `Err` is
/// why the inspector could not be reached at all. Gives up after `limit`.
///
/// A call turned away for asking too often (429) is asked again once, after
/// the `Retry-After` the inspector gave, within `limit`: it was refused
/// before anything ran, so asking again cannot do a thing twice. A second
/// refusal is the answer, and so is the first when the wait it asks for is
/// longer than `MAX_RETRY_AFTER` or would not end within `limit`: the call
/// is never asked again before the inspector said it may be.
pub async fn post(target: &Target, message: String, limit: Duration) -> Result<Answer, Unreached> {
    let deadline = tokio::time::Instant::now() + limit;
    let asked = async {
        let first = post_now(target, message.clone()).await?;
        match first.retry_after {
            Some(wait)
                if first.status == 429
                    && wait <= MAX_RETRY_AFTER
                    && tokio::time::Instant::now() + wait < deadline =>
            {
                tokio::time::sleep(wait).await;
                post_now(target, message).await
            }
            _ => Ok(first),
        }
    };
    match tokio::time::timeout(limit, asked).await {
        Ok(answer) => answer,
        Err(_) => Err(Unreached(format!(
            "the inspector at {} did not answer within {} s",
            target.url,
            limit.as_secs()
        ))),
    }
}

async fn post_now(target: &Target, message: String) -> Result<Answer, Unreached> {
    let address = Address::of(&target.url).map_err(|e| Unreached(e.to_string()))?;
    let fail = |e: std::io::Error| Unreached(format!("{}: {e}", target.url));
    match address {
        Address::Tcp { authority, path } => {
            let stream = tokio::net::TcpStream::connect(authority.as_str())
                .await
                .map_err(fail)?;
            exchange(stream, &authority, &path, &target.token, message).await
        }
        #[cfg(unix)]
        Address::Unix(socket) => {
            let stream = tokio::net::UnixStream::connect(&socket)
                .await
                .map_err(fail)?;
            // The inspector answers only requests addressed to this
            // machine, whichever way they come.
            exchange(stream, "localhost", "/mcp", &target.token, message).await
        }
        #[cfg(not(unix))]
        Address::Unix(_) => Err(Unreached(format!("{}: no Unix sockets here", target.url))),
    }
}

struct AbortConnection(tokio::task::AbortHandle);

impl Drop for AbortConnection {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn exchange<S>(
    stream: S,
    host: &str,
    path: &str,
    token: &str,
    message: String,
) -> Result<Answer, Unreached>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let why = |e: &dyn std::fmt::Display| Unreached(e.to_string());
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|e| why(&e))?;
    // A timeout or oversized body must also stop the HTTP driver.
    let connection = tokio::spawn(connection);
    let _connection = AbortConnection(connection.abort_handle());
    let request = http::Request::post(path)
        .header(http::header::HOST, host)
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .header(http::header::ACCEPT, "application/json, text/event-stream")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(message)))
        .map_err(|e| why(&e))?;
    let response = sender.send_request(request).await.map_err(|e| why(&e))?;
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get(http::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs);
    let body = read_body(response.into_body()).await?;
    Ok(Answer {
        status,
        body,
        retry_after,
    })
}

// DECISION: match the server's 1 MiB request cap for inspector responses.
// Count streaming frames rather than trusting Content-Length.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

async fn read_body<B>(mut body: B) -> Result<Vec<u8>, Unreached>
where
    B: hyper::body::Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
{
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|e| Unreached(e.to_string()))?;
        if let Ok(data) = frame.into_data() {
            if data.len() > MAX_BODY_BYTES.saturating_sub(bytes.len()) {
                return Err(Unreached("inspector response exceeds 1 MiB".into()));
            }
            bytes.extend_from_slice(&data);
        }
    }
    Ok(bytes)
}

/// Calls in flight on the client thread at most; one more is refused
/// rather than queued, as the inspector refuses one past its own queue.
pub const IN_FLIGHT: usize = 16;

type Job = Pin<Box<dyn Future<Output = ()> + Send>>;

struct Thread {
    tx: tokio::sync::mpsc::UnboundedSender<Job>,
    in_flight: Arc<AtomicUsize>,
}

/// Runs `job` on the one client thread of the process, made the first
/// time it is asked for; `Err` says why it was not run. A thread per call
/// would be born, wake the process and end each time; this one sleeps in
/// its reactor between calls, with no timer, so an idle client costs
/// nothing. Its runtime is its own, so a caller on another runtime's
/// thread neither blocks that runtime's reactor nor enters it.
pub fn spawn(job: impl Future<Output = ()> + Send + 'static) -> Result<(), Unreached> {
    static THREAD: OnceLock<Result<Thread, Unreached>> = OnceLock::new();
    let thread = THREAD.get_or_init(start).as_ref().map_err(Clone::clone)?;
    let in_flight = Arc::clone(&thread.in_flight);
    if in_flight.fetch_add(1, Ordering::AcqRel) >= IN_FLIGHT {
        in_flight.fetch_sub(1, Ordering::AcqRel);
        return Err(Unreached(format!(
            "busy: {IN_FLIGHT} calls are waiting for the inspector already"
        )));
    }
    let job = Box::pin(async move {
        job.await;
        in_flight.fetch_sub(1, Ordering::AcqRel);
    });
    thread
        .tx
        .send(job)
        .map_err(|_| Unreached("the client thread has ended".to_owned()))
}

fn start() -> Result<Thread, Unreached> {
    // Unbounded in type only: `spawn` counts what is in flight.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|e| Unreached(format!("no runtime for the client: {e}")))?;
    std::thread::Builder::new()
        .name("ljos-inspector-client".to_owned())
        .spawn(move || {
            runtime.block_on(async move {
                while let Some(job) = rx.recv().await {
                    tokio::spawn(job);
                }
            });
        })
        .map_err(|e| Unreached(format!("no thread for the client: {e}")))?;
    Ok(Thread {
        tx,
        in_flight: Arc::new(AtomicUsize::new(0)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(port: u16) -> Target {
        Target {
            url: format!("http://127.0.0.1:{port}/mcp"),
            token: "t".into(),
            pid: None,
        }
    }

    /// An inspector that turns the first call away for a moment, as its
    /// rate limit does, and answers the second.
    #[tokio::test]
    async fn a_call_turned_away_is_asked_again_after_the_retry_after() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = tokio::spawn(async move {
            let mut statuses = Vec::new();
            for reply in [
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            ] {
                let (mut conn, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 4096];
                let _ = conn.read(&mut buf).await.unwrap();
                conn.write_all(reply.as_bytes()).await.unwrap();
                statuses.push(reply.len());
            }
            statuses.len()
        });
        let answer = post(&target(port), "{}".into(), Duration::from_secs(10))
            .await
            .unwrap();
        assert_eq!(answer.status, 200);
        assert_eq!(served.await.unwrap(), 2);
    }

    /// A refusal that asks for a longer wait than is waited for is the
    /// answer at once: the call is not asked again early.
    #[tokio::test]
    async fn a_long_retry_after_is_answered_with_the_refusal() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = tokio::spawn(async move {
            let (mut conn, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = conn.read(&mut buf).await.unwrap();
            conn.write_all(
                b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 60\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            )
            .await
            .unwrap();
            // A second call would be accepted here; none may come.
            tokio::time::timeout(Duration::from_millis(200), listener.accept())
                .await
                .is_err()
        });
        let answer = post(&target(port), "{}".into(), Duration::from_secs(10))
            .await
            .unwrap();
        assert_eq!(answer.status, 429);
        assert_eq!(answer.retry_after, Some(Duration::from_secs(60)));
        assert!(served.await.unwrap(), "the call was asked again");
    }

    #[tokio::test]
    async fn response_frames_are_capped_even_without_content_length() {
        use hyper::body::Frame;
        use std::collections::VecDeque;
        struct Frames(VecDeque<Bytes>);
        impl hyper::body::Body for Frames {
            type Data = Bytes;
            type Error = std::convert::Infallible;
            fn poll_frame(
                mut self: Pin<&mut Self>,
                _: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
                std::task::Poll::Ready(self.0.pop_front().map(|data| Ok(Frame::data(data))))
            }
        }
        let exact = Bytes::from(vec![b'x'; MAX_BODY_BYTES]);
        assert_eq!(
            read_body(Full::new(exact.clone())).await.unwrap().len(),
            MAX_BODY_BYTES
        );
        let oversized = Frames(VecDeque::from([exact, Bytes::from_static(b"x")]));
        assert!(
            read_body(oversized)
                .await
                .unwrap_err()
                .0
                .contains("exceeds 1 MiB")
        );
        assert_eq!(
            read_body(Full::new(Bytes::from_static(b"valid")))
                .await
                .unwrap(),
            b"valid"
        );
    }

    #[test]
    fn an_answer_is_read_plain_or_streamed() {
        let answer = |body: &str| Answer {
            status: 200,
            body: body.as_bytes().to_vec(),
            retry_after: None,
        };
        let plain = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        assert_eq!(answer(plain).messages(), [plain]);
        let streamed = format!("event: message\ndata: {plain}\n\nid: 2\ndata: {plain}\n\n");
        assert_eq!(answer(&streamed).messages(), [plain, plain]);
        assert!(answer("").messages().is_empty());
    }
}
