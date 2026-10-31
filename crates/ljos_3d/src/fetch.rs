//! Where a scene's files come from: a glTF file, the buffers and images it
//! names beside itself, an instance table.
//!
//! A host reads them its own way through [`Fetch`]: a desktop from its
//! resources, its disk and the network, a web page with `fetch()`. Every
//! reply is a callback, now or later, so nothing here waits or starts a
//! thread, and a browser's promises fit it as well as a native thread pool.
//! What is not the host's is here: how a file's relative URIs resolve
//! against the file's own URL ([`join`]), and fetching a file's other files
//! together ([`fetch_all`]).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A file's bytes, or why there are none.
pub type Fetched = Result<Vec<u8>, String>;

/// What a host calls with a file's bytes, once.
pub type Reply<C> = Box<dyn FnOnce(&C, Fetched)>;

/// A host's way of reading files.
pub trait Fetch {
    /// What the host hands every reply, such as its engine: a reply comes
    /// back from the host's own loop, where the caller's state lives.
    type Cx: ?Sized + 'static;

    /// Reads `url` (absolute, as [`join`] makes it) and calls `reply` with
    /// its bytes, in this call or later; never more than once. One that is
    /// never called leaves what asked waiting, so a host that drops a
    /// request (its owner gone) drops the reply with it.
    fn fetch(&self, cx: &Self::Cx, url: &str, reply: Reply<Self::Cx>);
}

/// The files one file names, by the URI it wrote for each.
#[derive(Debug, Default)]
pub struct Files(HashMap<String, Fetched>);

impl Files {
    /// The file `uri` named, taken out: each is handed on once.
    pub fn take(&mut self, uri: &str) -> Fetched {
        self.0
            .remove(uri)
            .unwrap_or_else(|| Err(format!("{uri} is not in the file")))
    }
}

/// Fetches every one of `uris`, resolved against `base`, all at once, and
/// calls `done` when the last has answered; at once if there are none.
pub fn fetch_all<F>(
    fetch: &F,
    cx: &F::Cx,
    base: &str,
    uris: Vec<String>,
    done: impl FnOnce(&F::Cx, Files) + 'static,
) where
    F: Fetch,
{
    if uris.is_empty() {
        done(cx, Files::default());
        return;
    }
    type Done<C> = Box<dyn FnOnce(&C, Files)>;
    struct Waiting<C: ?Sized> {
        left: usize,
        files: Files,
        done: Option<Done<C>>,
    }
    let waiting = Rc::new(RefCell::new(Waiting {
        left: uris.len(),
        files: Files::default(),
        done: Some(Box::new(done)),
    }));
    for uri in uris {
        let url = join(base, &uri);
        let waiting = waiting.clone();
        fetch.fetch(
            cx,
            &url,
            Box::new(move |cx, got| {
                let finished = {
                    let mut w = waiting.borrow_mut();
                    w.files.0.insert(uri, got);
                    w.left -= 1;
                    (w.left == 0).then(|| (w.done.take(), std::mem::take(&mut w.files)))
                };
                // Called with the state let go: `done` may fetch again.
                if let Some((Some(done), files)) = finished {
                    done(cx, files);
                }
            }),
        );
    }
}

/// `rel` resolved against the URL of the file that names it. glTF names
/// its files by relative URI references; one with a scheme is taken as it
/// is.
pub fn join(base: &str, rel: &str) -> String {
    let scheme = rel
        .find(':')
        .is_some_and(|i| i > 0 && !rel[..i].contains(['/', '?', '#']));
    if scheme {
        return rel.to_owned();
    }
    let base = base.split(['?', '#']).next().unwrap_or(base);
    if let Some(abs) = rel.strip_prefix('/') {
        // From the root of the base's authority.
        let after_scheme = base.find("://").map_or(0, |i| i + 3);
        let root = base[after_scheme..]
            .find('/')
            .map_or(base.len(), |i| after_scheme + i);
        return format!("{}/{abs}", &base[..root]);
    }
    let dir = base.rfind('/').map_or("", |i| &base[..=i]);
    let mut out: Vec<&str> = dir.split('/').collect();
    // `dir` ends in '/', so its last part is empty: the place for `rel`.
    out.pop();
    for part in rel.split('/') {
        match part {
            "." => {}
            ".." => {
                // Never above the authority (`scheme://host`).
                if out.len() > 3 || (out.len() > 1 && !dir.contains("://")) {
                    out.pop();
                }
            }
            p => out.push(p),
        }
    }
    out.join("/")
}

/// Files kept in memory by URL: a host's test double, or a page that has
/// its files already.
#[derive(Debug, Default, Clone)]
pub struct InMemory {
    files: Rc<RefCell<HashMap<String, Vec<u8>>>>,
    asked: Rc<RefCell<Vec<String>>>,
}

impl InMemory {
    pub fn with(self, url: &str, bytes: impl Into<Vec<u8>>) -> Self {
        self.files.borrow_mut().insert(url.to_owned(), bytes.into());
        self
    }

    /// The URLs asked for, in order.
    pub fn asked(&self) -> Vec<String> {
        self.asked.borrow().clone()
    }
}

impl Fetch for InMemory {
    type Cx = ();

    fn fetch(&self, cx: &(), url: &str, reply: Reply<()>) {
        self.asked.borrow_mut().push(url.to_owned());
        let got = self.files.borrow().get(url).cloned();
        reply(cx, got.ok_or_else(|| "no such file".to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_uris_resolve_against_the_file() {
        assert_eq!(join("file:///a/b/m.gltf", "m.bin"), "file:///a/b/m.bin");
        assert_eq!(
            join("file:///a/b/m.gltf", "tex/c.png"),
            "file:///a/b/tex/c.png"
        );
        assert_eq!(join("file:///a/b/m.gltf", "../c.png"), "file:///a/c.png");
        assert_eq!(
            join("file:///a/b/m.gltf", "./c%20d.png"),
            "file:///a/b/c%20d.png"
        );
        assert_eq!(join("https://h/x/m.gltf?v=1", "m.bin"), "https://h/x/m.bin");
        assert_eq!(join("https://h/x/m.gltf", "/r/m.bin"), "https://h/r/m.bin");
        assert_eq!(join("https://h/m.gltf", "../../m.bin"), "https://h/m.bin");
        assert_eq!(join("qrc:/m/a.gltf", "a.bin"), "qrc:/m/a.bin");
        assert_eq!(
            join("file:///a/m.gltf", "https://h/m.bin"),
            "https://h/m.bin"
        );
    }

    #[test]
    fn a_files_others_are_fetched_together_and_handed_on_by_their_uri() {
        let fetch = InMemory::default()
            .with("https://h/x/a.bin", [1u8])
            .with("https://h/b.png", [2u8]);
        let got = Rc::new(RefCell::new(None));
        let g = got.clone();
        let uris = vec![
            "a.bin".to_owned(),
            "../b.png".to_owned(),
            "c.bin".to_owned(),
        ];
        fetch_all(&fetch, &(), "https://h/x/m.gltf", uris, move |_, files| {
            *g.borrow_mut() = Some(files);
        });
        let mut files = got.borrow_mut().take().expect("answered");
        assert_eq!(files.take("a.bin"), Ok(vec![1]));
        assert_eq!(files.take("../b.png"), Ok(vec![2]));
        assert_eq!(files.take("c.bin"), Err("no such file".to_owned()));
        assert!(files.take("a.bin").is_err(), "handed on once");
    }
}
