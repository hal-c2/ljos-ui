//! A `MediaPlayer` that plays bytes a program writes as it gets them:
//! what `QMediaPlayer::setSourceDevice` is to Qt.
//!
//! The program writes an elementary stream (MJPEG, or H.264 with start
//! codes or with lengths) or a whole container. A decode thread of the
//! stream's own cuts the bytes into pictures, decodes them and converts
//! them to RGBA, and hands them to the player, which shows them:
//!
//! - **paced**, one every frame period while it plays, in the order they
//!   came. The decoder keeps a few pictures ahead; when they are not
//!   taken (the player is paused), the writer waits, so a stream is never
//!   read faster than it is played.
//! - **live**, at once: no duration, no seeking, and a picture that comes
//!   while another still waits for the Engine replaces it. The writer
//!   never waits on the player.
//!
//! The stream wakes the Engine only for what it has decoded: one job per
//! picture at most, none while no bytes come.
//!
//! A container is opened when its last byte is written, as a file is:
//! FilmCraft's demuxers index a whole file when they open it.

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use ljos_engine::{Engine, ObjectId};
use ljos_platform::{Platform, Post, Reply};
use ljos_render::ImageHandle;

use crate::player;

/// What the bytes are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamFormat {
    /// JPEG after JPEG; anything between them, as the boundaries of a
    /// `multipart/x-mixed-replace` body, is skipped.
    Mjpeg,
    /// H.264 with start codes and parameter sets in the stream (Annex B).
    H264AnnexB,
    /// H.264 NAL units after their lengths, with the decoder
    /// configuration record (`avcC`) that says how long the lengths are
    /// and carries the parameter sets.
    H264Avcc { config: Vec<u8> },
    /// A file in a container FilmCraft reads (MP4, Matroska, ...), named
    /// so that its kind can be told.
    Container { name: String },
}

/// How a stream is played.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StreamOptions {
    /// Show each picture as soon as it is decoded, dropping any not yet
    /// shown: a camera, a screen. Otherwise every picture is shown, one
    /// each frame period.
    pub live: bool,
    /// Pictures a second, for a paced stream; 25 when not given.
    pub frame_rate: Option<f64>,
}

/// Why a stream could not be made or written.
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("{0:?} is no MediaPlayer")]
    NotPlayer(ObjectId),
    #[error("this build has no media decoder")]
    NoDecoder,
    #[error("no thread to decode the stream on: {0}")]
    Thread(#[from] std::io::Error),
    /// The player took another source, or was destroyed, or the stream
    /// could not be decoded.
    #[error("the stream is no longer played")]
    Closed,
}

/// How long after a write the decode thread keeps asking a threaded
/// decoder for the pictures it finished.
#[cfg(feature = "decode")]
const SETTLE: std::time::Duration = std::time::Duration::from_millis(250);

/// Pictures a paced stream decodes ahead of the one shown.
const AHEAD: usize = 4;

/// Writes of bytes the decode thread has not taken yet, before a write
/// waits.
const WRITES_AHEAD: usize = 16;

/// The writing end of a stream a player plays. Dropping it, or
/// [`MediaStream::finish`], ends the stream: the player plays what it has
/// and then reaches `EndOfMedia`.
pub struct MediaStream {
    bytes: Mutex<Option<SyncSender<Vec<u8>>>>,
    shared: Arc<Shared>,
}

impl std::fmt::Debug for MediaStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaStream").finish_non_exhaustive()
    }
}

impl MediaStream {
    /// Makes `player` (a `MediaPlayer` or `Video`) play what is written
    /// to the stream returned, in place of its source. The player is
    /// `LoadingMedia` until the first picture is decoded, then
    /// `LoadedMedia`; `play()` before then plays it when it comes, as
    /// for a file. Made on the Engine thread, written from any.
    pub fn open(
        engine: &Engine,
        player: ObjectId,
        format: StreamFormat,
        options: StreamOptions,
    ) -> Result<MediaStream, StreamError> {
        let player = player::of(engine, player).ok_or(StreamError::NotPlayer(player))?;
        Self::begin(engine, player, format, options, player::load_stream)
    }

    /// Plays the stream returned on `player` (a `MediaPlayer`), handing
    /// what it keeps of it to `load`, which answers with the generation
    /// the stream's answers must carry.
    pub(crate) fn begin(
        engine: &Engine,
        player: ObjectId,
        format: StreamFormat,
        options: StreamOptions,
        load: fn(&Engine, ObjectId, Played) -> u64,
    ) -> Result<MediaStream, StreamError> {
        if !cfg!(feature = "decode") {
            return Err(StreamError::NoDecoder);
        }
        let shared = Arc::new(Shared {
            state: Mutex::default(),
            taken: Condvar::new(),
            live: options.live,
        });
        let (tx, rx) = sync_channel(WRITES_AHEAD);
        let generation = load(
            engine,
            player,
            Played {
                shared: Arc::clone(&shared),
                live: options.live,
                period_ms: 1000.0 / options.frame_rate.filter(|r| *r > 0.0).unwrap_or(25.0),
                next_at: None,
                shown: 0,
            },
        );
        // The first thing the stream tells, a picture, its end or what
        // went wrong, is promised.
        shared.lock().promise = Some(Platform::of(engine).expect());
        let post = Platform::of(engine).poster();
        let wake = Wake {
            shared: Arc::clone(&shared),
            post,
            player,
            generation,
        };
        std::thread::Builder::new()
            .name("ljos-media-stream".to_owned())
            .spawn(move || decode(format, &rx, &wake))
            .inspect_err(|_| drop(shared.lock().promise.take()))?;
        Ok(MediaStream {
            bytes: Mutex::new(Some(tx)),
            shared,
        })
    }

    /// Hands `bytes` to the decoder. Waits while a paced stream is as
    /// far ahead of the player as it may be.
    pub fn write(&self, bytes: &[u8]) -> Result<(), StreamError> {
        let sender = lock(&self.bytes).clone().ok_or(StreamError::Closed)?;
        if self.shared.lock().closed {
            return Err(StreamError::Closed);
        }
        sender.send(bytes.to_vec()).map_err(|_| StreamError::Closed)
    }

    /// Ends the stream: nothing more is written.
    pub fn finish(&self) {
        lock(&self.bytes).take();
    }

    /// The player no longer plays it.
    pub fn is_closed(&self) -> bool {
        self.shared.lock().closed
    }
}

impl Drop for MediaStream {
    fn drop(&mut self) {
        self.finish();
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the decode thread and the player share.
pub(crate) struct Shared {
    state: Mutex<State>,
    /// A picture was taken, or the player let the stream go.
    taken: Condvar,
    live: bool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }
}

#[derive(Default)]
struct State {
    pictures: VecDeque<ImageHandle>,
    /// The decoder is done: every picture there is, is in `pictures`.
    ended: bool,
    error: Option<String>,
    /// A job is on its way to the player.
    posted: bool,
    /// The player let the stream go.
    closed: bool,
    /// The player waits for what the decoder has next, as a platform
    /// answer: a test that lets time pass waits for it, as for a fetch.
    /// Held while the player has nothing to show and waits for the
    /// decode thread, which answers it when it tells the player next.
    promise: Option<Reply>,
    /// A container, read whole: opened on the Engine thread's side.
    #[cfg(feature = "decode")]
    opened: Option<Result<crate::open::Opened, crate::open::OpenError>>,
}

/// What a player keeps of the stream it plays.
pub(crate) struct Played {
    shared: Arc<Shared>,
    live: bool,
    period_ms: f64,
    /// When a paced stream's next picture is due.
    next_at: Option<ljos_runtime::Time>,
    /// Pictures shown since the stream began.
    shown: u64,
}

impl Drop for Played {
    fn drop(&mut self) {
        let mut s = self.shared.lock();
        s.closed = true;
        let promise = s.promise.take();
        drop(s);
        drop(promise);
        self.shared.taken.notify_all();
    }
}

/// What the player finds when it looks at its stream.
pub(crate) enum Look {
    /// A picture to show now.
    Show(ImageHandle),
    /// A paced picture is waiting for its time, which is in this long.
    Due(std::time::Duration),
    /// Nothing yet.
    Wait,
    /// Every picture was shown.
    Ended,
    Failed(String),
}

impl Played {
    /// The pictures are there to know the stream by: the first, or the
    /// end, or what went wrong.
    pub(crate) fn first(&self) -> Option<Result<(u32, u32), String>> {
        let s = self.shared.lock();
        if let Some(err) = &s.error {
            return Some(Err(err.clone()));
        }
        if let Some(p) = s.pictures.front() {
            return Some(Ok((p.width(), p.height())));
        }
        s.ended.then_some(Ok((0, 0)))
    }

    #[cfg(feature = "decode")]
    pub(crate) fn take_opened(
        &self,
    ) -> Option<Result<crate::open::Opened, crate::open::OpenError>> {
        self.shared.lock().opened.take()
    }

    /// The next thing to do for a playing player at `now`.
    pub(crate) fn look(&mut self, now: ljos_runtime::Time) -> Look {
        let mut s = self.shared.lock();
        if let Some(err) = s.error.take() {
            return Look::Failed(err);
        }
        if self.live {
            // The newest only: the rest were passed by.
            let newest = s.pictures.pop_back();
            s.pictures.clear();
            drop(s);
            self.shared.taken.notify_all();
            return match newest {
                Some(p) => {
                    self.shown += 1;
                    Look::Show(p)
                }
                None if self.ended() => Look::Ended,
                None => Look::Wait,
            };
        }
        if s.pictures.is_empty() {
            if !s.ended {
                return Look::Wait;
            }
            // The last picture is shown for its period too.
            return match self.next_at {
                Some(at) if at > now => Look::Due(at.saturating_since(now)),
                _ => Look::Ended,
            };
        }
        let at = *self.next_at.get_or_insert(now);
        if at > now {
            return Look::Due(at.saturating_since(now));
        }
        let Some(p) = s.pictures.pop_front() else {
            return Look::Wait;
        };
        drop(s);
        self.shared.taken.notify_all();
        self.shown += 1;
        // A picture late by more than a period puts the rest off, rather
        // than hurrying them after it.
        let period = std::time::Duration::from_secs_f64(self.period_ms / 1000.0);
        self.next_at = Some(if now.saturating_since(at) > period {
            now + period
        } else {
            at + period
        });
        Look::Show(p)
    }

    fn ended(&self) -> bool {
        let s = self.shared.lock();
        s.ended && s.pictures.is_empty()
    }

    /// The player stopped or paused: the next picture is due when it
    /// plays again.
    pub(crate) fn halted(&mut self) {
        self.next_at = None;
    }

    /// Where a paced stream is: the time of the pictures shown.
    pub(crate) fn position(&self) -> f64 {
        if self.live {
            0.0
        } else {
            (self.shown as f64 * self.period_ms).round()
        }
    }

    /// The decode thread may tell the player of what comes next.
    pub(crate) fn rearm(&self) {
        self.shared.lock().posted = false;
    }

    /// A playing paced stream has no picture yet: the next is on its
    /// way unless the stream ended. A live stream is waited for by no
    /// one, as a camera may never send another.
    pub(crate) fn expect(&self, e: &Engine) {
        let mut s = self.shared.lock();
        if !self.live && !s.ended && s.pictures.is_empty() && s.promise.is_none() {
            s.promise = Some(Platform::of(e).expect());
        }
    }
}

/// How the decode thread tells the player.
struct Wake {
    shared: Arc<Shared>,
    post: Post,
    player: ObjectId,
    generation: u64,
}

impl Wake {
    /// Tells the player there is something to look at, unless it was
    /// told already and has not looked.
    fn wake(&self) {
        let mut s = self.shared.lock();
        let first = !std::mem::replace(&mut s.posted, true);
        let promise = s.promise.take();
        drop(s);
        let (me, generation) = (self.player, self.generation);
        let job = move |e: &Engine| player::stream_woke(e, me, generation);
        // A promise is answered even when the player was told already:
        // it may have looked since, and one look more does no harm.
        match promise {
            Some(promise) => promise.send(job),
            None if first => self.post.send(job),
            None => {}
        }
    }

    /// Hands over a picture; false when the player let the stream go.
    fn picture(&self, p: ImageHandle) -> bool {
        let mut s = self.shared.lock();
        if !self.shared.live {
            while s.pictures.len() >= AHEAD && !s.closed {
                s = self
                    .shared
                    .taken
                    .wait(s)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        }
        if s.closed {
            return false;
        }
        if self.shared.live {
            // One not yet taken was passed by.
            s.pictures.clear();
        }
        s.pictures.push_back(p);
        drop(s);
        self.wake();
        true
    }

    fn end(&self, error: Option<String>) {
        let mut s = self.shared.lock();
        s.ended = true;
        s.error = error;
        drop(s);
        self.wake();
    }

    fn closed(&self) -> bool {
        self.shared.lock().closed
    }
}

#[cfg(not(feature = "decode"))]
fn decode(_: StreamFormat, _: &Receiver<Vec<u8>>, wake: &Wake) {
    wake.end(Some("This build of Ljós has no media decoder".to_owned()));
}

#[cfg(feature = "decode")]
fn decode(format: StreamFormat, bytes: &Receiver<Vec<u8>>, wake: &Wake) {
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::Duration;

    use crate::frame_input::VideoFrame;
    use crate::stream_decoder::{DecodeError, Decoder};

    if let StreamFormat::Container { name } = &format {
        return container(name, bytes, wake);
    }
    let mut decoder = match Decoder::new(&format) {
        Ok(d) => d,
        Err(err) => return wake.end(Some(err.to_string())),
    };
    // False when the player let the stream go or it cannot be decoded.
    // A picture that cannot be converted is a format error, as one that
    // cannot be decoded is.
    let give = |got: Result<Vec<VideoFrame>, DecodeError>| {
        let fail = |err: String| {
            wake.end(Some(err));
            false
        };
        let frames = match got {
            Ok(frames) => frames,
            Err(err) => return fail(err.to_string()),
        };
        for frame in frames {
            match frame.into_image() {
                Ok(image) => {
                    if !wake.picture(image) {
                        return false;
                    }
                }
                Err(err) => return fail(err.to_string()),
            }
        }
        true
    };
    // FilmCraft's H.264 decoder works on threads of its own and hands out
    // a finished picture only when it is next called. After a write it is
    // asked again with nothing to decode, at growing intervals, until
    // `SETTLE` has passed, so that the last pictures written are not held
    // until the next write; then the thread sleeps until bytes come.
    let mut settle: Option<(Duration, Duration)> = None;
    // Until the writer ends the stream or the player lets it go.
    loop {
        let chunk = match settle {
            Some((wait, _)) => match bytes.recv_timeout(wait) {
                Ok(chunk) => Some(chunk),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match bytes.recv() {
                Ok(chunk) => Some(chunk),
                Err(_) => break,
            },
        };
        let Some(chunk) = chunk else {
            if !give(decoder.poll()) {
                return;
            }
            settle = settle.and_then(|(wait, spent)| {
                let spent = spent + wait;
                (spent < SETTLE).then_some((wait * 2, spent))
            });
            continue;
        };
        if !give(decoder.push(&chunk)) {
            return;
        }
        if decoder.finishes_later() {
            settle = Some((Duration::from_millis(1), Duration::ZERO));
        }
        if wake.closed() {
            return;
        }
    }
    if give(decoder.finish()) {
        wake.end(None);
    }
}

/// A container is opened whole once it has all been written.
#[cfg(feature = "decode")]
fn container(name: &str, bytes: &Receiver<Vec<u8>>, wake: &Wake) {
    let mut all = Vec::new();
    while let Ok(chunk) = bytes.recv() {
        if wake.closed() {
            return;
        }
        all.extend(chunk);
    }
    let opened = crate::open::from_bytes(name, all, None);
    wake.shared.lock().opened = Some(opened);
    wake.wake();
}
