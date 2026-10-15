//! `AudioOutput`, `AudioDevice` and `MediaDevices`: where a player's sound
//! goes and what the system has to play it on.
//!
//! Qt's `audioDevice` is a value type; here a device is an uncreatable
//! object, one per device the engine has seen, so `output.device ===
//! devices.defaultAudioOutput` holds as it does in Qt.

use std::cell::Cell;

use ljos_engine::{Atom, Cx, Engine, EngineError, ObjectId, QmlObject, QtObject, Value, qml_impl};

use crate::sink::Audio;

fn device_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![("Mode", vec![("Null", 0), ("Input", 1), ("Output", 2)])]
}

/// `AudioDevice.Mode`.
pub(crate) mod mode {
    pub const NULL: i32 = 0;
    pub const OUTPUT: i32 = 2;
}

/// `AudioDevice` (QtMultimedia): one device, as the system names it.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtMultimedia",
    name = "AudioDevice",
    extends = QtObject,
    enums = device_enums,
    uncreatable
)]
pub struct AudioDevice {
    #[qml(base)]
    pub object: QtObject,
    #[qml(readonly, constant)]
    pub id: Atom,
    #[qml(readonly, constant)]
    pub description: Atom,
    #[qml(readonly, constant)]
    pub is_default: bool,
    #[qml(readonly, constant, enumeration = "Mode")]
    pub mode: i32,
}

#[qml_impl]
impl AudioDevice {}

/// The devices this engine has handed out.
#[derive(Default)]
struct Known {
    output: Cell<Option<ObjectId>>,
    null_input: Cell<Option<ObjectId>>,
}

/// The default output as an `AudioDevice`: the system's, or a device of
/// mode `Null` when sound goes nowhere.
pub(crate) fn default_output(e: &Engine) -> Option<ObjectId> {
    let known = e.extension::<Known>();
    if let Some(id) = known.output.get().filter(|id| e.is_alive(*id)) {
        return Some(id);
    }
    let device = match Audio::of(e).describe() {
        Some(d) => AudioDevice {
            object: QtObject::default(),
            id: Atom::from(d.id),
            description: Atom::from(d.description),
            is_default: true,
            mode: mode::OUTPUT,
        },
        None => AudioDevice::default(),
    };
    let id = e.create_native(device, None).ok()?;
    let _ = e.complete(id);
    known.output.set(Some(id));
    Some(id)
}

fn null_input(e: &Engine) -> Option<ObjectId> {
    let known = e.extension::<Known>();
    if let Some(id) = known.null_input.get().filter(|id| e.is_alive(*id)) {
        return Some(id);
    }
    let id = e.create_native(AudioDevice::default(), None).ok()?;
    let _ = e.complete(id);
    known.null_input.set(Some(id));
    Some(id)
}

/// `AudioOutput` (QtMultimedia): the volume and device a player's sound
/// goes to.
#[derive(Debug, QmlObject)]
#[qml(module = "QtMultimedia", name = "AudioOutput", extends = QtObject)]
pub struct AudioOutput {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, object = "AudioDevice")]
    pub device: Option<ObjectId>,
    /// Linear, from 0 to 1; what is written outside is brought inside.
    #[qml(notify)]
    pub volume: f64,
    #[qml(notify)]
    pub muted: bool,
}

impl Default for AudioOutput {
    fn default() -> Self {
        AudioOutput {
            object: QtObject::default(),
            device: None,
            volume: 1.0,
            muted: false,
        }
    }
}

#[qml_impl]
impl AudioOutput {
    #[qml(invokable, name = "setVolume")]
    fn set_volume_js(cx: &Cx, volume: f64) -> Result<(), EngineError> {
        cx.set(AudioOutput::VOLUME, volume.clamp(0.0, 1.0))
    }

    #[qml(invokable, name = "setMuted")]
    fn set_muted_js(cx: &Cx, muted: bool) -> Result<(), EngineError> {
        cx.set(AudioOutput::MUTED, muted)
    }

    #[qml(invokable, name = "setDevice")]
    fn set_device_js(cx: &Cx, device: Option<ObjectId>) -> Result<(), EngineError> {
        cx.set(AudioOutput::DEVICE, device)
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        if cx.get(AudioOutput::DEVICE)?.is_none() {
            cx.set(AudioOutput::DEVICE, default_output(cx.engine))?;
        }
        clamp_volume(cx.engine, me);
        cx.engine
            .watch(me, AudioOutput::VOLUME.index, move |e, _| {
                clamp_volume(e, me);
            })?;
        Ok(())
    }
}

fn clamp_volume(e: &Engine, me: ObjectId) {
    let Ok(v) = e.get(me, AudioOutput::VOLUME) else {
        return;
    };
    let clamped = if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
    if clamped != v {
        let _ = e.set(me, AudioOutput::VOLUME, clamped);
    }
}

/// The gain `output` asks for: 0 when muted.
pub(crate) fn gain_of(e: &Engine, output: Option<ObjectId>) -> f32 {
    let Some(out) = output else {
        return 1.0;
    };
    let muted = e.get(out, AudioOutput::MUTED).unwrap_or(false);
    let volume = e.get(out, AudioOutput::VOLUME).unwrap_or(1.0);
    if muted { 0.0 } else { volume as f32 }
}

/// `MediaDevices` (QtMultimedia): the system's audio and video devices.
/// One output at most is listed: the default, which is where sound goes.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtMultimedia", name = "MediaDevices", extends = QtObject)]
pub struct MediaDevices {
    #[qml(base)]
    pub object: QtObject,
    #[qml(readonly)]
    pub audio_inputs: Vec<Value>,
    #[qml(readonly)]
    pub audio_outputs: Vec<Value>,
    #[qml(readonly)]
    pub video_inputs: Vec<Value>,
    #[qml(readonly, object = "AudioDevice")]
    pub default_audio_input: Option<ObjectId>,
    #[qml(readonly, object = "AudioDevice")]
    pub default_audio_output: Option<ObjectId>,
    #[qml(readonly)]
    pub default_video_input: Value,
}

#[qml_impl]
impl MediaDevices {
    #[qml(signal, name = "audioInputsChanged")]
    fn audio_inputs_changed();
    #[qml(signal, name = "audioOutputsChanged")]
    fn audio_outputs_changed();
    #[qml(signal, name = "videoInputsChanged")]
    fn video_inputs_changed();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let e = cx.engine;
        let output = default_output(e);
        let real = output
            .and_then(|o| e.get(o, AudioDevice::MODE).ok())
            .is_some_and(|m| m != mode::NULL);
        let outputs: Vec<Value> = output
            .filter(|_| real)
            .map(Value::Object)
            .into_iter()
            .collect();
        cx.set(MediaDevices::AUDIO_OUTPUTS, outputs)?;
        cx.set(MediaDevices::DEFAULT_AUDIO_OUTPUT, output)?;
        cx.set(MediaDevices::DEFAULT_AUDIO_INPUT, null_input(e))?;
        Ok(())
    }
}
