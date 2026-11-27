//! The system's cameras on Android, through Camera2 on the Java side
//! (`dev.ljos.LjosCamera` of the template, reached through `ljos_jni`).
//! Each capture streams YUV_420_888 pictures on a thread of its own; their
//! planes come here as the direct buffers the camera wrote, with their row
//! and pixel strides, and become [`VideoFrame::yuv420_strided`] frames on
//! that thread.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use ljos_jni::jni::objects::{JByteBuffer, JClass, JString, JValue};
use ljos_jni::jni::{Env, EnvUnowned, jni_sig, jni_str};
use ljos_jni::{app_class, with_java};

use crate::android_camera_list::parse;
use crate::camera::{CameraFrames, CameraInfo, CameraService, Capture};
use crate::frame_input::{StridedPlane, VideoFrame, YuvColour, YuvMatrix};

/// The captures running, by the handle the Java side names them by.
fn open() -> MutexGuard<'static, HashMap<i64, CameraFrames>> {
    static OPEN: OnceLock<Mutex<HashMap<i64, CameraFrames>>> = OnceLock::new();
    OPEN.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn next_handle() -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Android's cameras; the first the system lists that faces back is the
/// default, else the first.
#[derive(Clone, Copy, Debug, Default)]
pub struct AndroidCameras;

impl CameraService for AndroidCameras {
    fn cameras(&self) -> Vec<CameraInfo> {
        let listed = with_java(|env, context| {
            let class = app_class(env, context, jni_str!("dev.ljos.LjosCamera"))?;
            let listed = env
                .call_static_method(
                    &class,
                    jni_str!("list"),
                    jni_sig!("(Landroid/content/Context;)Ljava/lang/String;"),
                    &[JValue::Object(context)],
                )?
                .l()?;
            env.cast_local::<JString>(listed)?.try_to_string(env)
        });
        match listed {
            Ok(listed) => parse(&listed),
            Err(e) => {
                log::warn!("the system did not list its cameras: {e}");
                Vec::new()
            }
        }
    }

    fn open(&self, camera: &CameraInfo, frames: CameraFrames) -> Result<Capture, String> {
        let handle = next_handle();
        open().insert(handle, frames);
        let opened = with_java(|env, context| {
            let class = app_class(env, context, jni_str!("dev.ljos.LjosCamera"))?;
            let id = env.new_string(&camera.id)?;
            env.call_static_method(
                &class,
                jni_str!("open"),
                jni_sig!("(Landroid/content/Context;Ljava/lang/String;J)V"),
                &[
                    JValue::Object(context),
                    JValue::from(&id),
                    JValue::Long(handle),
                ],
            )?;
            Ok(())
        });
        match opened {
            Ok(()) => Ok(Capture::new(Running(handle))),
            Err(e) => {
                open().remove(&handle);
                Err(e.to_string())
            }
        }
    }
}

/// A capture on the Java side, closed when dropped.
struct Running(i64);

impl Drop for Running {
    fn drop(&mut self) {
        // Taken out first: a picture on its way finds no capture.
        let frames = open().remove(&self.0);
        drop(frames);
        let handle = self.0;
        let closed = with_java(|env, context| {
            let class = app_class(env, context, jni_str!("dev.ljos.LjosCamera"))?;
            env.call_static_method(
                &class,
                jni_str!("close"),
                jni_sig!("(J)V"),
                &[JValue::Long(handle)],
            )?;
            Ok(())
        });
        if let Err(e) = closed {
            log::warn!("camera capture {handle} did not close: {e}");
        }
    }
}

/// Phone cameras write full-range BT.601 (JFIF) Y'CbCr.
const COLOUR: YuvColour = YuvColour {
    matrix: YuvMatrix::Bt601,
    full_range: true,
};

/// The bytes of a direct buffer, for as long as the Java side holds the
/// picture (the call this runs in).
///
/// # Safety
/// `buffer` is a direct buffer whose memory outlives `'a`.
unsafe fn bytes<'a>(env: &Env<'_>, buffer: &JByteBuffer<'_>) -> Option<&'a [u8]> {
    let ptr = env.get_direct_buffer_address(buffer).ok()?;
    let len = env.get_direct_buffer_capacity(buffer).ok()?;
    // SAFETY: the caller's; the address and capacity are the buffer's.
    Some(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// A picture of the capture `handle`, from `LjosCamera`'s reader thread.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_ljos_LjosCamera_nativeFrame<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    handle: i64,
    width: i32,
    height: i32,
    y: JByteBuffer<'caller>,
    y_row: i32,
    u: JByteBuffer<'caller>,
    v: JByteBuffer<'caller>,
    uv_row: i32,
    uv_pixel: i32,
) {
    unowned
        .with_env(|env| -> ljos_jni::jni::errors::Result<()> {
            let Some(frames) = open().get(&handle).cloned() else {
                return Ok(());
            };
            if !frames.is_live() {
                return Ok(());
            }
            // SAFETY: the Image's planes stay the camera's until the Java
            // side closes it, after this call returns.
            let planes = unsafe { (bytes(env, &y), bytes(env, &u), bytes(env, &v)) };
            let (Some(y), Some(u), Some(v)) = planes else {
                return Ok(());
            };
            let plane = |data, row: i32, pixel: i32| StridedPlane {
                data,
                row_stride: row.max(0) as usize,
                pixel_stride: pixel.max(1) as usize,
            };
            match VideoFrame::yuv420_strided(
                width.max(0) as u32,
                height.max(0) as u32,
                [
                    plane(y, y_row, 1),
                    plane(u, uv_row, uv_pixel),
                    plane(v, uv_row, uv_pixel),
                ],
                COLOUR,
            ) {
                Ok(frame) => frames.frame(frame),
                Err(e) => log::warn!("a camera picture was not read: {e}"),
            }
            Ok(())
        })
        .resolve::<ljos_jni::jni::errors::ThrowRuntimeExAndDefault>()
}

/// The capture `handle` stopped by itself; `why` is what the user is told.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_ljos_LjosCamera_nativeLost<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    handle: i64,
    why: JString<'caller>,
) {
    unowned
        .with_env(|env| -> ljos_jni::jni::errors::Result<()> {
            let why = if why.is_null() {
                String::from("The camera stopped")
            } else {
                why.try_to_string(env)?
            };
            if let Some(frames) = open().get(&handle).cloned() {
                frames.fail(why);
            }
            Ok(())
        })
        .resolve::<ljos_jni::jni::errors::ThrowRuntimeExAndDefault>()
}
