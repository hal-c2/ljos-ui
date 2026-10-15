//! Where sound goes: the system's default output through cpal, or a null
//! sink. A voice is one sound playing (a `SoundEffect`'s clip, a
//! `MediaPlayer`'s stream). The device's callback mixes the voices and
//! counts the frames each has played, which is the media clock while a
//! device plays (ARCHITECTURE.md §7.6).
//!
//! The stream runs only while a voice is in it: the last voice to go
//! pauses it, so a silent application leaves the device alone.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use ljos_engine::Engine;

/// What the device plays, in its own rate and channel count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Format {
    pub rate: u32,
    pub channels: u16,
}

/// One sound playing.
#[derive(Debug)]
pub(crate) struct Voice {
    gain: AtomicU32,
    /// Frames played (silence while starved not counted).
    played: AtomicU64,
    done: AtomicBool,
    data: Mutex<VoiceData>,
}

#[derive(Debug)]
enum VoiceData {
    /// A whole clip, played `loops` more times after this one (`-1`:
    /// for ever).
    Clip {
        samples: Arc<[f32]>,
        at: usize,
        loops: i32,
    },
    /// What a decoder feeds, interleaved.
    Stream { ring: VecDeque<f32> },
}

/// How much a stream voice holds before its feeder waits, in seconds.
pub(crate) const STREAM_AHEAD: f64 = 0.4;

impl Voice {
    pub(crate) fn clip(samples: Arc<[f32]>, loops: i32, gain: f32) -> Arc<Voice> {
        Arc::new(Voice {
            gain: AtomicU32::new(gain.to_bits()),
            played: AtomicU64::new(0),
            done: AtomicBool::new(false),
            data: Mutex::new(VoiceData::Clip {
                samples,
                at: 0,
                loops,
            }),
        })
    }

    pub(crate) fn stream(gain: f32) -> Arc<Voice> {
        Arc::new(Voice {
            gain: AtomicU32::new(gain.to_bits()),
            played: AtomicU64::new(0),
            done: AtomicBool::new(false),
            data: Mutex::new(VoiceData::Stream {
                ring: VecDeque::new(),
            }),
        })
    }

    pub(crate) fn set_gain(&self, gain: f32) {
        self.gain.store(gain.to_bits(), Ordering::Relaxed);
    }

    /// Frames the device has played of this voice.
    pub(crate) fn played(&self) -> u64 {
        self.played.load(Ordering::Relaxed)
    }

    pub(crate) fn stop(&self) {
        self.done.store(true, Ordering::Relaxed);
    }

    pub(crate) fn is_done(&self) -> bool {
        self.done.load(Ordering::Relaxed)
    }

    /// Samples (not frames) waiting in a stream voice.
    pub(crate) fn queued(&self) -> usize {
        match &*self.data.lock().unwrap_or_else(PoisonError::into_inner) {
            VoiceData::Stream { ring } => ring.len(),
            VoiceData::Clip { .. } => 0,
        }
    }

    pub(crate) fn push(&self, interleaved: &[f32]) {
        if let VoiceData::Stream { ring } =
            &mut *self.data.lock().unwrap_or_else(PoisonError::into_inner)
        {
            ring.extend(interleaved.iter().copied());
        }
    }

    /// Adds this voice into `out` (interleaved, `channels` wide). Never
    /// waits: a voice its feeder holds is skipped for this buffer.
    fn mix_into(&self, out: &mut [f32], channels: usize) {
        if self.is_done() {
            return;
        }
        let Ok(mut data) = self.data.try_lock() else {
            return;
        };
        let gain = f32::from_bits(self.gain.load(Ordering::Relaxed));
        let mut frames = 0u64;
        match &mut *data {
            VoiceData::Clip { samples, at, loops } => {
                for frame in out.chunks_mut(channels) {
                    if *at >= samples.len() {
                        if *loops == 0 {
                            self.done.store(true, Ordering::Relaxed);
                            break;
                        }
                        if *loops > 0 {
                            *loops -= 1;
                        }
                        *at = 0;
                    }
                    for s in frame.iter_mut() {
                        *s += samples.get(*at).copied().unwrap_or(0.0) * gain;
                        *at += 1;
                    }
                    frames += 1;
                }
            }
            VoiceData::Stream { ring } => {
                for frame in out.chunks_mut(channels) {
                    if ring.len() < channels {
                        break;
                    }
                    for s in frame.iter_mut() {
                        *s += ring.pop_front().unwrap_or(0.0) * gain;
                    }
                    frames += 1;
                }
            }
        }
        self.played.fetch_add(frames, Ordering::Relaxed);
    }
}

/// The voices one device mixes.
#[derive(Debug, Default)]
struct Mixer {
    voices: Mutex<Vec<Arc<Voice>>>,
}

impl Mixer {
    fn fill(&self, out: &mut [f32], channels: usize) {
        out.fill(0.0);
        let Ok(mut voices) = self.voices.try_lock() else {
            return;
        };
        voices.retain(|v| !v.is_done());
        for v in voices.iter() {
            v.mix_into(out, channels);
        }
    }
}

/// The engine's sound output: an extension, one per engine.
#[derive(Default)]
pub(crate) struct Audio {
    null: Cell<bool>,
    device: RefCell<Device>,
}

#[derive(Default)]
enum Device {
    #[default]
    Untried,
    Absent,
    #[cfg(feature = "audio")]
    Open(cpal_device::Open),
}

/// What `MediaDevices` reports of the output.
#[derive(Clone, Debug, Default)]
pub(crate) struct Description {
    pub id: String,
    pub description: String,
}

impl Audio {
    pub(crate) fn of(engine: &Engine) -> Rc<Audio> {
        engine.extension::<Audio>()
    }

    pub(crate) fn force_null(&self) {
        self.null.set(true);
        *self.device.borrow_mut() = Device::Absent;
    }

    /// The device's format, opening it the first time; `None` for the null
    /// sink.
    pub(crate) fn format(&self) -> Option<Format> {
        if self.null.get() {
            return None;
        }
        self.open();
        match &*self.device.borrow() {
            #[cfg(feature = "audio")]
            Device::Open(open) => Some(open.format),
            _ => None,
        }
    }

    /// The default output, as the system names it; `None` without one.
    pub(crate) fn describe(&self) -> Option<Description> {
        if self.null.get() {
            return None;
        }
        #[cfg(feature = "audio")]
        {
            cpal_device::describe()
        }
        #[cfg(not(feature = "audio"))]
        {
            None
        }
    }

    /// Starts `voice` on the device. With the null sink nothing happens:
    /// the caller's clock is the engine's.
    pub(crate) fn play(&self, voice: &Arc<Voice>) {
        #[cfg(feature = "audio")]
        if let Device::Open(open) = &*self.device.borrow() {
            open.add(Arc::clone(voice));
        }
        #[cfg(not(feature = "audio"))]
        let _ = voice;
    }

    /// Ends `voice`, pausing the stream when it was the last.
    pub(crate) fn remove(&self, voice: &Arc<Voice>) {
        voice.stop();
        #[cfg(feature = "audio")]
        if let Device::Open(open) = &*self.device.borrow() {
            open.prune();
        }
    }

    fn open(&self) {
        if !matches!(*self.device.borrow(), Device::Untried) {
            return;
        }
        #[cfg(feature = "audio")]
        let device = match cpal_device::Open::default_output() {
            Some(open) => Device::Open(open),
            None => Device::Absent,
        };
        #[cfg(not(feature = "audio"))]
        let device = Device::Absent;
        *self.device.borrow_mut() = device;
    }
}

#[cfg(feature = "audio")]
mod cpal_device {
    use std::cell::Cell;
    use std::sync::{Arc, PoisonError};

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    use super::{Description, Format, Mixer, Voice};

    /// An output stream on the default device, and what it mixes.
    pub(super) struct Open {
        pub format: Format,
        mixer: Arc<Mixer>,
        stream: cpal::Stream,
        running: Cell<bool>,
    }

    pub(super) fn describe() -> Option<Description> {
        let device = cpal::default_host().default_output_device()?;
        let description = device
            .description()
            .map(|d| d.name().to_owned())
            .unwrap_or_default();
        let id = device
            .id()
            .map(|id| id.to_string())
            .unwrap_or_else(|_| description.clone());
        Some(Description { id, description })
    }

    impl Open {
        /// The default device, with a stream built and paused; `None` when
        /// there is none or it refuses.
        pub(super) fn default_output() -> Option<Open> {
            let device = cpal::default_host().default_output_device()?;
            let supported = device.default_output_config().ok()?;
            let config = supported.config();
            let format = Format {
                rate: config.sample_rate,
                channels: config.channels.max(1),
            };
            let mixer = Arc::new(Mixer::default());
            let feed = Arc::clone(&mixer);
            let channels = usize::from(format.channels);
            let stream = device
                .build_output_stream::<f32, _, _>(
                    config,
                    move |out: &mut [f32], _| feed.fill(out, channels),
                    |_err| {},
                    None,
                )
                .ok()?;
            // Built streams may start on their own; this one waits for a
            // voice.
            let _ = stream.pause();
            Some(Open {
                format,
                mixer,
                stream,
                running: Cell::new(false),
            })
        }

        pub(super) fn add(&self, voice: Arc<Voice>) {
            self.mixer
                .voices
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(voice);
            if !self.running.get() && self.stream.play().is_ok() {
                self.running.set(true);
            }
        }

        pub(super) fn prune(&self) {
            let empty = {
                let mut voices = self
                    .mixer
                    .voices
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                voices.retain(|v| !v.is_done());
                voices.is_empty()
            };
            if empty && self.running.get() && self.stream.pause().is_ok() {
                self.running.set(false);
            }
        }
    }
}

/// Interleaves `channels` (one buffer per channel) for a device with
/// `out` channels: mono goes to every channel, more than the device has
/// are dropped, fewer are filled with the last.
pub(crate) fn interleave(channels: &[Vec<f32>], out: u16) -> Vec<f32> {
    let frames = channels.first().map_or(0, Vec::len);
    let out = usize::from(out.max(1));
    let mut v = Vec::with_capacity(frames * out);
    for i in 0..frames {
        for c in 0..out {
            let src = channels.get(c).or_else(|| channels.last());
            v.push(src.and_then(|s| s.get(i)).copied().unwrap_or(0.0));
        }
    }
    v
}
