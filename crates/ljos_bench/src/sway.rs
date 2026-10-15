//! The compositor's side of a window: sway's IPC (the i3 protocol) says
//! when a window is mapped and where it is.
//!
//! An xdg toplevel is mapped when its first buffer is committed, so the
//! `new` window event is the first frame the compositor has from the
//! application: a signal from outside it, the same for every runtime.

use std::{
    io::{Read, Write},
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

fn receive(stream: &mut UnixStream) -> std::io::Result<(u32, Value)> {
    let mut head = [0u8; 14];
    stream.read_exact(&mut head)?;
    if &head[..6] != MAGIC {
        return Err(std::io::Error::other("not an i3-ipc message"));
    }
    let len = u32::from_ne_bytes(head[6..10].try_into().unwrap_or_default());
    let kind = u32::from_ne_bytes(head[10..14].try_into().unwrap_or_default());
    let mut body = vec![0; len as usize];
    stream.read_exact(&mut body)?;
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
        let (_, reply) = receive(&mut stream)?;
        if reply["success"] != Value::Bool(true) {
            return Err(std::io::Error::other(format!(
                "sway refused the subscription: {reply}"
            )));
        }
        let (tx, rx) = channel();
        thread::Builder::new()
            .name("bench-sway".to_owned())
            .spawn(move || {
                while let Ok((kind, event)) = receive(&mut stream) {
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
    /// height of its content.
    ///
    /// # Errors
    /// The compositor cannot be reached.
    pub fn window_rect(&self, pid: u32) -> std::io::Result<Option<(i32, i32, i32, i32)>> {
        let mut stream = self.connect()?;
        send(&mut stream, GET_TREE, "")?;
        let (_, tree) = receive(&mut stream)?;
        Ok(find(&tree, pid))
    }

    /// The size of the whole layout, which absolute pointer motion is
    /// relative to.
    ///
    /// # Errors
    /// The compositor cannot be reached.
    pub fn layout(&self) -> std::io::Result<Option<(i32, i32)>> {
        let mut stream = self.connect()?;
        send(&mut stream, GET_TREE, "")?;
        let (_, tree) = receive(&mut stream)?;
        let r = &tree["rect"];
        let get = |k: &str| r[k].as_i64().and_then(|v| i32::try_from(v).ok());
        Ok(get("width").zip(get("height")))
    }

    /// Runs a sway command (`[pid=…] focus`).
    ///
    /// # Errors
    /// The compositor cannot be reached.
    pub fn command(&self, command: &str) -> std::io::Result<Value> {
        let mut stream = self.connect()?;
        send(&mut stream, RUN_COMMAND, command)?;
        Ok(receive(&mut stream)?.1)
    }
}

fn find(node: &Value, pid: u32) -> Option<(i32, i32, i32, i32)> {
    if node["pid"].as_u64() == Some(u64::from(pid)) {
        let r = &node["rect"];
        let get = |k: &str| r[k].as_i64().and_then(|v| i32::try_from(v).ok());
        return Some((get("x")?, get("y")?, get("width")?, get("height")?));
    }
    ["nodes", "floating_nodes"]
        .iter()
        .filter_map(|k| node[*k].as_array())
        .flatten()
        .find_map(|child| find(child, pid))
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
}
