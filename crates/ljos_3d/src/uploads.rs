//! When an uploaded image's pixels are let go.
//!
//! An image may let its pixels go once a renderer has its own copy
//! ([`crate::TexturePixels::uploaded`]), and makes them again from its file if
//! anything asks later. A process can draw 3D on more than one device at
//! once, and each uploads for itself: a shell draws a window's views on the
//! window's device, and a view inside an effect's source on the off-screen
//! one too. Pixels let go after the first device's upload were decoded again
//! for the next, on the thread that draws, inside its frame: about 150 ms of
//! the car configurator's first second went to its KTX files a second time.
//! One renderer that reads an image two ways (as a colour and as numbers)
//! uploads it twice and had the same problem.
//!
//! So each renderer's maps and probes are a [`Reader`], and the pixels a
//! reader uploads are kept while another live reader of the same kind does
//! not have them yet: until each has its copy, or for [`HOLD`] at most, as
//! a reader cannot say that it will not draw an image. [`HOLD`] counts from
//! the end of the [`Frame`] that uploaded them, and nothing a reader lacks
//! goes while it is inside a frame of its own. An image read both as maps
//! and as probes lets its pixels go when neither waits. A reader alone lets
//! them go at the upload, so a process that draws on one device keeps no
//! pixel longer than it did. What is kept is let go by a thread that sleeps
//! until the next is due, started the first time anything is kept: nothing
//! is woken while nothing is due.
//!
//! A device made after the pixels went decodes them again: nothing can tell
//! that one is coming. A shell makes one when a window drawn on the
//! processor, whose views were drawn off-screen, moves to GL.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::texture::TextureImage;

/// What a reader uploads images as: renderers read an image as maps and as
/// probes, and a renderer's maps wait only for other renderers' maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    Maps,
    Probes,
}

/// One renderer's uploads of one kind, from when it is made until it goes.
#[derive(Debug)]
// The web keeps nothing for another reader: it has one.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) struct Reader {
    id: u64,
    kind: Kind,
}

impl Reader {
    pub(crate) fn new(kind: Kind) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let reader = Reader {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            kind,
        };
        held::came(&reader);
        reader
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        held::went(self);
    }
}

/// `reader` has its own copy of each of `images`: their pixels go once
/// every live reader of its kind has one, or [`HOLD`] after the frame that
/// uploaded them, unless a reader without a copy is inside a frame then.
pub(crate) fn uploaded<'a>(reader: &Reader, images: impl IntoIterator<Item = &'a TextureImage>) {
    held::uploaded(reader, images);
}

/// One renderer's frame, from [`Frame::begin`] until it is dropped. What
/// its readers keep waits [`HOLD`] from its end, and what they lack is not
/// let go while it draws: a frame on a slow device can take longer than
/// [`HOLD`] between its first upload and its last.
#[must_use]
pub(crate) struct Frame([u64; 2]);

impl Frame {
    pub(crate) fn begin(readers: [&Reader; 2]) -> Self {
        let ids = readers.map(|r| r.id);
        held::frame_began(&ids);
        Frame(ids)
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        held::frame_ended(&self.0);
    }
}

/// How long pixels are kept at most for a reader that has no copy yet. A
/// shell draws a view's picture for an effect within a few hundred
/// milliseconds of the window's device drawing the view (the car
/// configurator: 290 ms).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) const HOLD: std::time::Duration = std::time::Duration::from_secs(1);

/// The web has no threads to sleep on, and draws on one device: there the
/// pixels go at the upload.
#[cfg(target_arch = "wasm32")]
mod held {
    use super::Reader;
    use crate::texture::TextureImage;

    pub(super) fn came(_: &Reader) {}

    pub(super) fn went(_: &Reader) {}

    pub(super) fn frame_began(_: &[u64]) {}

    pub(super) fn frame_ended(_: &[u64]) {}

    pub(super) fn uploaded<'a>(_: &Reader, images: impl IntoIterator<Item = &'a TextureImage>) {
        for image in images {
            image.0.uploaded();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod held {
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard, PoisonError, Weak};
    use std::time::Instant;

    use super::{HOLD, Kind, Reader};
    use crate::texture::{TextureImage, TexturePixels};

    /// An image's pixels kept for the readers that have no copy yet.
    struct Held {
        /// When they go if a reader still has none: [`HOLD`] after the
        /// frame that kept them ended. `None` while it draws.
        due: Option<Instant>,
        /// The reader whose upload kept them.
        by_whom: u64,
        image: Weak<dyn TexturePixels>,
        /// The readers that have their copy.
        by: Vec<u64>,
    }

    #[derive(Default)]
    struct Due {
        /// The live readers, by id.
        readers: HashMap<u64, Kind>,
        /// The readers inside a frame: what they lack is not let go until
        /// the frame ends, however long it takes.
        drawing: HashSet<u64>,
        /// What is kept, by image id and the kind it was uploaded as.
        held: HashMap<(u64, Kind), Held>,
        /// The same, in the order they are due: [`HOLD`] is the same for
        /// all.
        order: VecDeque<(Instant, (u64, Kind))>,
        /// The thread that lets them go when due is running.
        running: bool,
    }

    impl Due {
        /// Whether every live reader of `kind` is among `by`.
        fn all_have(&self, kind: Kind, by: &[u64]) -> bool {
            self.readers
                .iter()
                .all(|(id, k)| *k != kind || by.contains(id))
        }

        /// Whether a reader of `kind` that has no copy is inside a frame.
        fn awaited(&self, kind: Kind, by: &[u64]) -> bool {
            self.drawing
                .iter()
                .any(|id| self.readers.get(id) == Some(&kind) && !by.contains(id))
        }

        /// Takes the hold of `key` away, and its image when no hold of the
        /// other kind is left for it: an image lets its pixels go for every
        /// kind at once.
        fn take(&mut self, key: (u64, Kind)) -> Option<Arc<dyn TexturePixels>> {
            let held = self.held.remove(&key)?;
            if self.held_as_other(key) {
                return None;
            }
            held.image.upgrade()
        }

        /// Whether the image of `key` is also kept as the other kind.
        fn held_as_other(&self, (id, kind): (u64, Kind)) -> bool {
            let other = match kind {
                Kind::Maps => Kind::Probes,
                Kind::Probes => Kind::Maps,
            };
            self.held.contains_key(&(id, other))
        }

        /// Starts the clock of what `reader` kept in the frame that ends.
        fn frame_ended(&mut self, reader: u64, now: Instant) {
            let at = now + HOLD;
            let mut started = Vec::new();
            for (key, h) in &mut self.held {
                if h.by_whom == reader && h.due.is_none() {
                    h.due = Some(at);
                    started.push(*key);
                }
            }
            let was_empty = self.order.is_empty();
            self.order.extend(started.into_iter().map(|key| (at, key)));
            if was_empty && !self.order.is_empty() {
                DUE.1.notify_one();
            }
        }

        /// What is due by `now` and waits for no reader inside a frame.
        fn take_due(&mut self, now: Instant) -> Vec<Arc<dyn TexturePixels>> {
            let due: Vec<_> = self
                .held
                .iter()
                .filter(|(key, h)| h.due.is_some_and(|at| at <= now) && !self.awaited(key.1, &h.by))
                .map(|(key, _)| *key)
                .collect();
            due.into_iter().filter_map(|key| self.take(key)).collect()
        }
    }

    static DUE: LazyLock<(Mutex<Due>, Condvar)> = LazyLock::new(Default::default);

    fn due() -> MutexGuard<'static, Due> {
        DUE.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Lets `images` go, outside the lock: an image takes its own.
    fn let_go(images: Vec<Arc<dyn TexturePixels>>) {
        for image in images {
            image.uploaded();
        }
    }

    pub(super) fn came(reader: &Reader) {
        due().readers.insert(reader.id, reader.kind);
    }

    /// A reader that goes waits for nothing: what only it had no copy of
    /// goes now.
    pub(super) fn went(reader: &Reader) {
        let images = {
            let mut d = due();
            d.readers.remove(&reader.id);
            d.drawing.remove(&reader.id);
            d.frame_ended(reader.id, Instant::now());
            let covered: Vec<_> = d
                .held
                .iter()
                .filter(|((_, kind), h)| *kind == reader.kind && d.all_have(*kind, &h.by))
                .map(|(key, _)| *key)
                .collect();
            covered
                .into_iter()
                .filter_map(|key| d.take(key))
                .collect::<Vec<_>>()
        };
        let_go(images);
    }

    pub(super) fn frame_began(readers: &[u64]) {
        due().drawing.extend(readers);
    }

    /// The frame of `readers` ended: the clock of what they kept starts,
    /// and what waited only for them while it was due goes.
    pub(super) fn frame_ended(readers: &[u64]) {
        let images = {
            let mut d = due();
            let now = Instant::now();
            for reader in readers {
                d.drawing.remove(reader);
                d.frame_ended(*reader, now);
            }
            d.take_due(now)
        };
        let_go(images);
    }

    pub(super) fn uploaded<'a>(
        reader: &Reader,
        images: impl IntoIterator<Item = &'a TextureImage>,
    ) {
        let mut now_go = Vec::new();
        {
            let mut d = due();
            for image in images {
                keep(&mut d, reader, image, &mut now_go);
            }
        }
        let_go(now_go);
    }

    /// Keeps `image` for the readers of its kind that have no copy, now
    /// that `reader` has one, or puts it in `now_go`.
    fn keep(
        d: &mut Due,
        reader: &Reader,
        image: &TextureImage,
        now_go: &mut Vec<Arc<dyn TexturePixels>>,
    ) {
        let key = (image.0.id(), reader.kind);
        let mut by = vec![reader.id];
        let mut due = None;
        let mut by_whom = reader.id;
        if let Some(h) = d.held.get(&key) {
            if !std::ptr::addr_eq(h.image.as_ptr(), Arc::as_ptr(&image.0)) {
                // Another wrapper of the same pixels: the readers that have
                // its copy have this one's, and it goes now, as this does
                // when they all have it.
                now_go.extend(h.image.upgrade());
            }
            for id in &h.by {
                if !by.contains(id) {
                    by.push(*id);
                }
            }
            due = h.due;
            by_whom = h.by_whom;
        }
        if d.all_have(reader.kind, &by) || !started(d) {
            d.held.remove(&key);
            // The other kind's hold lets the pixels go when it is done.
            if !d.held_as_other(key) {
                now_go.push(Arc::clone(&image.0));
            }
            return;
        }
        if due.is_none() && !d.drawing.contains(&by_whom) {
            // Kept outside a frame: the clock starts now.
            let at = Instant::now() + HOLD;
            due = Some(at);
            d.order.push_back((at, key));
            if d.order.len() == 1 {
                DUE.1.notify_one();
            }
        }
        d.held.insert(
            key,
            Held {
                due,
                by_whom,
                image: Arc::downgrade(&image.0),
                by,
            },
        );
    }

    /// Whether the thread that lets go when due runs, started now if not.
    /// Without it nothing is kept.
    fn started(d: &mut Due) -> bool {
        if !d.running {
            d.running = std::thread::Builder::new()
                .name("ljos_3d pixels".into())
                .spawn(let_go_when_due)
                .is_ok();
        }
        d.running
    }

    /// Sleeps until the next image is due and lets it go, outside the lock.
    /// What waits for a reader inside a frame goes when the frame ends.
    fn let_go_when_due() {
        loop {
            let images = {
                let mut d = due();
                loop {
                    let Some(&(at, _)) = d.order.front() else {
                        d = DUE.1.wait(d).unwrap_or_else(PoisonError::into_inner);
                        continue;
                    };
                    let now = Instant::now();
                    if at <= now {
                        while d.order.front().is_some_and(|(at, _)| *at <= now) {
                            d.order.pop_front();
                        }
                        break d.take_due(now);
                    }
                    d = DUE
                        .1
                        .wait_timeout(d, at - now)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }
            };
            let_go(images);
        }
    }
}
