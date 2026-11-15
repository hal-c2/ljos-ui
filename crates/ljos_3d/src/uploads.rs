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
//! Pixels no reader uploads go too, when a frame ended [`UNSHOWN`] ago and
//! none uploaded them since, for images that say so
//! ([`TextureImage::let_go_unless_drawn`]): a scene holds the pictures of
//! its other states and of emitters that have not emitted, and the car
//! configurator kept 57 MiB of them it never drew. A renderer that draws
//! one later decodes it then, as it would the first time.
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

/// Lets `image`'s pixels go if no renderer uploads them: see
/// [`TextureImage::let_go_unless_drawn`].
pub(crate) fn unless_drawn(image: &TextureImage) {
    held::unless_drawn(image);
}

/// How long pixels no renderer uploaded are kept after a frame ends: the
/// frame an image arrives in may be over before its view is drawn again
/// to show it.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) const UNSHOWN: std::time::Duration = std::time::Duration::from_secs(2);

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

    pub(super) fn unless_drawn(_: &TextureImage) {}

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

    use super::{HOLD, Kind, Reader, UNSHOWN};
    use crate::texture::{TextureImage, TexturePixels};

    /// An image's pixels kept for the readers that have no copy yet.
    struct Held {
        /// When they go if a reader still has none: [`HOLD`] after the
        /// frame that kept them ended. `None` while it draws.
        due: Option<Instant>,
        /// The reader whose upload kept them.
        by_whom: u64,
        image: Weak<dyn TexturePixels>,
        /// Other wrappers of the same pixels that readers uploaded: they
        /// may share them with `image`, so they go when it goes.
        others: Vec<Weak<dyn TexturePixels>>,
        /// The readers that have their copy.
        by: Vec<u64>,
    }

    impl Held {
        /// Every wrapper this keeps, `image` first.
        fn wrappers(self) -> Vec<Weak<dyn TexturePixels>> {
            let mut all = self.others;
            all.insert(0, self.image);
            all
        }

        fn keeps(&self, wrapper: &Weak<dyn TexturePixels>) -> bool {
            Weak::ptr_eq(&self.image, wrapper)
                || self.others.iter().any(|w| Weak::ptr_eq(w, wrapper))
        }
    }

    /// Pixels no reader has uploaded since they came.
    struct Unshown {
        /// When they go: [`UNSHOWN`] after a frame ended. `None` until one
        /// does, as nothing could have drawn them before.
        due: Option<Instant>,
        image: Weak<dyn TexturePixels>,
    }

    #[derive(Default)]
    struct Due {
        /// The live readers, by id.
        readers: HashMap<u64, Kind>,
        /// The readers inside a frame, with how many of their frames are
        /// open (a renderer draws a reflection or what is behind a view in
        /// a frame inside its own): what they lack is not let go until the
        /// outermost ends, however long it takes.
        drawing: HashMap<u64, u32>,
        /// What is kept, by image id and the kind it was uploaded as.
        held: HashMap<(u64, Kind), Held>,
        /// The same, in the order they are due: [`HOLD`] is the same for
        /// all.
        order: VecDeque<(Instant, (u64, Kind))>,
        /// What no reader has uploaded, by image id.
        unshown: HashMap<u64, Unshown>,
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
                .keys()
                .any(|id| self.readers.get(id) == Some(&kind) && !by.contains(id))
        }

        /// Takes the hold of `key` away, and lets its wrappers go.
        fn take(&mut self, key: (u64, Kind)) -> Vec<Arc<dyn TexturePixels>> {
            match self.held.remove(&key) {
                Some(held) => self.release(key, held.wrappers()),
                None => Vec::new(),
            }
        }

        /// The wrappers of the image of `key` that go now: none while the
        /// image is kept as the other kind, whose hold takes them, as an
        /// image lets its pixels go for every kind at once.
        fn release(
            &mut self,
            (id, kind): (u64, Kind),
            wrappers: Vec<Weak<dyn TexturePixels>>,
        ) -> Vec<Arc<dyn TexturePixels>> {
            let other = match kind {
                Kind::Maps => Kind::Probes,
                Kind::Probes => Kind::Maps,
            };
            if let Some(h) = self.held.get_mut(&(id, other)) {
                for w in wrappers {
                    if !h.keeps(&w) {
                        h.others.push(w);
                    }
                }
                return Vec::new();
            }
            wrappers.iter().filter_map(Weak::upgrade).collect()
        }

        /// Starts the clock of what `reader` kept in the frame that ends,
        /// and of what no reader has uploaded.
        fn frame_ended(&mut self, reader: u64, now: Instant) {
            let mut unshown = false;
            for u in self.unshown.values_mut() {
                if u.due.is_none() {
                    u.due = Some(now + UNSHOWN);
                    unshown = true;
                }
            }
            if unshown {
                DUE.1.notify_one();
            }
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
            let mut images: Vec<_> = due.into_iter().flat_map(|key| self.take(key)).collect();
            // A reader inside a frame may be about to upload them.
            if self.drawing.is_empty() {
                let unshown: Vec<_> = self
                    .unshown
                    .iter()
                    .filter(|(_, u)| u.due.is_some_and(|at| at <= now))
                    .map(|(id, _)| *id)
                    .collect();
                images.extend(
                    unshown
                        .into_iter()
                        .filter_map(|id| self.unshown.remove(&id)?.image.upgrade()),
                );
            }
            images
        }

        /// When the thread is next to let something go: what no reader has
        /// uploaded waits while one draws, and goes when its frame ends.
        fn next(&self) -> Option<Instant> {
            let held = self.order.front().map(|(at, _)| *at);
            let unshown = if self.drawing.is_empty() {
                self.unshown.values().filter_map(|u| u.due).min()
            } else {
                None
            };
            match (held, unshown) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            }
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

    /// Without the thread nothing is let go for not being drawn.
    pub(super) fn unless_drawn(image: &TextureImage) {
        let mut d = due();
        if started(&mut d) {
            d.unshown.entry(image.0.id()).or_insert_with(|| Unshown {
                due: None,
                image: Arc::downgrade(&image.0),
            });
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
                .flat_map(|key| d.take(key))
                .collect::<Vec<_>>()
        };
        let_go(images);
    }

    pub(super) fn frame_began(readers: &[u64]) {
        let mut d = due();
        for reader in readers {
            *d.drawing.entry(*reader).or_insert(0) += 1;
        }
    }

    /// A frame of `readers` ended. When it was their outermost, the clock
    /// of what they kept starts, and what waited only for them while it
    /// was due goes.
    pub(super) fn frame_ended(readers: &[u64]) {
        let images = {
            let mut d = due();
            let now = Instant::now();
            for reader in readers {
                if let Some(open) = d.drawing.get_mut(reader) {
                    *open -= 1;
                    if *open > 0 {
                        continue;
                    }
                }
                d.drawing.remove(reader);
                d.frame_ended(*reader, now);
            }
            if d.drawing.is_empty() && !d.unshown.is_empty() {
                // The thread waits for no frame now.
                DUE.1.notify_one();
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
        d.unshown.remove(&key.0);
        let wrapper = Arc::downgrade(&image.0);
        let mut by = vec![reader.id];
        let mut due = None;
        let mut by_whom = reader.id;
        let mut others = Vec::new();
        if let Some(h) = d.held.remove(&key) {
            // The readers that have another wrapper of the same pixels
            // have this one's; that wrapper may share the pixels with this
            // one, so it is kept with it.
            for id in &h.by {
                if !by.contains(id) {
                    by.push(*id);
                }
            }
            due = h.due;
            by_whom = h.by_whom;
            others = h.wrappers();
            others.retain(|w| !Weak::ptr_eq(w, &wrapper));
        }
        if d.all_have(reader.kind, &by) || !started(d) {
            others.insert(0, wrapper);
            now_go.extend(d.release(key, others));
            return;
        }
        if due.is_none() && !d.drawing.contains_key(&by_whom) {
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
                image: wrapper,
                others,
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
                    let Some(at) = d.next() else {
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::texture::{PixelEncoding, TexturePixels, fresh_pixels_id};

    /// One wrapper of pixels it may share with others, and how often it
    /// was told to let them go.
    #[derive(Debug)]
    struct Wrapper {
        id: u64,
        pixels: Arc<Mutex<Option<Vec<u8>>>>,
        let_go: AtomicUsize,
    }

    impl TexturePixels for Wrapper {
        fn id(&self) -> u64 {
            self.id
        }
        fn size(&self) -> [u32; 2] {
            [1, 1]
        }
        fn encoding(&self) -> PixelEncoding {
            PixelEncoding::Srgb8Straight
        }
        fn opaque(&self) -> bool {
            true
        }
        fn with_pixels(&self, f: &mut dyn FnMut(&[u8])) {
            f(self.pixels.lock().unwrap().as_deref().unwrap_or_default());
        }
        fn uploaded(&self) {
            self.let_go.fetch_add(1, Ordering::Relaxed);
            *self.pixels.lock().unwrap() = None;
        }
    }

    fn wrapper(id: u64, pixels: &Arc<Mutex<Option<Vec<u8>>>>) -> (TextureImage, Arc<Wrapper>) {
        let w = Arc::new(Wrapper {
            id,
            pixels: Arc::clone(pixels),
            let_go: AtomicUsize::new(0),
        });
        (TextureImage(w.clone()), w)
    }

    fn pixels() -> Arc<Mutex<Option<Vec<u8>>>> {
        Arc::new(Mutex::new(Some(vec![0; 4])))
    }

    #[test]
    fn a_frame_drawn_inside_a_frame_does_not_end_it() {
        let first = Reader::new(Kind::Maps);
        let maps = Reader::new(Kind::Maps);
        let probes = Reader::new(Kind::Probes);
        let outer = Frame::begin([&maps, &probes]);
        // A reflection drawn before the maps are reached.
        drop(Frame::begin([&maps, &probes]));
        let (image, w) = wrapper(fresh_pixels_id(), &pixels());
        uploaded(&first, [&image]);
        std::thread::sleep(HOLD + std::time::Duration::from_millis(300));
        assert_eq!(
            w.let_go.load(Ordering::Relaxed),
            0,
            "let go while a renderer that lacks them still draws its frame"
        );
        drop(outer);
    }

    /// Whether `w` lets its pixels go within a few seconds: another test's
    /// frame may be open a while.
    fn let_go_soon(w: &Wrapper) -> bool {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < until {
            if w.let_go.load(Ordering::Relaxed) > 0 {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn pixels_no_reader_drew_are_not_let_go_while_one_draws() {
        let maps = Reader::new(Kind::Maps);
        let probes = Reader::new(Kind::Probes);
        let (image, w) = wrapper(fresh_pixels_id(), &pixels());
        unless_drawn(&image);
        drop(Frame::begin([&maps, &probes]));
        // A frame as slow as a slow device's first: it may draw them yet.
        let drawing = Frame::begin([&maps, &probes]);
        std::thread::sleep(UNSHOWN + std::time::Duration::from_millis(300));
        assert_eq!(
            w.let_go.load(Ordering::Relaxed),
            0,
            "let go while a renderer that may draw them draws its frame"
        );
        drop(drawing);
        assert!(let_go_soon(&w), "kept after the frame ended without them");
    }

    #[test]
    fn a_wrapper_another_replaced_is_kept_with_it() {
        let [one, two, three] = [(); 3].map(|()| Reader::new(Kind::Maps));
        let shared = pixels();
        let id = fresh_pixels_id();
        let (first, first_w) = wrapper(id, &shared);
        let (second, _) = wrapper(id, &shared);
        uploaded(&one, [&first]);
        uploaded(&two, [&second]);
        assert_eq!(
            first_w.let_go.load(Ordering::Relaxed),
            0,
            "the first wrapper let the pixels go while a reader still lacks them"
        );
        assert!(shared.lock().unwrap().is_some());
        drop(three);
    }
}
