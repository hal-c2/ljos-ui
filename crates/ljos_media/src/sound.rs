//! `SoundEffect`: a short sound decoded whole when its source is set, and
//! played from memory.
//!
//! What it reports was read on qml6: a source starts `Loading` and
//! becomes `Ready`, or `Error` with the warning `QSoundEffect: Error
//! decoding source <url>`; `play()` sets `loopsRemaining` to `loops` and
//! then `playing` at once; each time through the sound takes one off
//! `loopsRemaining`, and at 0 `playing` turns false. `Infinite` (-2)
//! plays until `stop()`.
//!
//! The device plays the loops itself; the engine keeps one deadline, the
//! end of the current time through, while the sound plays, and none
//! otherwise.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, LogLevel, ObjectId, Prop, QmlObject, QmlValue, QtObject, Url,
    qml_impl,
};
use ljos_runtime::TimerId;

use crate::open::{self, OpenError, Opened};
use crate::sink::{Audio, Voice};

fn sound_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        ("Loop", vec![("Infinite", INFINITE.into())]),
        (
            "Status",
            vec![("Null", 0), ("Loading", 1), ("Ready", 2), ("Error", 3)],
        ),
    ]
}

/// `SoundEffect.Infinite`.
const INFINITE: i32 = -2;

mod status {
    pub const NULL: i32 = 0;
    pub const LOADING: i32 = 1;
    pub const READY: i32 = 2;
    pub const ERROR: i32 = 3;
}

/// `SoundEffect` (QtMultimedia).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtMultimedia",
    name = "SoundEffect",
    extends = QtObject,
    enums = sound_enums
)]
pub struct SoundEffect {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub source: Url,
    /// Times to play; `SoundEffect.Infinite` for ever. Its signal is
    /// `loopCountChanged`, as in Qt.
    pub loops: i32,
    #[qml(notify, readonly)]
    pub loops_remaining: i32,
    #[qml(notify)]
    pub volume: f64,
    #[qml(notify)]
    pub muted: bool,
    #[qml(notify, readonly)]
    pub playing: bool,
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    #[qml(notify, object = "AudioDevice")]
    pub audio_device: Option<ObjectId>,
    #[qml(skip)]
    run: Run,
}

impl Default for SoundEffect {
    fn default() -> Self {
        SoundEffect {
            object: QtObject::default(),
            source: Url(Atom::EMPTY),
            loops: 1,
            loops_remaining: 0,
            volume: 1.0,
            muted: false,
            playing: false,
            status: status::NULL,
            audio_device: None,
            run: Run::default(),
        }
    }
}

#[derive(Default)]
struct Run {
    generation: u64,
    /// The length of one time through, in milliseconds.
    length: f64,
    samples: Option<Arc<[f32]>>,
    voice: Option<Arc<Voice>>,
    turn: Option<TimerId>,
    /// `play()` came while loading.
    want_play: bool,
}

impl fmt::Debug for Run {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Run")
            .field("length", &self.length)
            .field("playing", &self.turn.is_some())
            .finish_non_exhaustive()
    }
}

#[qml_impl]
impl SoundEffect {
    #[qml(signal, name = "loopCountChanged")]
    fn loop_count_changed();
    #[qml(signal, name = "loadedChanged")]
    fn loaded_changed();

    #[qml(invokable)]
    fn play(cx: &Cx) -> Result<(), EngineError> {
        play(cx.engine, cx.this);
        Ok(())
    }

    #[qml(invokable)]
    fn stop(cx: &Cx) -> Result<(), EngineError> {
        stop(cx.engine, cx.this);
        Ok(())
    }

    #[qml(invokable, name = "isLoaded")]
    fn is_loaded(&self) -> bool {
        self.status == status::READY
    }

    #[qml(invokable, name = "isMuted")]
    fn is_muted(&self) -> bool {
        self.muted
    }

    #[qml(invokable, name = "isPlaying")]
    fn is_playing(&self) -> bool {
        self.playing
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.watch(me, SoundEffect::SOURCE.index, move |e, _| load(e, me))?;
        e.watch(me, SoundEffect::LOOPS.index, move |e, _| {
            let _ = e.emit_by_name(me, "loopCountChanged", &[]);
        })?;
        e.effect_in(me, move |e| {
            let gain = gain(e, me);
            if let Some(v) = e.untrack(|| run(e, me, |r| r.voice.clone())).flatten() {
                v.set_gain(gain);
            }
        })?;
        load(e, me);
        Ok(())
    }
}

fn run<R>(e: &Engine, me: ObjectId, f: impl FnOnce(&mut Run) -> R) -> Option<R> {
    e.with_mut::<SoundEffect, _>(me, |s| f(&mut s.run)).ok()
}

fn put<T: QmlValue>(e: &Engine, me: ObjectId, p: Prop<T>, v: T) {
    let _ = e.set_internal(me, p, v);
}

fn get<T: QmlValue + Default>(e: &Engine, me: ObjectId, p: Prop<T>) -> T {
    e.untrack(|| e.get(me, p)).unwrap_or_default()
}

fn gain(e: &Engine, me: ObjectId) -> f32 {
    let muted = e.get(me, SoundEffect::MUTED).unwrap_or(false);
    let volume = e
        .get(me, SoundEffect::VOLUME)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    if muted { 0.0 } else { volume as f32 }
}

fn load(e: &Engine, me: ObjectId) {
    stop(e, me);
    let generation = run(e, me, |r| {
        r.generation += 1;
        r.samples = None;
        r.length = 0.0;
        r.generation
    })
    .unwrap_or(0);
    let source = get(e, me, SoundEffect::SOURCE).0;
    if source.is_empty() {
        set_status(e, me, status::NULL);
        return;
    }
    set_status(e, me, status::LOADING);
    let url = e.resolve_url(me, &source);
    open::start(e, me, generation, url, Audio::of(e).soon(), loaded, None);
}

// `loadedChanged` is declared and never emitted: Qt 6.11 does not emit
// it when a sound loads either, and tells it by `statusChanged`.
fn set_status(e: &Engine, me: ObjectId, s: i32) {
    put(e, me, SoundEffect::STATUS, s);
}

fn loaded(e: &Engine, me: ObjectId, generation: u64, outcome: Result<Opened, OpenError>) {
    if !e.is_alive(me) || run(e, me, |r| r.generation) != Some(generation) {
        return;
    }
    match outcome {
        Ok(o) if o.has_audio => {
            run(e, me, |r| {
                r.length = o.duration_ms;
                r.samples = o.samples;
            });
            set_status(e, me, status::READY);
            if run(e, me, |r| std::mem::take(&mut r.want_play)).unwrap_or(false) {
                play(e, me);
            }
        }
        _ => {
            let url = e.resolve_url(me, &get(e, me, SoundEffect::SOURCE).0);
            e.log(
                LogLevel::Warning,
                crate::CATEGORY,
                format!("QSoundEffect: Error decoding source {url}"),
            );
            run(e, me, |r| r.want_play = false);
            set_status(e, me, status::ERROR);
        }
    }
}

fn play(e: &Engine, me: ObjectId) {
    match get(e, me, SoundEffect::STATUS) {
        status::LOADING => {
            run(e, me, |r| r.want_play = true);
            return;
        }
        status::READY => {}
        _ => return,
    }
    // Playing again starts again.
    halt(e, me);
    let loops = get(e, me, SoundEffect::LOOPS);
    let loops = if loops == 0 { 1 } else { loops };
    put(e, me, SoundEffect::LOOPS_REMAINING, loops);
    put(e, me, SoundEffect::PLAYING, true);
    let samples = run(e, me, |r| r.samples.clone()).flatten();
    if let Some(samples) = samples {
        let more = if loops < 0 { -1 } else { loops - 1 };
        let voice = Voice::clip(samples, more, e.untrack(|| gain(e, me)));
        Audio::of(e).play(&voice);
        run(e, me, |r| r.voice = Some(voice));
    }
    arm(e, me);
}

fn arm(e: &Engine, me: ObjectId) {
    let Some((length, generation)) = run(e, me, |r| (r.length, r.generation)) else {
        return;
    };
    let wait = Duration::from_secs_f64((length / 1000.0).max(0.001));
    let id = e.call_at(e.now() + wait, move |e| turned(e, me, generation));
    run(e, me, |r| r.turn = Some(id));
}

/// One time through is over.
fn turned(e: &Engine, me: ObjectId, generation: u64) {
    if !e.is_alive(me) || run(e, me, |r| r.generation) != Some(generation) {
        return;
    }
    run(e, me, |r| r.turn = None);
    let left = get(e, me, SoundEffect::LOOPS_REMAINING);
    if left == INFINITE || left < 0 {
        arm(e, me);
        return;
    }
    let left = left - 1;
    put(e, me, SoundEffect::LOOPS_REMAINING, left.max(0));
    if left > 0 {
        arm(e, me);
        return;
    }
    halt(e, me);
    put(e, me, SoundEffect::PLAYING, false);
}

fn halt(e: &Engine, me: ObjectId) {
    let Some((turn, voice)) = run(e, me, |r| (r.turn.take(), r.voice.take())) else {
        return;
    };
    if let Some(id) = turn {
        e.cancel_call(id);
    }
    if let Some(v) = voice {
        Audio::of(e).remove(&v);
    }
}

fn stop(e: &Engine, me: ObjectId) {
    halt(e, me);
    run(e, me, |r| r.want_play = false);
    if get(e, me, SoundEffect::PLAYING) {
        put(e, me, SoundEffect::LOOPS_REMAINING, 0);
        put(e, me, SoundEffect::PLAYING, false);
    }
}

impl Drop for SoundEffect {
    fn drop(&mut self) {
        if let Some(v) = self.run.voice.take() {
            v.stop();
        }
    }
}
