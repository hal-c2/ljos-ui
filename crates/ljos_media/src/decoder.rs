//! A player's decode thread: the frame the clock asks for and the sound
//! a device plays, from one opened source.
//!
//! One frame request waits at a time. A newer one replaces it before it
//! starts, so a frame the clock has already passed is never decoded, and
//! the answer of the one replaced counts as given (its reply is dropped).
//! Frames are converted to RGBA here, off the Engine thread.
//!
//! Sound is decoded ahead of the device by [`STREAM_AHEAD`] and no more:
//! the thread sleeps while the voice has enough, waking to top it up, and
//! only while it feeds one. Without a request and a voice it waits on the
//! condition with no timeout.

use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use filmcraft_media::{FrameRequest, SharedSource};
use filmcraft_time::Tick;
use ljos_engine::Engine;
use ljos_platform::Reply;
use ljos_render::ImageHandle;

use crate::mp4_video::Mp4Video;
use crate::sink::{Format, STREAM_AHEAD, Voice, interleave};

/// What the Engine thread does with a frame: `None` when it could not
/// be decoded.
pub(crate) type Deliver = Box<dyn FnOnce(&Engine, Option<ImageHandle>) + Send>;

/// How much sound one decode call makes, in seconds.
const CHUNK: f64 = 0.1;

/// How often a feeding thread looks at its voice.
const FEED_WAIT: Duration = Duration::from_millis(40);

pub(crate) struct Decoder {
    shared: Arc<Shared>,
}

struct Shared {
    job: Mutex<Job>,
    wake: Condvar,
}

#[derive(Default)]
struct Job {
    frame: Option<FrameJob>,
    feed: Option<Feed>,
    /// Bumped by every new feed, so a chunk decoded for an old one is
    /// thrown away.
    feed_serial: u64,
    quit: bool,
}

struct FrameJob {
    at_ms: f64,
    reply: Reply,
    deliver: Deliver,
}

struct Feed {
    voice: Arc<Voice>,
    /// The next frame to decode and the last, at `decode_rate`.
    next: i64,
    end: i64,
    /// The rate decoded at: the device's divided by the playback rate, so
    /// the device plays it faster or slower.
    decode_rate: u32,
    channels: u16,
    /// Samples (all channels) the voice may hold.
    ahead: usize,
}

impl Decoder {
    /// A thread for `source`, its pictures read by `video` when there is
    /// one; `None` when none could be started.
    pub(crate) fn new(source: SharedSource, video: Option<Mp4Video>) -> Option<Decoder> {
        let shared = Arc::new(Shared {
            job: Mutex::new(Job::default()),
            wake: Condvar::new(),
        });
        let worker = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("ljos-media-decode".to_owned())
            .spawn(move || work(&worker, &source, video))
            .ok()?;
        Some(Decoder { shared })
    }

    fn with(&self, f: impl FnOnce(&mut Job)) {
        f(&mut self
            .shared
            .job
            .lock()
            .unwrap_or_else(PoisonError::into_inner));
        self.shared.wake.notify_one();
    }

    /// Asks for the frame shown at `at_ms`, replacing a request not yet
    /// started.
    pub(crate) fn frame(&self, at_ms: f64, reply: Reply, deliver: Deliver) {
        self.with(|j| {
            j.frame = Some(FrameJob {
                at_ms,
                reply,
                deliver,
            });
        });
    }

    /// Feeds `voice` from `from_ms` to `to_ms` for a device of `format`,
    /// at playback `rate`.
    pub(crate) fn feed(
        &self,
        voice: Arc<Voice>,
        from_ms: f64,
        to_ms: f64,
        format: Format,
        rate: f64,
    ) {
        let decode_rate = (f64::from(format.rate) / rate)
            .round()
            .clamp(1000.0, 768_000.0) as u32;
        let at = |ms: f64| (ms / 1000.0 * f64::from(decode_rate)).round() as i64;
        let ahead = (STREAM_AHEAD * f64::from(format.rate)) as usize * usize::from(format.channels);
        self.with(|j| {
            j.feed_serial += 1;
            j.feed = Some(Feed {
                voice,
                next: at(from_ms),
                end: at(to_ms),
                decode_rate,
                channels: format.channels,
                ahead,
            });
        });
    }

    pub(crate) fn stop_feed(&self) {
        self.with(|j| {
            j.feed_serial += 1;
            j.feed = None;
        });
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // The thread ends at its next look; nothing waits for it.
        self.with(|j| {
            j.quit = true;
            j.frame = None;
            j.feed = None;
        });
    }
}

/// A chunk of sound to make, taken out of the job.
struct Chunk {
    voice: Arc<Voice>,
    start: i64,
    frames: usize,
    decode_rate: u32,
    channels: u16,
    serial: u64,
}

fn work(shared: &Shared, source: &SharedSource, mut video: Option<Mp4Video>) {
    loop {
        let (frame, chunk) = {
            let mut job = shared.job.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                if job.quit {
                    return;
                }
                if job
                    .feed
                    .as_ref()
                    .is_some_and(|f| f.voice.is_done() || f.next >= f.end)
                {
                    job.feed = None;
                }
                let hungry = job
                    .feed
                    .as_ref()
                    .is_some_and(|f| f.voice.queued() < f.ahead);
                if job.frame.is_some() || hungry {
                    break;
                }
                job = if job.feed.is_some() {
                    shared
                        .wake
                        .wait_timeout(job, FEED_WAIT)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                } else {
                    shared
                        .wake
                        .wait(job)
                        .unwrap_or_else(PoisonError::into_inner)
                };
            }
            let serial = job.feed_serial;
            let chunk = job.feed.as_mut().and_then(|f| {
                if f.voice.queued() >= f.ahead {
                    return None;
                }
                let frames = ((CHUNK * f64::from(f.decode_rate)) as i64).min(f.end - f.next);
                let c = Chunk {
                    voice: Arc::clone(&f.voice),
                    start: f.next,
                    frames: usize::try_from(frames).unwrap_or(0),
                    decode_rate: f.decode_rate,
                    channels: f.channels,
                    serial,
                };
                f.next += frames;
                Some(c)
            });
            (job.frame.take(), chunk)
        };
        if let Some(f) = frame {
            let image = decode_frame(source, video.as_mut(), f.at_ms);
            let deliver = f.deliver;
            f.reply.send(move |e| deliver(e, image));
        }
        if let Some(c) = chunk {
            let Ok(buffer) = source.audio(c.start, c.frames, c.decode_rate) else {
                let mut job = shared.job.lock().unwrap_or_else(PoisonError::into_inner);
                if job.feed_serial == c.serial {
                    job.feed = None;
                }
                continue;
            };
            let samples = interleave(&buffer.channels, c.channels);
            let job = shared.job.lock().unwrap_or_else(PoisonError::into_inner);
            if job.feed_serial == c.serial {
                c.voice.push(&samples);
            }
        }
    }
}

fn decode_frame(
    source: &SharedSource,
    video: Option<&mut Mp4Video>,
    at_ms: f64,
) -> Option<ImageHandle> {
    let at = Tick::from_seconds_f64(at_ms / 1000.0);
    // The source's cache would keep every frame shown (see `mp4_video`).
    let frame = match video {
        Some(v) => v.frame(at)?,
        None => Arc::unwrap_or_clone(source.video_frame(FrameRequest::full(at)).ok()?),
    };
    let rgba = frame.to_rgba8();
    ImageHandle::from_straight(frame.width, frame.height, rgba).ok()
}
