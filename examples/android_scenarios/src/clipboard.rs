//! `ScenarioClipboard` (module `LjosScenarios`): the system's clipboard
//! as the platform reaches it, for the runner to copy as another
//! application does, to read what the application copied, and to copy a
//! picture as the application's "Copy image" would
//! (features/mobile/platform-services.feature).
//!
//! Each call goes straight to the platform's clipboard service, behind the
//! back of what the application's text fields follow: they hear of it
//! only as they hear of another application's copy, through the system.

use ljos::prelude::*;

/// The name the application puts its one `ScenarioClipboard` in the
/// root's scope by, for the runner (`ljos_bdd`'s `@android` backend) to
/// call.
pub(crate) const NAME: &str = "scenarioClipboard";

#[derive(Debug, Default, QmlObject)]
#[qml(module = "LjosScenarios", name = "ScenarioClipboard", extends = QtObject)]
pub struct ScenarioClipboard {
    #[qml(base)]
    pub object: QtObject,
}

#[qml_impl]
impl ScenarioClipboard {
    /// Puts `text` on the clipboard, as copying it in another application
    /// does. Empty when it went there, else why not.
    #[qml(invokable)]
    fn copy_elsewhere(cx: &Cx, text: String) -> String {
        failure(
            Platform::of(cx.engine)
                .clipboard()
                .set_text(ClipboardMode::Clipboard, &text),
        )
    }

    /// The text on the clipboard; `null` when it could not be read.
    #[qml(invokable)]
    fn text(cx: &Cx) -> Value {
        Platform::of(cx.engine)
            .clipboard()
            .text(ClipboardMode::Clipboard)
            .map_or(Value::Null, |text| Value::from(text.as_str()))
    }

    /// Copies a picture of `width` by `height` pixels through the
    /// platform's clipboard, as the application's "Copy image" does.
    /// Empty when it went there, else why not.
    #[qml(invokable)]
    fn copy_picture(cx: &Cx, width: i32, height: i32) -> String {
        match picture(width, height) {
            Some(image) => failure(Platform::of(cx.engine).clipboard().set_image(image)),
            None => format!("no picture is {width} by {height} pixels"),
        }
    }

    /// The picture on the clipboard as "width height", as a paste in this
    /// application reads it; empty when it holds none.
    #[qml(invokable)]
    fn picture(cx: &Cx) -> String {
        Platform::of(cx.engine)
            .clipboard()
            .image()
            .map_or_else(String::new, |i| format!("{} {}", i.width, i.height))
    }
}

fn failure(done: Result<(), ClipboardError>) -> String {
    done.err().map_or_else(String::new, |e| e.to_string())
}

/// A picture of `width` by `height` opaque pixels, each its own shade so
/// a picture turned or cut on the way is not the same.
fn picture(width: i32, height: i32) -> Option<ClipboardImage> {
    let (width, height) = (usize::try_from(width).ok()?, usize::try_from(height).ok()?);
    if width == 0 || height == 0 {
        return None;
    }
    let rgba = (0..width * height)
        .flat_map(|i| {
            let shade = u8::try_from(i % 256).unwrap_or(0);
            [shade, 255 - shade, 128, 255]
        })
        .collect();
    Some(ClipboardImage {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::picture;

    #[test]
    fn a_picture_is_as_big_as_asked() {
        let image = picture(3, 2).expect("a picture");
        assert_eq!((image.width, image.height, image.rgba.len()), (3, 2, 24));
        assert_ne!(image.rgba[0..4], image.rgba[4..8]);
    }

    #[test]
    fn no_picture_is_empty_or_negative() {
        assert!(picture(0, 2).is_none());
        assert!(picture(3, -1).is_none());
    }
}
