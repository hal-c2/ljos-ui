//! An Android application's services, through its Java side
//! (`dev.ljos.LjosServices` of Ljós's Android template, reached with
//! `ljos_jni`): the system's clipboard (text, HTML and pictures), its
//! opener of links, the directories the system keeps for the application,
//! and how far a finger goes before a touch is a drag.
//!
//! Each call waits for the Java side, which runs the clipboard's calls on
//! the main thread: call them from the application's thread, never from
//! the main thread's own callbacks.

use std::path::PathBuf;

use ljos_jni::jni::objects::{JIntArray, JString, JValue};
use ljos_jni::jni::{jni_sig, jni_str};
use ljos_jni::{services, with_java};

use crate::{ClipboardError, ClipboardImage, ClipboardMode, ClipboardService, OpenUrlService};

/// Android's clipboard: text, HTML with its text, and a picture, which
/// Android keeps as a content URI (here of the template's FileProvider). A
/// link another application copied is pasted as its text.
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

    fn clear(&self, mode: ClipboardMode) -> Result<(), ClipboardError> {
        if mode != ClipboardMode::Clipboard {
            return Err(ClipboardError::Unavailable);
        }
        with_java(|env, context| {
            let class = services(env, context)?;
            env.call_static_method(
                &class,
                jni_str!("clearClipboard"),
                jni_sig!("(Landroid/content/Context;)V"),
                &[JValue::Object(context)],
            )?;
            Ok(())
        })
        .map_err(|e| ClipboardError::Other(e.to_string()))
    }

    fn html(&self) -> Option<String> {
        let read = with_java(|env, context| {
            let class = services(env, context)?;
            let html = env
                .call_static_method(
                    &class,
                    jni_str!("clipboardHtml"),
                    jni_sig!("(Landroid/content/Context;)Ljava/lang/String;"),
                    &[JValue::Object(context)],
                )?
                .l()?;
            if html.is_null() {
                return Ok(None);
            }
            let html = env.cast_local::<JString>(html)?;
            Ok(Some(html.try_to_string(env)?))
        });
        read.unwrap_or_else(|e| {
            log::warn!("the clipboard's HTML was not read: {e}");
            None
        })
    }

    fn set_html(&self, html: &str, alt_text: &str) -> Result<(), ClipboardError> {
        with_java(|env, context| {
            let class = services(env, context)?;
            let html = env.new_string(html)?;
            let text = env.new_string(alt_text)?;
            env.call_static_method(
                &class,
                jni_str!("setClipboardHtml"),
                jni_sig!("(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)V"),
                &[
                    JValue::Object(context),
                    JValue::from(&html),
                    JValue::from(&text),
                ],
            )?;
            Ok(())
        })
        .map_err(|e| ClipboardError::Other(e.to_string()))
    }

    fn image(&self) -> Option<ClipboardImage> {
        let read = with_java(|env, context| {
            let class = services(env, context)?;
            let pixels = env
                .call_static_method(
                    &class,
                    jni_str!("clipboardImage"),
                    jni_sig!("(Landroid/content/Context;)[I"),
                    &[JValue::Object(context)],
                )?
                .l()?;
            if pixels.is_null() {
                return Ok(None);
            }
            let pixels = env.cast_local::<JIntArray>(pixels)?;
            let len = pixels.len(env)?;
            let mut argb = vec![0; len];
            pixels.get_region(env, 0, &mut argb)?;
            Ok(from_argb(&argb))
        });
        read.unwrap_or_else(|e| {
            log::warn!("the clipboard's picture was not read: {e}");
            None
        })
    }

    fn set_image(&self, image: ClipboardImage) -> Result<(), ClipboardError> {
        let (argb, width, height) = to_argb(&image).ok_or_else(|| {
            ClipboardError::Other(format!(
                "a picture of {}×{} with {} bytes is not one Android takes",
                image.width,
                image.height,
                image.rgba.len()
            ))
        })?;
        with_java(|env, context| {
            let class = services(env, context)?;
            let pixels = env.new_int_array(argb.len())?;
            pixels.set_region(env, 0, &argb)?;
            env.call_static_method(
                &class,
                jni_str!("setClipboardImage"),
                jni_sig!("(Landroid/content/Context;[III)V"),
                &[
                    JValue::Object(context),
                    JValue::from(&pixels),
                    JValue::Int(width),
                    JValue::Int(height),
                ],
            )?;
            Ok(())
        })
        .map_err(|e| ClipboardError::Other(e.to_string()))
    }
}

/// A picture as the Java side gives it, its width and height first and
/// then its pixels as unpremultiplied ARGB (`Bitmap.getPixels`). `None`
/// for one whose size and pixels disagree.
fn from_argb(argb: &[i32]) -> Option<ClipboardImage> {
    let (&[width, height], pixels) = argb.split_first_chunk::<2>()?;
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(height).ok()?;
    if width.checked_mul(height)? != pixels.len() {
        return None;
    }
    let rgba = pixels
        .iter()
        .flat_map(|&p| {
            let [a, r, g, b] = p.to_be_bytes();
            [r, g, b, a]
        })
        .collect();
    Some(ClipboardImage {
        width,
        height,
        rgba,
    })
}

/// The pixels of `image` as `Bitmap.createBitmap` takes them, with its
/// width and height. `None` for a picture Android cannot hold: empty, too
/// large for a Java array, or with bytes its size does not have.
fn to_argb(image: &ClipboardImage) -> Option<(Vec<i32>, i32, i32)> {
    let width = i32::try_from(image.width).ok()?;
    let height = i32::try_from(image.height).ok()?;
    let count = image.width.checked_mul(image.height)?;
    if count == 0 || i32::try_from(count).is_err() || image.rgba.len() != count.checked_mul(4)? {
        return None;
    }
    let argb = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&[r, g, b, a]| i32::from_be_bytes([a, r, g, b]))
        .collect();
    Some((argb, width, height))
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

/// How far a finger goes before a touch is a drag, in the application's
/// pixels (dp): `ViewConfiguration.getScaledTouchSlop`, which is 8 dp on
/// most devices. `None` without the Java side.
pub fn touch_slop() -> Option<f32> {
    with_java(|env, context| {
        let class = services(env, context)?;
        env.call_static_method(
            &class,
            jni_str!("touchSlop"),
            jni_sig!("(Landroid/content/Context;)F"),
            &[JValue::Object(context)],
        )?
        .f()
    })
    .inspect_err(|e| log::warn!("the touch slop is not known: {e}"))
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_has_no_selection_to_clear() {
        assert_eq!(
            AndroidClipboard.clear(ClipboardMode::Selection),
            Err(ClipboardError::Unavailable)
        );
    }

    #[test]
    fn a_picture_goes_to_java_as_argb_and_comes_back_as_rgba() {
        let image = ClipboardImage {
            width: 2,
            height: 1,
            rgba: vec![0x10, 0x20, 0x30, 0x80, 0xff, 0x00, 0x7f, 0xff],
        };
        let (argb, width, height) = to_argb(&image).unwrap();
        assert_eq!((width, height), (2, 1));
        #[allow(clippy::cast_possible_wrap)]
        let expected = [0x8010_2030_u32 as i32, 0xffff_007f_u32 as i32];
        assert_eq!(argb, expected);
        let mut from_java = vec![2, 1];
        from_java.extend(argb);
        assert_eq!(from_argb(&from_java), Some(image));
    }

    #[test]
    fn a_picture_whose_size_and_pixels_disagree_is_none() {
        assert_eq!(from_argb(&[]), None);
        assert_eq!(from_argb(&[2, 2, 0, 0, 0]), None);
        assert_eq!(from_argb(&[-1, 1, 0]), None);
        let short = ClipboardImage {
            width: 2,
            height: 2,
            rgba: vec![0; 12],
        };
        assert_eq!(to_argb(&short), None);
        let empty = ClipboardImage {
            width: 0,
            height: 0,
            rgba: Vec::new(),
        };
        assert_eq!(to_argb(&empty), None);
    }

    #[test]
    fn html_without_the_java_side_is_none_and_its_copy_fails() {
        assert_eq!(AndroidClipboard.html(), None);
        assert!(matches!(
            AndroidClipboard.set_html("<b>x</b>", "x"),
            Err(ClipboardError::Other(_))
        ));
        assert_eq!(AndroidClipboard.image(), None);
    }

    #[test]
    fn a_clear_without_the_java_side_says_it_did_not_happen() {
        // A test binary has no application, so no Java side to ask.
        assert!(matches!(
            AndroidClipboard.clear(ClipboardMode::Clipboard),
            Err(ClipboardError::Other(_))
        ));
    }
}
