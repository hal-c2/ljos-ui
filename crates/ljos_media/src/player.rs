//! `MediaPlayer`: a source opened, its clock, and what it reports.
//!
//! The order of what a player says was read on qml6 (Qt 6.11, FFmpeg
//! backend) and is kept here: loading reports the duration, the tracks,
//! the metadata, `seekable`, `hasAudio` and `hasVideo` before
//! `LoadedMedia`; `play()` turns `playing` true before `playbackState`,
//! then `mediaStatus` goes to `BufferingMedia` and, from the start of a
//! loaded file, `BufferedMedia`; a source that cannot be opened reports
//! `errorOccurred` and `errorChanged` before `InvalidMedia`.
//!
//! The clock is the sound device's while one plays the player's sound
//! (frames played, `sink::Voice::played`), and the engine's clock
//! otherwise. Nothing runs while the player is not playing: the next
//! position report, the next frame and the end of the stream are
//! deadlines on the engine's timers, armed by `play()` and dropped by
//! `pause()` and `stop()`.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, LogLevel, ObjectId, Prop, QmlObject, QmlValue, QtObject, Url,
    Value, qml_impl,
};
use ljos_runtime::{Time, TimerId};

use crate::devices::gain_of;
use crate::meta::{Entry, MediaMetaData};
use crate::open::{self, OpenError, Opened, VideoInfo};
use crate::sink::{Audio, Voice};
use crate::video;

fn player_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "PlaybackState",
            vec![("StoppedState", 0), ("PlayingState", 1), ("PausedState", 2)],
        ),
        (
            "MediaStatus",
            vec![
                ("NoMedia", 0),
                ("LoadingMedia", 1),
                ("LoadedMedia", 2),
                ("StalledMedia", 3),
                ("BufferingMedia", 4),
                ("BufferedMedia", 5),
                ("EndOfMedia", 6),
                ("InvalidMedia", 7),
            ],
        ),
        (
            "Error",
            vec![
                ("NoError", 0),
                ("ResourceError", 1),
                ("FormatError", 2),
                ("NetworkError", 3),
                ("AccessDeniedError", 4),
            ],
        ),
        ("Loops", vec![("Infinite", -1), ("Once", 1)]),
        (
            "PitchCompensationAvailability",
            vec![("AlwaysOn", 0), ("Available", 1), ("Unavailable", 2)],
        ),
    ]
}

/// `MediaPlayer.PlaybackState`.
pub(crate) mod state {
    pub const STOPPED: i32 = 0;
    pub const PLAYING: i32 = 1;
    pub const PAUSED: i32 = 2;
}

/// `MediaPlayer.MediaStatus`.
pub(crate) mod status {
    pub const NO_MEDIA: i32 = 0;
    pub const LOADING: i32 = 1;
    pub const LOADED: i32 = 2;
    pub const BUFFERING: i32 = 4;
    pub const BUFFERED: i32 = 5;
    pub const END: i32 = 6;
    pub const INVALID: i32 = 7;
}

/// How often a playing player reports its position, as Qt's FFmpeg
/// backend does.
const POSITION_INTERVAL: Duration = Duration::from_millis(50);

/// `MediaPlayer` (QtMultimedia).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtMultimedia",
    name = "MediaPlayer",
    extends = QtObject,
    enums = player_enums
)]
pub struct MediaPlayer {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub source: Url,
    #[qml(notify)]
    pub auto_play: bool,
    /// Milliseconds.
    #[qml(notify, readonly)]
    pub duration: f64,
    /// Milliseconds. Writing it seeks.
    #[qml(notify)]
    pub position: f64,
    #[qml(notify, readonly)]
    pub buffer_progress: f64,
    #[qml(notify, readonly)]
    pub has_audio: bool,
    #[qml(notify, readonly)]
    pub has_video: bool,
    #[qml(notify, readonly)]
    pub seekable: bool,
    #[qml(notify, readonly)]
    pub playing: bool,
    #[qml(notify)]
    pub playback_rate: f64,
    #[qml(notify)]
    pub loops: i32,
    #[qml(notify, readonly, enumeration = "PlaybackState")]
    pub playback_state: i32,
    #[qml(notify, readonly, enumeration = "MediaStatus")]
    pub media_status: i32,
    #[qml(readonly, object = "MediaMetaData")]
    pub meta_data: Option<ObjectId>,
    #[qml(readonly, enumeration = "Error")]
    pub error: i32,
    #[qml(readonly)]
    pub error_string: Atom,
    #[qml(notify)]
    pub video_output: Option<ObjectId>,
    #[qml(notify, object = "AudioOutput")]
    pub audio_output: Option<ObjectId>,
    #[qml(notify)]
    pub audio_buffer_output: Option<ObjectId>,
    #[qml(readonly)]
    pub audio_tracks: Vec<Value>,
    #[qml(readonly)]
    pub video_tracks: Vec<Value>,
    #[qml(readonly)]
    pub subtitle_tracks: Vec<Value>,
    pub active_audio_track: i32,
    pub active_video_track: i32,
    pub active_subtitle_track: i32,
    /// Speed changes the pitch: the sound is played at another rate, not
    /// stretched.
    #[qml(readonly, constant, enumeration = "PitchCompensationAvailability")]
    pub pitch_compensation_availability: i32,
    #[qml(notify)]
    pub pitch_compensation: bool,
    #[qml(skip)]
    run: Run,
}

impl Default for MediaPlayer {
    fn default() -> Self {
        MediaPlayer {
            object: QtObject::default(),
            source: Url(Atom::EMPTY),
            auto_play: false,
            duration: 0.0,
            position: 0.0,
            buffer_progress: 0.0,
            has_audio: false,
            has_video: false,
            seekable: false,
            playing: false,
            playback_rate: 1.0,
            loops: 1,
            playback_state: state::STOPPED,
            media_status: status::NO_MEDIA,
            meta_data: None,
            error: 0,
            error_string: Atom::EMPTY,
            video_output: None,
            audio_output: None,
            audio_buffer_output: None,
            audio_tracks: Vec::new(),
            video_tracks: Vec::new(),
            subtitle_tracks: Vec::new(),
            active_audio_track: 0,
            active_video_track: 0,
            active_subtitle_track: -1,
            pitch_compensation_availability: 2,
            pitch_compensation: false,
            run: Run::default(),
        }
    }
}

/// What a player keeps between calls.
#[derive(Default)]
struct Run {
    /// Bumped by every new source: answers for an older one are dropped.
    generation: u64,
    media: Option<Media>,
    /// The position the clock started from, and when (`None`: stopped).
    anchor_pos: f64,
    anchor_at: Option<Time>,
    /// The sound playing, and the position it started from.
    voice: Option<Arc<Voice>>,
    voice_from: f64,
    /// The last position reported, so that a write that is not ours is
    /// told apart as a seek.
    reported: f64,
    tick: Option<TimerId>,
    frame: Option<TimerId>,
    /// The frame last asked for, by index.
    frame_asked: Option<i64>,
    /// `play()` came while the source was loading.
    want_play: bool,
    /// Times the source has played through since `play()`.
    loops_done: i32,
    completed: bool,
}

impl fmt::Debug for Run {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Run")
            .field("generation", &self.generation)
            .field("playing", &self.anchor_at.is_some())
            .finish_non_exhaustive()
    }
}

struct Media {
    duration: f64,
    has_audio: bool,
    video: Option<VideoInfo>,
    #[cfg(feature = "decode")]
    decoder: Option<crate::decoder::Decoder>,
}

#[qml_impl]
impl MediaPlayer {
    #[qml(signal, name = "tracksChanged")]
    fn tracks_changed();
    #[qml(signal, name = "activeTracksChanged")]
    fn active_tracks_changed();
    #[qml(signal, name = "metaDataChanged")]
    fn meta_data_changed();
    #[qml(signal, name = "errorChanged")]
    fn error_changed();
    #[qml(signal, name = "errorOccurred")]
    fn error_occurred(error: i32, error_string: Atom);

    #[qml(invokable)]
    fn play(cx: &Cx) -> Result<(), EngineError> {
        play(cx.engine, cx.this);
        Ok(())
    }

    #[qml(invokable)]
    fn pause(cx: &Cx) -> Result<(), EngineError> {
        pause(cx.engine, cx.this);
        Ok(())
    }

    #[qml(invokable)]
    fn stop(cx: &Cx) -> Result<(), EngineError> {
        stop(cx.engine, cx.this);
        Ok(())
    }

    #[qml(invokable, name = "setPosition")]
    fn set_position_js(cx: &Cx, position: f64) -> Result<(), EngineError> {
        cx.set(MediaPlayer::POSITION, position)
    }

    #[qml(invokable, name = "setPlaybackRate")]
    fn set_playback_rate_js(cx: &Cx, rate: f64) -> Result<(), EngineError> {
        cx.set(MediaPlayer::PLAYBACK_RATE, rate)
    }

    #[qml(invokable, name = "setSource")]
    fn set_source_js(cx: &Cx, source: Url) -> Result<(), EngineError> {
        cx.set(MediaPlayer::SOURCE, source)
    }

    #[qml(invokable, name = "setPitchCompensation")]
    fn set_pitch_compensation_js(cx: &Cx, on: bool) -> Result<(), EngineError> {
        cx.set(MediaPlayer::PITCH_COMPENSATION, on)
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        let e = cx.engine;
        cx.with_mut::<MediaPlayer, _>(|p| p.run.completed = true)?;
        e.watch(me, MediaPlayer::SOURCE.index, move |e, _| load(e, me))?;
        e.watch(me, MediaPlayer::POSITION.index, move |e, v| {
            let reported = run(e, me, |r| r.reported).unwrap_or(0.0);
            let v = v.to_number();
            if v != reported {
                seek(e, me, v);
            }
        })?;
        e.watch(me, MediaPlayer::PLAYBACK_RATE.index, move |e, _| {
            if playing(e, me) {
                // The clock goes on from here at the new rate.
                let pos = clock(e, me);
                restart_clock(e, me, pos);
            }
        })?;
        for p in [
            MediaPlayer::ACTIVE_AUDIO_TRACK.index,
            MediaPlayer::ACTIVE_VIDEO_TRACK.index,
            MediaPlayer::ACTIVE_SUBTITLE_TRACK.index,
        ] {
            e.watch(me, p, move |e, _| {
                let _ = e.emit_by_name(me, "activeTracksChanged", &[]);
            })?;
        }
        e.watch(me, MediaPlayer::VIDEO_OUTPUT.index, move |e, v| {
            video::attach(e, v.as_object(), me);
            arm_frames(e, me);
        })?;
        if let Some(out) = cx.get(MediaPlayer::VIDEO_OUTPUT)? {
            video::attach(e, Some(out), me);
        }
        // The sound follows the output's volume and mute.
        e.effect_in(me, move |e| {
            let out = e.get(me, MediaPlayer::AUDIO_OUTPUT).ok().flatten();
            let gain = gain_of(e, out);
            if let Some(v) = e.untrack(|| run(e, me, |r| r.voice.clone())).flatten() {
                v.set_gain(gain);
            }
        })?;
        e.watch(me, MediaPlayer::AUDIO_OUTPUT.index, move |e, _| {
            if playing(e, me) {
                let pos = clock(e, me);
                restart_clock(e, me, pos);
            }
        })?;
        let auto = cx.get(MediaPlayer::AUTO_PLAY)?;
        cx.with_mut::<MediaPlayer, _>(|p| p.run.want_play = auto)?;
        load(e, me);
        Ok(())
    }
}

fn run<R>(e: &Engine, me: ObjectId, f: impl FnOnce(&mut Run) -> R) -> Option<R> {
    e.with_mut::<MediaPlayer, _>(me, |p| f(&mut p.run)).ok()
}

/// Writes a property as the player, read-only or not.
fn put<T: QmlValue>(e: &Engine, me: ObjectId, p: Prop<T>, v: T) {
    let _ = e.set_internal(me, p, v);
}

fn get<T: QmlValue + Default>(e: &Engine, me: ObjectId, p: Prop<T>) -> T {
    e.untrack(|| e.get(me, p)).unwrap_or_default()
}

fn media_status(e: &Engine, me: ObjectId) -> i32 {
    get(e, me, MediaPlayer::MEDIA_STATUS)
}

fn playing(e: &Engine, me: ObjectId) -> bool {
    run(e, me, |r| r.anchor_at.is_some()).unwrap_or(false)
}

fn set_status(e: &Engine, me: ObjectId, s: i32) {
    let progress = match s {
        status::BUFFERING => 0.25,
        status::BUFFERED => 1.0,
        _ => 0.0,
    };
    // Qt reports the buffer before the status it explains.
    if s != status::BUFFERING {
        put(e, me, MediaPlayer::BUFFER_PROGRESS, progress);
        put(e, me, MediaPlayer::MEDIA_STATUS, s);
    } else {
        put(e, me, MediaPlayer::MEDIA_STATUS, s);
        put(e, me, MediaPlayer::BUFFER_PROGRESS, progress);
    }
}

/// Reports a position the player reached.
fn report(e: &Engine, me: ObjectId, pos: f64) {
    run(e, me, |r| r.reported = pos);
    put(e, me, MediaPlayer::POSITION, pos);
}

/// Opens the current source, dropping whatever the last one was doing.
fn load(e: &Engine, me: ObjectId) {
    halt(e, me);
    let generation = run(e, me, |r| {
        r.generation += 1;
        r.media = None;
        r.loops_done = 0;
        r.generation
    })
    .unwrap_or(0);
    let output = get(e, me, MediaPlayer::VIDEO_OUTPUT);
    video::clear(e, output);
    let source = get(e, me, MediaPlayer::SOURCE).0;
    let had_error = get(e, me, MediaPlayer::ERROR) != 0;
    put(e, me, MediaPlayer::ERROR, 0);
    put(e, me, MediaPlayer::ERROR_STRING, Atom::EMPTY);
    if had_error {
        let _ = e.emit_by_name(me, "errorChanged", &[]);
    }
    put(e, me, MediaPlayer::PLAYING, false);
    put(e, me, MediaPlayer::PLAYBACK_STATE, state::STOPPED);
    report(e, me, 0.0);
    put(e, me, MediaPlayer::DURATION, 0.0);
    put(e, me, MediaPlayer::SEEKABLE, false);
    put(e, me, MediaPlayer::HAS_AUDIO, false);
    put(e, me, MediaPlayer::HAS_VIDEO, false);
    if source.is_empty() {
        set_status(e, me, status::NO_MEDIA);
        return;
    }
    set_status(e, me, status::LOADING);
    let url = e.resolve_url(me, &source);
    open::start(e, me, generation, url, None, opened);
}

fn opened(e: &Engine, me: ObjectId, generation: u64, outcome: Result<Opened, OpenError>) {
    if !e.is_alive(me) || run(e, me, |r| r.generation) != Some(generation) {
        return;
    }
    match outcome {
        Ok(o) => loaded(e, me, o),
        Err(err) => failed(e, me, &err),
    }
}

fn loaded(e: &Engine, me: ObjectId, o: Opened) {
    #[cfg(feature = "decode")]
    let decoder = crate::decoder::Decoder::new(o.source.clone());
    run(e, me, |r| {
        r.media = Some(Media {
            duration: o.duration_ms,
            has_audio: o.has_audio,
            video: o.video,
            #[cfg(feature = "decode")]
            decoder,
        });
    });
    put(e, me, MediaPlayer::DURATION, o.duration_ms);
    let audio = tracks(e, me, o.audio_tracks);
    let video = tracks(e, me, o.video_tracks);
    put(e, me, MediaPlayer::AUDIO_TRACKS, audio);
    put(e, me, MediaPlayer::VIDEO_TRACKS, video);
    let _ = e.emit_by_name(me, "tracksChanged", &[]);
    set_meta(e, me, o.meta);
    let _ = e.emit_by_name(me, "metaDataChanged", &[]);
    put(e, me, MediaPlayer::SEEKABLE, true);
    put(e, me, MediaPlayer::HAS_AUDIO, o.has_audio);
    put(e, me, MediaPlayer::HAS_VIDEO, o.video.is_some());
    set_status(e, me, status::LOADED);
    if run(e, me, |r| std::mem::take(&mut r.want_play)).unwrap_or(false) {
        play(e, me);
    }
}

fn tracks(e: &Engine, me: ObjectId, list: Vec<Vec<Entry>>) -> Vec<Value> {
    list.into_iter()
        .filter_map(|entries| {
            let id = e
                .create_native(MediaMetaData::with(entries), Some(me))
                .ok()?;
            let _ = e.complete(id);
            Some(Value::Object(id))
        })
        .collect()
}

fn set_meta(e: &Engine, me: ObjectId, entries: Vec<Entry>) {
    match get(e, me, MediaPlayer::META_DATA) {
        Some(meta) if e.is_alive(meta) => {
            let _ = e.with_mut::<MediaMetaData, _>(meta, |m| m.replace(entries));
        }
        _ => {
            if let Ok(id) = e.create_native(MediaMetaData::with(entries), Some(me)) {
                let _ = e.complete(id);
                put(e, me, MediaPlayer::META_DATA, Some(id));
            }
        }
    }
}

fn failed(e: &Engine, me: ObjectId, err: &OpenError) {
    let message = err.message();
    e.log(
        LogLevel::Warning,
        crate::CATEGORY,
        format!(
            "Could not open media {}: {message}",
            get(e, me, MediaPlayer::SOURCE).0
        ),
    );
    run(e, me, |r| r.want_play = false);
    put(e, me, MediaPlayer::ERROR, err.code());
    put(
        e,
        me,
        MediaPlayer::ERROR_STRING,
        Atom::from(message.as_str()),
    );
    let _ = e.emit_by_name(
        me,
        "errorOccurred",
        &[Value::Int(err.code()), Value::from(message.as_str())],
    );
    let _ = e.emit_by_name(me, "errorChanged", &[]);
    set_status(e, me, status::INVALID);
}

pub(crate) fn play(e: &Engine, me: ObjectId) {
    let s = media_status(e, me);
    match s {
        status::NO_MEDIA | status::INVALID => return,
        status::LOADING => {
            run(e, me, |r| r.want_play = true);
            return;
        }
        _ => {}
    }
    if playing(e, me) {
        return;
    }
    let mut pos = get(e, me, MediaPlayer::POSITION);
    if s == status::END {
        pos = 0.0;
        report(e, me, 0.0);
    }
    let from_start = s == status::LOADED && pos == 0.0;
    run(e, me, |r| r.loops_done = 0);
    put(e, me, MediaPlayer::PLAYING, true);
    put(e, me, MediaPlayer::PLAYBACK_STATE, state::PLAYING);
    if s != status::BUFFERING {
        set_status(e, me, status::BUFFERING);
    }
    if from_start {
        let generation = run(e, me, |r| r.generation).unwrap_or(0);
        e.call_at(e.now(), move |e| {
            if run(e, me, |r| r.generation) == Some(generation) && playing(e, me) {
                set_status(e, me, status::BUFFERED);
            }
        });
    }
    restart_clock(e, me, pos);
}

pub(crate) fn pause(e: &Engine, me: ObjectId) {
    let s = media_status(e, me);
    match s {
        status::NO_MEDIA | status::INVALID => return,
        status::LOADING => {
            run(e, me, |r| r.want_play = false);
            return;
        }
        _ => {}
    }
    if get(e, me, MediaPlayer::PLAYBACK_STATE) == state::PAUSED {
        return;
    }
    let pos = if playing(e, me) {
        clock(e, me)
    } else {
        get(e, me, MediaPlayer::POSITION)
    };
    halt(e, me);
    report(e, me, pos);
    put(e, me, MediaPlayer::PLAYBACK_STATE, state::PAUSED);
    put(e, me, MediaPlayer::PLAYING, false);
    if matches!(s, status::LOADED | status::END) {
        set_status(e, me, status::BUFFERING);
    }
    show_still(e, me, pos);
}

pub(crate) fn stop(e: &Engine, me: ObjectId) {
    let s = media_status(e, me);
    if matches!(s, status::NO_MEDIA | status::INVALID | status::LOADING) {
        run(e, me, |r| r.want_play = false);
        return;
    }
    let was = get(e, me, MediaPlayer::PLAYBACK_STATE);
    halt(e, me);
    report(e, me, 0.0);
    if was == state::STOPPED {
        return;
    }
    put(e, me, MediaPlayer::PLAYING, false);
    put(e, me, MediaPlayer::PLAYBACK_STATE, state::STOPPED);
    set_status(e, me, status::LOADED);
    video::clear(e, get(e, me, MediaPlayer::VIDEO_OUTPUT));
}

fn seek(e: &Engine, me: ObjectId, to: f64) {
    let duration = run(e, me, |r| r.media.as_ref().map(|m| m.duration)).flatten();
    let Some(duration) = duration else {
        // Nothing to seek in: the position stays where it was.
        let back = run(e, me, |r| r.reported).unwrap_or(0.0);
        report(e, me, back);
        return;
    };
    let to = if to.is_nan() {
        0.0
    } else {
        to.clamp(0.0, duration)
    };
    report(e, me, to);
    if playing(e, me) {
        restart_clock(e, me, to);
        return;
    }
    let s = media_status(e, me);
    if get(e, me, MediaPlayer::PLAYBACK_STATE) == state::PAUSED {
        if s != status::LOADED {
            set_status(e, me, status::LOADED);
        }
        show_still(e, me, to);
    } else if s == status::END {
        set_status(e, me, status::LOADED);
    }
}

/// Drops what the player has armed and the sound it plays.
fn halt(e: &Engine, me: ObjectId) {
    let Some((tick, frame, voice)) = run(e, me, |r| {
        r.anchor_at = None;
        r.frame_asked = None;
        (r.tick.take(), r.frame.take(), r.voice.take())
    }) else {
        return;
    };
    for id in [tick, frame].into_iter().flatten() {
        e.cancel_call(id);
    }
    if let Some(v) = voice {
        Audio::of(e).remove(&v);
    }
    #[cfg(feature = "decode")]
    run(e, me, |r| {
        if let Some(d) = r.media.as_ref().and_then(|m| m.decoder.as_ref()) {
            d.stop_feed();
        }
    });
}

/// Where the player is, by its clock.
fn clock(e: &Engine, me: ObjectId) -> f64 {
    let rate = get(e, me, MediaPlayer::PLAYBACK_RATE);
    let format = Audio::of(e).format();
    run(e, me, |r| {
        let Some(at) = r.anchor_at else {
            return r.reported;
        };
        let duration = r.media.as_ref().map_or(0.0, |m| m.duration);
        let pos = match (&r.voice, format) {
            (Some(v), Some(f)) if v.played() > 0 => {
                // The device's: what it has played is where the sound is.
                let played = v.played() as f64 / f64::from(f.rate) * 1000.0 * rate;
                (r.voice_from + played).max(r.reported)
            }
            _ => {
                let elapsed = e.now().saturating_since(at).as_secs_f64() * 1000.0;
                r.anchor_pos + elapsed * rate
            }
        };
        pos.clamp(0.0, duration.max(0.0))
    })
    .unwrap_or(0.0)
}

/// Starts the clock from `pos` now, with sound and deadlines to match.
fn restart_clock(e: &Engine, me: ObjectId, pos: f64) {
    halt(e, me);
    let now = e.now();
    let rate = get(e, me, MediaPlayer::PLAYBACK_RATE);
    let out = get(e, me, MediaPlayer::AUDIO_OUTPUT);
    let gain = e.untrack(|| gain_of(e, out));
    let has_audio = run(e, me, |r| r.media.as_ref().is_some_and(|m| m.has_audio)).unwrap_or(false);
    let format = Audio::of(e).format();
    // A player with no output plays no sound, as in Qt.
    let voice = match (has_audio && out.is_some() && rate > 0.0, format) {
        (true, Some(_)) => Some(Voice::stream(gain)),
        _ => None,
    };
    run(e, me, |r| {
        r.anchor_pos = pos;
        r.anchor_at = Some(now);
        r.voice_from = pos;
        r.voice = voice.clone();
        r.reported = pos;
    });
    if let (Some(v), Some(format)) = (&voice, format) {
        Audio::of(e).play(v);
        #[cfg(feature = "decode")]
        run(e, me, |r| {
            if let Some(m) = &r.media
                && let Some(d) = &m.decoder
            {
                d.feed(Arc::clone(v), pos, m.duration, format, rate);
            }
        });
        #[cfg(not(feature = "decode"))]
        let _ = format;
    }
    arm_tick(e, me);
    arm_frames(e, me);
}

fn arm_tick(e: &Engine, me: ObjectId) {
    let pos = clock(e, me);
    let rate = get(e, me, MediaPlayer::PLAYBACK_RATE);
    let Some(duration) = run(e, me, |r| r.media.as_ref().map(|m| m.duration)).flatten() else {
        return;
    };
    let mut wait = POSITION_INTERVAL;
    if rate > 0.0 {
        // The end of the stream is a deadline of its own, so a clip ends
        // at its length, not at the next report after it.
        let to_end = Duration::from_secs_f64(((duration - pos) / rate / 1000.0).max(0.0));
        wait = wait.min(to_end);
    }
    let generation = run(e, me, |r| r.generation).unwrap_or(0);
    let id = e.call_at(e.now() + wait, move |e| tick(e, me, generation));
    run(e, me, |r| r.tick = Some(id));
}

fn tick(e: &Engine, me: ObjectId, generation: u64) {
    if !e.is_alive(me) || run(e, me, |r| r.generation) != Some(generation) {
        return;
    }
    run(e, me, |r| r.tick = None);
    if !playing(e, me) {
        return;
    }
    let pos = clock(e, me);
    let duration = run(e, me, |r| r.media.as_ref().map_or(0.0, |m| m.duration)).unwrap_or(0.0);
    if pos >= duration {
        ended(e, me, duration);
        return;
    }
    report(e, me, pos.round());
    arm_tick(e, me);
}

fn ended(e: &Engine, me: ObjectId, duration: f64) {
    let loops = get(e, me, MediaPlayer::LOOPS);
    let done = run(e, me, |r| {
        r.loops_done += 1;
        r.loops_done
    })
    .unwrap_or(1);
    if loops < 0 || done < loops {
        report(e, me, 0.0);
        restart_clock(e, me, 0.0);
        return;
    }
    halt(e, me);
    let output = get(e, me, MediaPlayer::VIDEO_OUTPUT);
    if video::clears_at_end(e, output) {
        video::clear(e, output);
    }
    report(e, me, duration);
    put(e, me, MediaPlayer::PLAYING, false);
    put(e, me, MediaPlayer::PLAYBACK_STATE, state::STOPPED);
    set_status(e, me, status::END);
}

/// Arms the next frame, when a frame can be seen.
pub(crate) fn arm_frames(e: &Engine, me: ObjectId) {
    if !e.is_alive(me) || !playing(e, me) {
        return;
    }
    let Some(info) = run(e, me, |r| r.media.as_ref().and_then(|m| m.video)).flatten() else {
        return;
    };
    let output = get(e, me, MediaPlayer::VIDEO_OUTPUT);
    if !video::shown(e, output) {
        if let Some(id) = run(e, me, |r| r.frame.take()).flatten() {
            e.cancel_call(id);
        }
        return;
    }
    if run(e, me, |r| r.frame.is_some()).unwrap_or(false) {
        return;
    }
    let pos = clock(e, me);
    let index = (pos * info.fps / 1000.0).floor() as i64;
    ask_frame(e, me, index, info);
    let rate = get(e, me, MediaPlayer::PLAYBACK_RATE);
    if rate <= 0.0 {
        return;
    }
    let next = (index + 1) as f64 * 1000.0 / info.fps;
    let wait = Duration::from_secs_f64(((next - pos) / rate / 1000.0).max(0.001));
    let generation = run(e, me, |r| r.generation).unwrap_or(0);
    let id = e.call_at(e.now() + wait, move |e| {
        if run(e, me, |r| r.generation) != Some(generation) {
            return;
        }
        run(e, me, |r| r.frame = None);
        arm_frames(e, me);
    });
    run(e, me, |r| r.frame = Some(id));
}

/// Shows the frame at `pos` without playing: after a pause or a seek.
fn show_still(e: &Engine, me: ObjectId, pos: f64) {
    let Some(info) = run(e, me, |r| r.media.as_ref().and_then(|m| m.video)).flatten() else {
        return;
    };
    if !video::shown(e, get(e, me, MediaPlayer::VIDEO_OUTPUT)) {
        return;
    }
    let index = (pos * info.fps / 1000.0).floor() as i64;
    ask_frame(e, me, index, info);
}

/// The output has come into view, or gone out of it.
pub(crate) fn output_shown(e: &Engine, me: ObjectId) {
    if playing(e, me) {
        arm_frames(e, me);
    } else if get(e, me, MediaPlayer::PLAYBACK_STATE) == state::PAUSED {
        let pos = get(e, me, MediaPlayer::POSITION);
        show_still(e, me, pos);
    }
}

#[cfg(feature = "decode")]
fn ask_frame(e: &Engine, me: ObjectId, index: i64, info: VideoInfo) {
    let asked = run(e, me, |r| {
        let again = r.frame_asked == Some(index);
        r.frame_asked = Some(index);
        again
    })
    .unwrap_or(true);
    if asked {
        return;
    }
    let generation = run(e, me, |r| r.generation).unwrap_or(0);
    let reply = ljos_platform::Platform::of(e).expect();
    let at = index as f64 * 1000.0 / info.fps;
    run(e, me, move |r| {
        if let Some(d) = r.media.as_ref().and_then(|m| m.decoder.as_ref()) {
            d.frame(
                at,
                reply,
                Box::new(move |e, image| {
                    if run(e, me, |r| r.generation) != Some(generation) {
                        return;
                    }
                    let output = get(e, me, MediaPlayer::VIDEO_OUTPUT);
                    // A frame decoded before the end may come back after it,
                    // and must not show again what the end took away.
                    if media_status(e, me) == status::END && video::clears_at_end(e, output) {
                        return;
                    }
                    if let Some(image) = image {
                        video::show(e, output, image);
                    }
                }),
            );
        }
    });
}

#[cfg(not(feature = "decode"))]
fn ask_frame(_: &Engine, _: ObjectId, _: i64, _: VideoInfo) {}

impl Drop for MediaPlayer {
    fn drop(&mut self) {
        if let Some(v) = self.run.voice.take() {
            v.stop();
        }
    }
}
