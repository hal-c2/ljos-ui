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
use std::sync::{Arc, Condvar, Mutex, PoisonError, mpsc};

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

// Only a device mixes; the null sink counts no frames.
#[cfg_attr(not(feature = "audio"), allow(dead_code))]
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
    #[cfg_attr(not(feature = "audio"), allow(dead_code))]
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
#[cfg_attr(not(feature = "audio"), allow(dead_code))]
#[derive(Debug, Default)]
pub(crate) struct Mixer {
    voices: Mutex<Vec<Arc<Voice>>>,
}

#[cfg_attr(not(feature = "audio"), allow(dead_code))]
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
///
/// The device is opened on a thread of its own, which keeps the stream
/// for as long as the engine has it: opening asks the sound server and
/// takes milliseconds, which the user interface would show as a hitch the
/// first time something makes a sound. A decoder that needs the device's
/// format waits for it on its own thread.
pub(crate) struct Audio {
    null: Cell<bool>,
    opener: Option<Opener>,
    device: RefCell<Device>,
}

impl Default for Audio {
    fn default() -> Self {
        #[cfg(feature = "audio")]
        let opener: Option<Opener> = Some(cpal_device::default_output);
        #[cfg(not(feature = "audio"))]
        let opener = None;
        Audio {
            null: Cell::new(false),
            opener,
            device: RefCell::default(),
        }
    }
}

/// Opens the default output, mixing what `Mixer` holds, on the audio
/// thread: its format, and what plays and pauses it there.
pub(crate) type Opener = fn(Arc<Mixer>) -> Option<(Format, Box<dyn Playback>)>;

/// A stream the audio thread owns.
pub(crate) trait Playback {
    fn play(&self);
    fn pause(&self);
}

#[derive(Default)]
enum Device {
    #[default]
    Untried,
    Absent,
    Output(Output),
}

/// The device as the Engine thread sees it, open or still opening.
struct Output {
    told: Soon,
    mixer: Arc<Mixer>,
    commands: mpsc::Sender<Command>,
    running: Cell<bool>,
}

enum Command {
    Play,
    Pause,
}

/// The device's format, once the audio thread has tried to open it:
/// `None` when there is no device.
#[derive(Clone, Default)]
pub(crate) struct Soon(Arc<(Mutex<Option<Option<Format>>>, Condvar)>);

impl Soon {
    fn tell(&self, format: Option<Format>) {
        let (slot, ready) = &*self.0;
        *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(format);
        ready.notify_all();
    }

    /// The format, waiting for the device to be tried.
    pub(crate) fn wait(&self) -> Option<Format> {
        let (slot, ready) = &*self.0;
        let mut told = slot.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(format) = *told {
                return format;
            }
            told = ready.wait(told).unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// The format if the device has been tried.
    fn known(&self) -> Option<Format> {
        let (slot, _) = &*self.0;
        slot.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .flatten()
    }
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

    /// The device's format, to be known off the Engine thread; `None` for
    /// the null sink. Starts opening the device the first time.
    pub(crate) fn soon(&self) -> Option<Soon> {
        self.open();
        match &*self.device.borrow() {
            Device::Output(out) => Some(out.told.clone()),
            _ => None,
        }
    }

    /// The device's format, waiting for it to open the first time; `None`
    /// for the null sink.
    pub(crate) fn format(&self) -> Option<Format> {
        self.soon().and_then(|s| s.wait())
    }

    /// The device's format if it is open, never waiting.
    pub(crate) fn known(&self) -> Option<Format> {
        match &*self.device.borrow() {
            Device::Output(out) => out.told.known(),
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
        if let Device::Output(out) = &*self.device.borrow() {
            out.mixer
                .voices
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(Arc::clone(voice));
            if !out.running.get() && out.commands.send(Command::Play).is_ok() {
                out.running.set(true);
            }
        }
    }

    /// Ends `voice`, pausing the stream when it was the last.
    pub(crate) fn remove(&self, voice: &Arc<Voice>) {
        voice.stop();
        if let Device::Output(out) = &*self.device.borrow() {
            let empty = {
                let mut voices = out
                    .mixer
                    .voices
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                voices.retain(|v| !v.is_done());
                voices.is_empty()
            };
            if empty && out.running.get() && out.commands.send(Command::Pause).is_ok() {
                out.running.set(false);
            }
        }
    }

    fn open(&self) {
        if !matches!(*self.device.borrow(), Device::Untried) {
            return;
        }
        let device = match self.opener.filter(|_| !self.null.get()) {
            Some(opener) => spawn_output(opener).map_or(Device::Absent, Device::Output),
            None => Device::Absent,
        };
        *self.device.borrow_mut() = device;
    }
}

/// Starts the audio thread: it opens the device, says its format, and
/// then plays and pauses it as told until the engine lets it go.
fn spawn_output(opener: Opener) -> Option<Output> {
    let told = Soon::default();
    let mixer = Arc::new(Mixer::default());
    let (commands, heard) = mpsc::channel();
    let (tell, feed) = (told.clone(), Arc::clone(&mixer));
    std::thread::Builder::new()
        .name("ljos-audio".to_owned())
        .spawn(move || {
            let opened = opener(feed);
            tell.tell(opened.as_ref().map(|(format, _)| *format));
            let Some((_, stream)) = opened else {
                return;
            };
            while let Ok(command) = heard.recv() {
                match command {
                    Command::Play => stream.play(),
                    Command::Pause => stream.pause(),
                }
            }
        })
        .ok()?;
    Some(Output {
        told,
        mixer,
        commands,
        running: Cell::new(false),
    })
}

#[cfg(feature = "audio")]
mod cpal_device {
    use std::sync::Arc;

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    use super::{Description, Format, Mixer, Playback};

    impl Playback for cpal::Stream {
        fn play(&self) {
            let _ = StreamTrait::play(self);
        }

        fn pause(&self) {
            let _ = StreamTrait::pause(self);
        }
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

    /// The default device, with a stream built and paused; `None` when
    /// there is none or it refuses.
    pub(super) fn default_output(mixer: Arc<Mixer>) -> Option<(Format, Box<dyn Playback>)> {
        let device = cpal::default_host().default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let config = supported.config();
        let format = Format {
            rate: config.sample_rate,
            channels: config.channels.max(1),
        };
        let channels = usize::from(format.channels);
        let stream = device
            .build_output_stream::<f32, _, _>(
                config,
                move |out: &mut [f32], _| mixer.fill(out, channels),
                |_err| {},
                None,
            )
            .ok()?;
        // Built streams may start on their own; this one waits for a
        // voice.
        let _ = StreamTrait::pause(&stream);
        Some((format, Box::new(stream)))
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    /// Held shut until the test has seen the Engine thread go on.
    static GATE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());
    static OPENED_ON: Mutex<Option<String>> = Mutex::new(None);
    static PLAYS: AtomicUsize = AtomicUsize::new(0);
    static PAUSES: AtomicUsize = AtomicUsize::new(0);

    struct Counted;

    impl Playback for Counted {
        fn play(&self) {
            PLAYS.fetch_add(1, Ordering::SeqCst);
        }

        fn pause(&self) {
            PAUSES.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn slow_device(_: Arc<Mixer>) -> Option<(Format, Box<dyn Playback>)> {
        let (open, shut) = &GATE;
        let mut open = open.lock().unwrap();
        while !*open {
            open = shut.wait(open).unwrap();
        }
        *OPENED_ON.lock().unwrap() = std::thread::current().name().map(str::to_owned);
        let format = Format {
            rate: 48_000,
            channels: 2,
        };
        Some((format, Box::new(Counted)))
    }

    #[test]
    fn the_device_is_opened_on_a_thread_of_its_own() {
        let audio = Audio {
            null: Cell::new(false),
            opener: Some(slow_device),
            device: RefCell::default(),
        };
        // The device cannot open yet, and asking for its format does not
        // wait for it.
        let soon = audio.soon().expect("a device is being opened");
        assert_eq!(audio.known(), None);
        let voice = Voice::clip(Arc::from([0.0; 4]), 0, 1.0);
        audio.play(&voice);

        *GATE.0.lock().unwrap() = true;
        GATE.1.notify_all();
        let format = Some(Format {
            rate: 48_000,
            channels: 2,
        });
        assert_eq!(soon.wait(), format);
        assert_eq!(audio.format(), format);
        assert_eq!(OPENED_ON.lock().unwrap().as_deref(), Some("ljos-audio"));

        // What was asked while it opened is done once it has.
        audio.remove(&voice);
        drop(audio);
        let waited = std::time::Instant::now();
        while PAUSES.load(Ordering::SeqCst) == 0 && waited.elapsed().as_secs() < 10 {
            std::thread::yield_now();
        }
        assert_eq!(PLAYS.load(Ordering::SeqCst), 1);
        assert_eq!(PAUSES.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_null_sink_opens_nothing() {
        let audio = Audio {
            null: Cell::new(true),
            opener: Some(|_| panic!("opened")),
            device: RefCell::default(),
        };
        assert!(audio.soon().is_none());
        assert_eq!(audio.format(), None);
    }
}
