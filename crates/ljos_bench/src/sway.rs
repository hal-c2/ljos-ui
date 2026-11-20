//! The compositor's side of a window: sway's IPC (the i3 protocol) says
//! when a window is mapped and where it is.
//!
//! An xdg toplevel is mapped when its first buffer is committed, so the
//! `new` window event is the first frame the compositor has from the
//! application: a signal from outside it, the same for every runtime.

use std::{
    io::{ErrorKind, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, channel},
    thread,
    time::Instant,
};

use serde_json::Value;

const MAGIC: &[u8] = b"i3-ipc";
const RUN_COMMAND: u32 = 0;
const SUBSCRIBE: u32 = 2;
const GET_TREE: u32 = 4;

fn send(stream: &mut UnixStream, kind: u32, payload: &str) -> std::io::Result<()> {
    let mut msg = MAGIC.to_vec();
    msg.extend_from_slice(
        &u32::try_from(payload.len())
            .map_err(std::io::Error::other)?
            .to_ne_bytes(),
    );
    msg.extend_from_slice(&kind.to_ne_bytes());
    msg.extend_from_slice(payload.as_bytes());
    stream.write_all(&msg)
}

/// Fills `buf`, by `by` when there is a deadline: the time left is set
/// before every read, so a reply that comes in pieces is held to the one
/// deadline, not to a fresh timeout for each piece. Past it, the read fails
/// with `TimedOut` (or `WouldBlock`, as the socket's own timeout says it).
fn read_by(stream: &mut UnixStream, buf: &mut [u8], by: Option<Instant>) -> std::io::Result<()> {
    let Some(by) = by else {
        return stream.read_exact(buf);
    };
    let mut done = 0;
    while done < buf.len() {
        let left = by.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(ErrorKind::TimedOut.into());
        }
        stream.set_read_timeout(Some(left))?;
        match stream.read(&mut buf[done..]) {
            Ok(0) => return Err(ErrorKind::UnexpectedEof.into()),
            Ok(n) => done += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn receive(stream: &mut UnixStream, by: Option<Instant>) -> std::io::Result<(u32, Value)> {
    let mut head = [0u8; 14];
    read_by(stream, &mut head, by)?;
    if &head[..6] != MAGIC {
        return Err(std::io::Error::other("not an i3-ipc message"));
    }
    let len = u32::from_ne_bytes(head[6..10].try_into().unwrap_or_default());
    let kind = u32::from_ne_bytes(head[10..14].try_into().unwrap_or_default());
    let mut body = vec![0; len as usize];
    read_by(stream, &mut body, by)?;
    let value = serde_json::from_slice(&body).map_err(std::io::Error::other)?;
    Ok((kind, value))
}

/// A window the compositor mapped: whose, and when the event was read.
#[derive(Clone, Debug)]
pub struct Mapped {
    pub pid: u32,
    pub at: Instant,
}

/// The compositor, by its IPC socket.
#[derive(Clone, Debug)]
pub struct Sway {
    socket: PathBuf,
}

impl Sway {
    #[must_use]
    pub fn new(socket: &Path) -> Self {
        Self {
            socket: socket.to_owned(),
        }
    }

    fn connect(&self) -> std::io::Result<UnixStream> {
        UnixStream::connect(&self.socket)
    }

    /// Windows as they are mapped, from now on. Subscribe before starting
    /// the application so its window cannot be missed.
    ///
    /// # Errors
    /// The compositor cannot be reached or refuses the subscription.
    pub fn windows(&self) -> std::io::Result<Receiver<Mapped>> {
        let mut stream = self.connect()?;
        send(&mut stream, SUBSCRIBE, r#"["window"]"#)?;
        let (_, reply) = receive(&mut stream, None)?;
        if reply["success"] != Value::Bool(true) {
            return Err(std::io::Error::other(format!(
                "sway refused the subscription: {reply}"
            )));
        }
        let (tx, rx) = channel();
        thread::Builder::new()
            .name("bench-sway".to_owned())
            .spawn(move || {
                while let Ok((kind, event)) = receive(&mut stream, None) {
                    let at = Instant::now();
                    // Events have the high bit set; anything else is not ours.
                    if kind & 0x8000_0000 == 0 || event["change"] != "new" {
                        continue;
                    }
                    let Some(pid) = event["container"]["pid"].as_u64() else {
                        continue;
                    };
                    let Ok(pid) = u32::try_from(pid) else {
                        continue;
                    };
                    if tx.send(Mapped { pid, at }).is_err() {
                        break;
                    }
                }
            })?;
        Ok(rx)
    }

    /// Where the window of `pid` is, in the layout's pixels: x, y, width,
    /// height of its content. The answer is waited for until `by`, when
    /// there is a deadline.
    ///
    /// # Errors
    /// The compositor cannot be reached, or does not answer by `by`
    /// (`WouldBlock` or `TimedOut`).
    pub fn window_rect(
        &self,
        pid: u32,
        by: Option<Instant>,
    ) -> std::io::Result<Option<(i32, i32, i32, i32)>> {
        let mut stream = self.connect()?;
        send(&mut stream, GET_TREE, "")?;
        let (_, tree) = receive(&mut stream, by)?;
        Ok(find(&tree, pid))
    }

    /// Where the window of `pid` is ([`Sway::window_rect`]), and whether
    /// that place is the size its client last committed. A floating
    /// window's place takes a size the client commits only once the
    /// compositor's transaction for it ends, up to its timeout later, so
    /// the place read meanwhile is the old one.
    ///
    /// # Errors
    /// The compositor cannot be reached, or does not answer by `by`
    /// (`WouldBlock` or `TimedOut`).
    pub fn window_place(&self, pid: u32, by: Option<Instant>) -> std::io::Result<Option<Place>> {
        let mut stream = self.connect()?;
        send(&mut stream, GET_TREE, "")?;
        let (_, tree) = receive(&mut stream, by)?;
        Ok(find_node(&tree, pid).and_then(place))
    }

    /// The size of the whole layout, which absolute pointer motion is
    /// relative to.
    ///
    /// # Errors
    /// The compositor cannot be reached.
    pub fn layout(&self) -> std::io::Result<Option<(i32, i32)>> {
        let mut stream = self.connect()?;
        send(&mut stream, GET_TREE, "")?;
        let (_, tree) = receive(&mut stream, None)?;
        let r = &tree["rect"];
        let get = |k: &str| r[k].as_i64().and_then(|v| i32::try_from(v).ok());
        Ok(get("width").zip(get("height")))
    }

    /// The process whose window has the focus, if a client's window has it.
    /// The answer is waited for until `by`, when there is a deadline.
    ///
    /// # Errors
    /// The compositor cannot be reached, or does not answer by `by`
    /// (`WouldBlock` or `TimedOut`).
    pub fn focused(&self, by: Option<Instant>) -> std::io::Result<Option<u32>> {
        let mut stream = self.connect()?;
        send(&mut stream, GET_TREE, "")?;
        let (_, tree) = receive(&mut stream, by)?;
        Ok(focused(&tree))
    }

    /// Runs a sway command (`[pid=…] focus`). Its reply is waited for until
    /// `by`, when there is a deadline.
    ///
    /// # Errors
    /// The compositor cannot be reached, or does not answer by `by`
    /// (`WouldBlock` or `TimedOut`).
    pub fn command(&self, command: &str, by: Option<Instant>) -> std::io::Result<Value> {
        let mut stream = self.connect()?;
        send(&mut stream, RUN_COMMAND, command)?;
        Ok(receive(&mut stream, by)?.1)
    }
}

fn focused(node: &Value) -> Option<u32> {
    if node["focused"] == Value::Bool(true) {
        return node["pid"].as_u64().and_then(|p| u32::try_from(p).ok());
    }
    ["nodes", "floating_nodes"]
        .iter()
        .filter_map(|k| node[*k].as_array())
        .flatten()
        .find_map(focused)
}

/// A window's place in the layout ([`Sway::window_place`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    /// x, y, width, height of its content, in the layout's pixels.
    pub rect: (i32, i32, i32, i32),
    /// The place is not yet the size the client last committed. Only a
    /// floating window is held to the size it commits; a tiled one is
    /// given its tile's whatever it draws.
    pub resizing: bool,
}

fn find(node: &Value, pid: u32) -> Option<(i32, i32, i32, i32)> {
    find_node(node, pid).and_then(place).map(|p| p.rect)
}

fn find_node(node: &Value, pid: u32) -> Option<&Value> {
    if node["pid"].as_u64() == Some(u64::from(pid)) {
        return Some(node);
    }
    ["nodes", "floating_nodes"]
        .iter()
        .filter_map(|k| node[*k].as_array())
        .flatten()
        .find_map(|child| find_node(child, pid))
}

fn place(node: &Value) -> Option<Place> {
    let get = |r: &Value, k: &str| r[k].as_i64().and_then(|v| i32::try_from(v).ok());
    let r = &node["rect"];
    let rect = (
        get(r, "x")?,
        get(r, "y")?,
        get(r, "width")?,
        get(r, "height")?,
    );
    let g = &node["geometry"];
    let committed = get(g, "width").zip(get(g, "height"));
    let resizing = node["type"] == "floating_con"
        && committed.is_some_and(|(w, h)| w > 0 && (w, h) != (rect.2, rect.3));
    Some(Place { rect, resizing })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_found_by_pid() {
        let tree = serde_json::json!({
            "nodes": [{"nodes": [], "floating_nodes": [
                {"pid": 7, "rect": {"x": 10, "y": 20, "width": 300, "height": 200}}
            ]}]
        });
        assert_eq!(find(&tree, 7), Some((10, 20, 300, 200)));
        assert_eq!(find(&tree, 8), None);
    }

    /// A floating window that committed another size than its place is
    /// still being resized; a tiled one drawn smaller than its tile is not.
    #[test]
    fn a_floating_window_is_resizing_until_its_place_is_its_committed_size() {
        let node = |kind: &str, w: i64, h: i64| {
            serde_json::json!({"type": kind, "pid": 7,
                "rect": {"x": 10, "y": 20, "width": 200, "height": 100},
                "geometry": {"x": 0, "y": 0, "width": w, "height": h}})
        };
        let tree = |n: Value| serde_json::json!({"nodes": [], "floating_nodes": [n]});
        let at = |n: Value| find_node(&tree(n), 7).and_then(place);
        assert_eq!(
            at(node("floating_con", 300, 200)),
            Some(Place {
                rect: (10, 20, 200, 100),
                resizing: true
            })
        );
        assert_eq!(
            at(node("floating_con", 200, 100)).map(|p| p.resizing),
            Some(false)
        );
        assert_eq!(at(node("con", 150, 80)).map(|p| p.resizing), Some(false));
    }

    /// A compositor that answers a byte at a time, each well inside the
    /// time left, is still held to the one deadline: a timeout set once per
    /// read would wait for the whole body.
    #[test]
    fn a_reply_that_trickles_in_ends_at_its_deadline() {
        let (mut ours, mut theirs) = UnixStream::pair().expect("a socket pair");
        let body = vec![b' '; 200];
        let mut head = MAGIC.to_vec();
        head.extend_from_slice(&u32::try_from(body.len()).unwrap_or(0).to_ne_bytes());
        head.extend_from_slice(&GET_TREE.to_ne_bytes());
        let writer = thread::spawn(move || {
            let _ = theirs.write_all(&head);
            for b in body {
                if theirs.write_all(&[b]).is_err() {
                    break;
                }
                thread::sleep(std::time::Duration::from_millis(20));
            }
        });
        let start = Instant::now();
        let by = start + std::time::Duration::from_millis(300);
        let err = receive(&mut ours, Some(by)).expect_err("the deadline passes first");
        let took = start.elapsed();
        assert!(
            matches!(err.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock),
            "{err}"
        );
        assert!(took < std::time::Duration::from_secs(2), "took {took:?}");
        drop(ours);
        let _ = writer.join();
    }

    #[test]
    fn a_reply_in_time_is_read_whole() {
        let (mut ours, mut theirs) = UnixStream::pair().expect("a socket pair");
        send(&mut theirs, RUN_COMMAND, r#"[{"success":true}]"#).expect("sent");
        let by = Instant::now() + std::time::Duration::from_secs(5);
        let (kind, value) = receive(&mut ours, Some(by)).expect("read");
        assert_eq!(kind, RUN_COMMAND);
        assert_eq!(value[0]["success"], Value::Bool(true));
    }
}
