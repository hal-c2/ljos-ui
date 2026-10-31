//! A `Model`'s `source` as a mesh file, read once for all the models that
//! name it so that the renderer uploads it once.
//!
//! Nothing waits for a file: the model draws nothing until it is there, then
//! the views that asked are painted again.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Weak};

use ljos::item;
use ljos::item::ljos_3d::fetch::Fetch;
use ljos::item::ljos_3d::mesh_file::MeshFile;
use ljos::prelude::*;

use crate::Model;
use crate::fetch::EngineFetch;

/// What has come of a model's `source`.
#[derive(Debug, Default)]
pub(crate) struct Loaded {
    url: String,
    state: State,
}

#[derive(Debug, Default)]
enum State {
    #[default]
    Idle,
    Reading,
    /// Held here: the file is kept while a model names it.
    Ready(Arc<MeshFile>),
    /// Not read again until the source changes.
    Failed,
}

/// The engine's files: read or being read, by URL.
#[derive(Debug, Default)]
struct MeshFiles {
    /// Who waits on each read: a model, and a view to paint again.
    reading: RefCell<HashMap<String, Vec<(ObjectId, ObjectId)>>>,
    /// Weak, as the models hold what is in use.
    ready: RefCell<HashMap<String, Weak<MeshFile>>>,
    /// The files already warned of, as a list of models naming a bad file
    /// would otherwise say so once each.
    warned: RefCell<HashSet<String>>,
}

/// The file the model `obj` names, read if it is not yet: `None` until
/// `view` is painted again for it. Tracked.
pub(crate) fn file_of(e: &Engine, view: ObjectId, obj: ObjectId) -> Option<Arc<MeshFile>> {
    let source = item::prop(e, obj, Model::SOURCE);
    if source.0.as_str().is_empty() {
        return None;
    }
    // A source a file wrote is relative to that file, as an Image's.
    let url = e.resolve_url(obj, source.0.as_str());
    let files = e.extension::<MeshFiles>();
    let found = e
        .with(obj, |m: &Model| {
            let mut loaded = m.mesh.borrow_mut();
            if loaded.url != url {
                *loaded = Loaded {
                    url: url.clone(),
                    state: State::Idle,
                };
            }
            match &loaded.state {
                State::Ready(file) => Some(Some(file.clone())),
                State::Failed => Some(None),
                State::Reading => {
                    let mut reading = files.reading.borrow_mut();
                    let waiting = reading.entry(url.clone()).or_default();
                    if !waiting.contains(&(obj, view)) {
                        waiting.push((obj, view));
                    }
                    Some(None)
                }
                State::Idle => {
                    let shared = files.ready.borrow().get(&url).and_then(Weak::upgrade);
                    if let Some(file) = shared {
                        loaded.state = State::Ready(file.clone());
                        return Some(Some(file));
                    }
                    loaded.state = State::Reading;
                    None
                }
            }
        })
        .ok()?;
    if let Some(found) = found {
        return found;
    }
    let first = {
        let mut reading = files.reading.borrow_mut();
        let first = !reading.contains_key(&url);
        reading.entry(url.clone()).or_default().push((obj, view));
        first
    };
    if first {
        read(e, url);
    }
    None
}

fn read(e: &Engine, url: String) {
    let wanted = url.clone();
    EngineFetch::unowned().fetch(
        e,
        &url,
        Box::new(move |e, got| match got {
            Ok(bytes) => parse(e, wanted, bytes),
            Err(why) => arrived(e, &wanted, Err(why)),
        }),
    );
}

/// On the pool, as a large file takes longer than a frame to go through.
fn parse(e: &Engine, url: String, bytes: Vec<u8>) {
    // Kept outside the task too, so a full pool can still answer.
    let wanted = Rc::new(url);
    let answer = wanted.clone();
    let spawned = ljos::task::spawn_for(
        e,
        format!("mesh {wanted}"),
        None,
        move || MeshFile::read(&bytes).map_err(|err| err.to_string()),
        move |e, got| {
            let got = got.unwrap_or_else(|_| Err("reading the file failed".to_owned()));
            arrived(e, &answer, got.map(Arc::new));
        },
    );
    if spawned.is_err() {
        arrived(e, &wanted, Err("too much work is waiting".to_owned()));
    }
}

fn arrived(e: &Engine, url: &str, got: Result<Arc<MeshFile>, String>) {
    let files = e.extension::<MeshFiles>();
    let waiting = files.reading.borrow_mut().remove(url).unwrap_or_default();
    match &got {
        Ok(file) => {
            files
                .ready
                .borrow_mut()
                .insert(url.to_owned(), Arc::downgrade(file));
            files.warned.borrow_mut().remove(url);
        }
        Err(why) => {
            if files.warned.borrow_mut().insert(url.to_owned()) {
                e.warn(format!("Model: {url}: {why}"));
            }
        }
    }
    for (model, view) in waiting {
        // One since gone, or since given another source, wants none of it.
        let wanted = e.with(model, |m: &Model| {
            let mut loaded = m.mesh.borrow_mut();
            if loaded.url != url {
                return false;
            }
            loaded.state = match &got {
                Ok(file) => State::Ready(file.clone()),
                Err(_) => State::Failed,
            };
            true
        });
        if wanted.unwrap_or(false) {
            item::repaint(e, view);
        }
    }
}
