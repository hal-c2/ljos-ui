//! An Android application's services, through its Java side
//! (`dev.ljos.LjosServices` of Ljós's Android template, reached with
//! `ljos_jni`): the system's clipboard, its opener of links, and the
//! directories the system keeps for the application.
//!
//! Each call waits for the Java side, which runs the clipboard's calls on
//! the main thread: call them from the application's thread, never from
//! the main thread's own callbacks.

use std::path::PathBuf;

use ljos_jni::jni::objects::{JString, JValue};
use ljos_jni::jni::{jni_sig, jni_str};
use ljos_jni::{services, with_java};

use crate::{ClipboardError, ClipboardImage, ClipboardMode, ClipboardService, OpenUrlService};

/// Android's clipboard. It holds text: what another application copied as
/// HTML or a link is pasted as its text.
#[derive(Debug, Default, Clone, Copy)]
pub struct AndroidClipboard;

impl ClipboardService for AndroidClipboard {
    fn is_system(&self) -> bool {
        true
    }

    fn text(&self, mode: ClipboardMode) -> Option<String> {
        if mode != ClipboardMode::Clipboard {
            return None;
        }
        let read = with_java(|env, context| {
            let class = services(env, context)?;
            let text = env
                .call_static_method(
                    &class,
                    jni_str!("clipboardText"),
                    jni_sig!("(Landroid/content/Context;)Ljava/lang/String;"),
                    &[JValue::Object(context)],
                )?
                .l()?;
            if text.is_null() {
                return Ok(None);
            }
            let text = env.cast_local::<JString>(text)?;
            Ok(Some(text.try_to_string(env)?))
        });
        read.unwrap_or_else(|e| {
            log::warn!("the clipboard was not read: {e}");
            None
        })
    }

    fn set_text(&self, mode: ClipboardMode, text: &str) -> Result<(), ClipboardError> {
        if mode != ClipboardMode::Clipboard {
            return Err(ClipboardError::Unavailable);
        }
        with_java(|env, context| {
            let class = services(env, context)?;
            let text = env.new_string(text)?;
            env.call_static_method(
                &class,
                jni_str!("setClipboardText"),
                jni_sig!("(Landroid/content/Context;Ljava/lang/String;)V"),
                &[JValue::Object(context), JValue::from(&text)],
            )?;
            Ok(())
        })
        .map_err(|e| ClipboardError::Other(e.to_string()))
    }

    fn html(&self) -> Option<String> {
        None
    }

    fn set_html(&self, _html: &str, alt_text: &str) -> Result<(), ClipboardError> {
        // What a paste anywhere gets is the text.
        self.set_text(ClipboardMode::Clipboard, alt_text)
    }

    fn image(&self) -> Option<ClipboardImage> {
        None
    }

    fn set_image(&self, _image: ClipboardImage) -> Result<(), ClipboardError> {
        // A picture goes on Android's clipboard as a content URI of a
        // provider of the application's, which the template has none of.
        Err(ClipboardError::Unavailable)
    }
}

/// Opens a link in the application the user picked for it (`ACTION_VIEW`).
#[derive(Debug, Default, Clone, Copy)]
pub struct AndroidOpener;

impl OpenUrlService for AndroidOpener {
    fn open(&self, url: &str) -> bool {
        with_java(|env, context| {
            let class = services(env, context)?;
            let url = env.new_string(url)?;
            env.call_static_method(
                &class,
                jni_str!("openUrl"),
                jni_sig!("(Landroid/content/Context;Ljava/lang/String;)Z"),
                &[JValue::Object(context), JValue::from(&url)],
            )?
            .z()
        })
        .unwrap_or_else(|e| {
            log::warn!("{url} was not opened: {e}");
            false
        })
    }
}

/// The directories the system keeps for the application alone: where it
/// keeps its files, and its cache, which the system may empty when short
/// of space. `None` without the Java side.
pub fn app_directories() -> Option<(PathBuf, PathBuf)> {
    let dir = |method| {
        with_java(|env, context| {
            let class = services(env, context)?;
            let path = env
                .call_static_method(
                    &class,
                    method,
                    jni_sig!("(Landroid/content/Context;)Ljava/lang/String;"),
                    &[JValue::Object(context)],
                )?
                .l()?;
            let path = env.cast_local::<JString>(path)?;
            path.try_to_string(env)
        })
        .map(PathBuf::from)
    };
    match (dir(jni_str!("filesDir")), dir(jni_str!("cacheDir"))) {
        (Ok(files), Ok(cache)) => Some((files, cache)),
        (Err(e), _) | (_, Err(e)) => {
            log::warn!("the application's directories are not known: {e}");
            None
        }
    }
}
