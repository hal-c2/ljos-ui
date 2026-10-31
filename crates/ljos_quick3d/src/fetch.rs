//! The engine's way of reading a scene's files, for the loaders that
//! [`ljos_3d::fetch`] leaves the reading to: an application's resources
//! first, then the network for a remote URL, then the disk inside the
//! sandbox.

use std::cell::Cell;
use std::rc::Rc;

use ljos::item::ljos_3d::fetch::{Fetch, Reply};
use ljos::prelude::*;

/// Reads for `owner`: what it asked is dropped, reply and all, once it is
/// gone.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EngineFetch {
    owner: ObjectId,
    /// Whether a file on disk is read on the compute pool, so the engine's
    /// thread never waits on it, or at once: an instance table is small,
    /// and its count is there as soon as the source is set, as it is in Qt.
    pooled: bool,
}

impl EngineFetch {
    pub(crate) fn pooled(owner: ObjectId) -> Self {
        EngineFetch {
            owner,
            pooled: true,
        }
    }

    pub(crate) fn at_once(owner: ObjectId) -> Self {
        EngineFetch {
            owner,
            pooled: false,
        }
    }
}

impl Fetch for EngineFetch {
    type Cx = Engine;

    fn fetch(&self, e: &Engine, url: &str, reply: Reply<Engine>) {
        let platform = Platform::of(e);
        if let Some(bytes) = platform.resource(url) {
            return reply(e, Ok(bytes.to_vec()));
        }
        if is_remote(url) {
            platform.fetch_for(Some(self.owner), url, move |e, got| {
                reply(e, got.map_err(|err| err.to_string()));
            });
            return;
        }
        let path = match url_path(url).map(|p| platform.env().real(&p)) {
            Some(Ok(path)) => path,
            Some(Err(err)) => return reply(e, Err(err.to_string())),
            None => return reply(e, Err("no such file".to_owned())),
        };
        if !self.pooled {
            return reply(e, std::fs::read(path).map_err(|err| err.to_string()));
        }
        // Kept outside the task too, so a full pool can still answer.
        let reply = Rc::new(Cell::new(Some(reply)));
        let answer = reply.clone();
        let spawned = ljos::task::spawn_for(
            e,
            format!("read {url}"),
            Some(self.owner),
            move || std::fs::read(path).map_err(|err| err.to_string()),
            move |e, got| {
                if let Some(reply) = answer.take() {
                    reply(
                        e,
                        got.unwrap_or_else(|_| Err("reading the file failed".to_owned())),
                    );
                }
            },
        );
        if spawned.is_err()
            && let Some(reply) = reply.take()
        {
            reply(e, Err("too much work is waiting".to_owned()));
        }
    }
}
