//! The QML face of [`crate::device`]: `BackEnd` in the
//! `qtjenny_consumer` module, the module `Main.qml` imports it
//! from. Every member is one `Main.qml` reads, writes, calls or
//! connects to.

use std::cell::RefCell;

use ljos::prelude::*;

use crate::device::{Device, DeviceError, WakeLock};

const CATEGORY: &str = "qtjennydemo";

/// What the popup says when the device cannot write system
/// settings; `Main.qml` shows it and then asks
/// `manageWriteSystemSettings()`.
const NO_SETTINGS: &str = "This device cannot grant the right to write system settings.";

/// `BackEnd`: the device the demo controls.
#[derive(Debug, QmlObject)]
#[qml(module = "qtjenny_consumer", name = "BackEnd", extends = QtObject)]
pub struct BackEnd {
    #[qml(base)]
    pub object: QtObject,
    /// The volume range the volume slider spans.
    #[qml(notify, readonly)]
    pub min_volume: i32,
    #[qml(notify, readonly)]
    pub max_volume: i32,
    /// Both ends are written by the slider's `onValueChanged`.
    #[qml(notify)]
    pub volume: i32,
    #[qml(notify)]
    pub brightness: i32,
    /// Read by the popup's close handling.
    #[qml(notify, readonly)]
    pub can_write_system_settings: bool,
    #[qml(skip)]
    device: RefCell<Device>,
}

impl Default for BackEnd {
    fn default() -> Self {
        let device = Device::desktop();
        BackEnd {
            object: QtObject::default(),
            min_volume: device.min_volume(),
            max_volume: device.max_volume(),
            volume: device.volume(),
            brightness: device.brightness(),
            can_write_system_settings: device.capabilities().system_settings,
            device: RefCell::new(device),
        }
    }
}

#[qml_impl]
impl BackEnd {
    /// Asks the QML to show `message`.
    #[qml(signal)]
    fn show_popup(message: String);

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.watch(me, BackEnd::VOLUME.index, move |e, v| {
            let now = e
                .with(me, |b: &BackEnd| {
                    b.device.borrow_mut().set_volume(v.to_number() as i32)
                })
                .unwrap_or(0);
            // A write outside the range reads back as the value in
            // effect, so the slider and the device agree.
            if now != v.to_number() as i32 {
                let _ = Cx::new(e, me).set(BackEnd::VOLUME, now);
            }
        })?;
        e.watch(me, BackEnd::BRIGHTNESS.index, move |e, v| {
            let asked = v.to_number() as i32;
            let got = e
                .with(me, |b: &BackEnd| {
                    b.device.borrow_mut().set_brightness(asked)
                })
                .unwrap_or(Ok(asked));
            match got {
                Ok(now) if now == asked => {}
                Ok(now) => {
                    let _ = Cx::new(e, me).set(BackEnd::BRIGHTNESS, now);
                }
                Err(err) => {
                    // Refused: the property goes back to the
                    // device's brightness and the refusal is told.
                    refused(e, err);
                    let was = e
                        .with(me, |b: &BackEnd| b.device.borrow().brightness())
                        .unwrap_or(0);
                    let _ = Cx::new(e, me).set(BackEnd::BRIGHTNESS, was);
                }
            }
        })?;
        // A device that cannot write settings says so as soon as
        // the QML is listening.
        let can = cx.get(BackEnd::CAN_WRITE_SYSTEM_SETTINGS)?;
        if !can {
            BackEnd::emit_show_popup(cx, NO_SETTINGS.to_owned())?;
        }
        Ok(())
    }

    /// Where Android would open its settings screen to grant the
    /// right; here there is none, which is logged.
    #[qml(invokable)]
    fn manage_write_system_settings(cx: &Cx) -> Result<(), EngineError> {
        refused(cx.engine, DeviceError::SettingsNotWritable);
        Ok(())
    }

    #[qml(invokable)]
    fn vibrate(cx: &Cx) -> Result<(), EngineError> {
        run(cx, |d| d.vibrate());
        Ok(())
    }

    #[qml(invokable)]
    fn notify(cx: &Cx) -> Result<(), EngineError> {
        run(cx, |d| d.notify());
        Ok(())
    }

    #[qml(invokable)]
    fn set_full_wake_lock(cx: &Cx) -> Result<(), EngineError> {
        run(cx, |d| d.hold(WakeLock::Full));
        Ok(())
    }

    #[qml(invokable)]
    fn set_partial_wake_lock(cx: &Cx) -> Result<(), EngineError> {
        run(cx, |d| d.hold(WakeLock::Partial));
        Ok(())
    }

    #[qml(invokable)]
    fn disable_full_wake_lock(cx: &Cx) -> Result<(), EngineError> {
        run(cx, |d| {
            d.release(WakeLock::Full);
            Ok(())
        });
        Ok(())
    }

    #[qml(invokable)]
    fn disable_partial_wake_lock(cx: &Cx) -> Result<(), EngineError> {
        run(cx, |d| {
            d.release(WakeLock::Partial);
            Ok(())
        });
        Ok(())
    }
}

fn refused(e: &Engine, err: DeviceError) {
    e.log(LogLevel::Warning, CATEGORY, format!("{err}"));
}

fn run(cx: &Cx, f: impl FnOnce(&mut Device) -> Result<(), DeviceError>) {
    let got = cx
        .with::<BackEnd, _>(|b| f(&mut b.device.borrow_mut()))
        .unwrap_or(Ok(()));
    if let Err(err) = got {
        refused(cx.engine, err);
    }
}

/// Registers `BackEnd` (module `qtjenny_consumer`) on `engine`.
pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("qtjenny_consumer" 1.0 => [BackEnd]));
}
