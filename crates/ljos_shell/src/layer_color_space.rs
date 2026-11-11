//! The wgpu painter's windows on macOS say the colour space the
//! application chose for them ([`ColorSpace`]), sRGB unless it chose
//! otherwise.
//!
//! wgpu configures a Metal layer for `SurfaceColorSpace::Srgb` by taking
//! its colour space away, and Core Animation does not colour-match a
//! `CAMetalLayer` that has none: on a wide-gamut display the values are
//! shown in the display's own primaries, more saturated than written
//! (#192). That is what Qt Quick's windows do, and what an application
//! that chooses [`ColorSpace::Untagged`] gets (#382). egui-wgpu has no way
//! to choose the colour space, and configures the surface again when it
//! is given the window and when the window is resized, each time taking
//! the tag away. A frame is composited with the colour space its layer
//! had when it was presented, so the layer is looked at before every
//! frame the painter shows (a few messages while it is still tagged) and
//! tagged when it has none. A surface egui-wgpu finds stale and
//! configures inside a paint shows that one frame untagged.

use egui_winit::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use egui_winit::winit::window::Window;
use objc2_app_kit::NSView;
use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};
use objc2_quartz_core::{CALayer, CAMetalLayer};

use crate::color_space::{ColorSpace, LayerTag};

/// Gives the Metal layers of `window`'s view what `space` says.
pub(crate) fn tag(window: &Window, space: ColorSpace) {
    // A choice that gives a bare layer nothing gives every layer nothing.
    if space.layer_tag(false).is_none() {
        return;
    }
    let Some(view) = (match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::AppKit(h)) => Some(h.ns_view),
        _ => None,
    }) else {
        return;
    };
    // SAFETY: winit's handle is to the window's live content view, and
    // the shell paints on the main thread, AppKit's.
    let view: &NSView = unsafe { view.cast().as_ref() };
    if let Some(root) = view.layer() {
        tag_tree(&root, space);
    }
}

/// Gives `root` and its sublayers that are Metal layers what `space`
/// says: wgpu draws into a view's own layer when that is a Metal layer,
/// and into one it adds under it when it is not.
pub(crate) fn tag_tree(root: &CALayer, space: ColorSpace) {
    if let Some(metal) = root.downcast_ref::<CAMetalLayer>() {
        tag_layer(metal, space);
    }
    // SAFETY: the sublayers are only read, on the thread that draws.
    let Some(sublayers) = (unsafe { root.sublayers() }) else {
        return;
    };
    for layer in sublayers.iter() {
        if let Some(metal) = layer.downcast_ref::<CAMetalLayer>() {
            tag_layer(metal, space);
        }
    }
}

/// wgpu leaves a layer for sRGB with no colour space, and gives every
/// other (extended, HDR) one its own; [`ColorSpace::layer_tag`] decides.
fn tag_layer(layer: &CAMetalLayer, space: ColorSpace) {
    match space.layer_tag(layer.colorspace().is_some()) {
        None => {}
        Some(LayerTag::Srgb) => {
            // SAFETY: an extern static of CoreGraphics, always present.
            let srgb = unsafe { kCGColorSpaceSRGB };
            layer.setColorspace(CGColorSpace::with_name(Some(srgb)).as_deref());
        }
    }
}

#[cfg(test)]
mod tests {
    use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};
    use objc2_quartz_core::{CALayer, CAMetalLayer};

    use crate::ColorSpace;

    fn named_srgb(layer: &CAMetalLayer) -> bool {
        let srgb = unsafe { kCGColorSpaceSRGB };
        layer
            .colorspace()
            .and_then(|space| CGColorSpace::name(Some(&space)))
            .as_deref()
            == Some(srgb)
    }

    /// A Metal layer as wgpu leaves it, under a view's own layer as
    /// raw-window-metal adds it, and as the view's layer itself.
    #[test]
    fn metal_layers_under_a_view_are_tagged_srgb() {
        let root = CALayer::new();
        let under = CAMetalLayer::new();
        root.addSublayer(&under);
        let own = CAMetalLayer::new();
        assert!(!named_srgb(&under) && !named_srgb(&own));

        super::tag_tree(&root, ColorSpace::Srgb);
        super::tag_tree(&own, ColorSpace::Srgb);
        assert!(named_srgb(&under));
        assert!(named_srgb(&own));

        // As a configure by wgpu leaves it, again.
        under.setColorspace(None);
        super::tag_tree(&root, ColorSpace::Srgb);
        assert!(named_srgb(&under));
    }

    /// A layer wgpu gave another colour space (an extended or HDR one)
    /// keeps it: only the one it left with none is sRGB.
    #[test]
    fn a_layer_with_its_own_colour_space_keeps_it() {
        let layer = CAMetalLayer::new();
        let p3 = unsafe { objc2_core_graphics::kCGColorSpaceDisplayP3 };
        layer.setColorspace(CGColorSpace::with_name(Some(p3)).as_deref());
        super::tag_tree(&layer, ColorSpace::Srgb);
        let named = layer
            .colorspace()
            .and_then(|space| CGColorSpace::name(Some(&space)));
        assert_eq!(named.as_deref(), Some(p3));
    }

    /// #382: an application that chose to leave its windows untagged, as
    /// Qt Quick's are, gets the layer as wgpu leaves it.
    #[test]
    fn an_untagged_application_s_layers_keep_none() {
        let root = CALayer::new();
        let under = CAMetalLayer::new();
        root.addSublayer(&under);
        super::tag_tree(&root, ColorSpace::Untagged);
        assert!(under.colorspace().is_none());
    }
}
