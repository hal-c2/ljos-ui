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
//! a reader cannot say that it will not draw an image. A reader alone lets
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
/// every live reader of its kind has one, or [`HOLD`] after the first did.
pub(crate) fn uploaded<'a>(reader: &Reader, images: impl IntoIterator<Item = &'a TextureImage>) {
    held::uploaded(reader, images);
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

    pub(super) fn uploaded<'a>(_: &Reader, images: impl IntoIterator<Item = &'a TextureImage>) {
        for image in images {
            image.0.uploaded();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod held {
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard, PoisonError, Weak};
    use std::time::Instant;

    use super::{HOLD, Kind, Reader};
    use crate::texture::{TextureImage, TexturePixels};

    /// An image's pixels kept for the readers that have no copy yet.
    struct Held {
        due: Instant,
        image: Weak<dyn TexturePixels>,
        /// The readers that have their copy.
        by: Vec<u64>,
    }

    #[derive(Default)]
    struct Due {
        /// The live readers, by id.
        readers: HashMap<u64, Kind>,
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
    }

    static DUE: LazyLock<(Mutex<Due>, Condvar)> = LazyLock::new(Default::default);

    fn due() -> MutexGuard<'static, Due> {
        DUE.0.lock().unwrap_or_else(PoisonError::into_inner)
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
            let covered: Vec<_> = d
                .held
                .iter()
                .filter(|((_, kind), h)| *kind == reader.kind && d.all_have(*kind, &h.by))
                .map(|(key, _)| *key)
                .collect();
            covered
                .into_iter()
                .filter_map(|key| d.held.remove(&key)?.image.upgrade())
                .collect::<Vec<_>>()
        };
        for image in images {
            image.uploaded();
        }
    }

    pub(super) fn uploaded<'a>(
        reader: &Reader,
        images: impl IntoIterator<Item = &'a TextureImage>,
    ) {
        let mut now_go = Vec::new();
        {
            let mut d = due();
            for image in images {
                if keep(&mut d, reader, image) {
                    continue;
                }
                now_go.push(Arc::clone(&image.0));
            }
        }
        // Outside the lock: an image takes its own.
        for image in now_go {
            image.uploaded();
        }
    }

    /// Whether `image` is kept, now that `reader` has its copy.
    fn keep(d: &mut Due, reader: &Reader, image: &TextureImage) -> bool {
        let key = (image.0.id(), reader.kind);
        if let Some(h) = d.held.get_mut(&key)
            && std::ptr::addr_eq(h.image.as_ptr(), Arc::as_ptr(&image.0))
        {
            if !h.by.contains(&reader.id) {
                h.by.push(reader.id);
            }
            if d.all_have(reader.kind, &d.held[&key].by) {
                d.held.remove(&key);
                return false;
            }
            return true;
        }
        let by = vec![reader.id];
        if d.all_have(reader.kind, &by) || !started(d) {
            return false;
        }
        let at = Instant::now() + HOLD;
        d.held.insert(
            key,
            Held {
                due: at,
                image: Arc::downgrade(&image.0),
                by,
            },
        );
        d.order.push_back((at, key));
        if d.order.len() == 1 {
            DUE.1.notify_one();
        }
        true
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
                        break take_due(&mut d, now);
                    }
                    d = DUE
                        .1
                        .wait_timeout(d, at - now)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }
            };
            for image in images {
                image.uploaded();
            }
        }
    }

    fn take_due(d: &mut Due, now: Instant) -> Vec<Arc<dyn TexturePixels>> {
        let mut images = Vec::new();
        while let Some(&(at, key)) = d.order.front()
            && at <= now
        {
            d.order.pop_front();
            // Only what this time was for: what went before it was due,
            // and was kept again since, is due later.
            if d.held.get(&key).is_some_and(|h| h.due == at)
                && let Some(h) = d.held.remove(&key)
                && let Some(image) = h.image.upgrade()
            {
                images.push(image);
            }
        }
        images
    }
}
