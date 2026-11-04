//! The wgpu painter's windows on macOS say that their pixels are sRGB.
//!
//! wgpu configures a Metal layer for `SurfaceColorSpace::Srgb` by taking
//! its colour space away, and Core Animation does not colour-match a
//! `CAMetalLayer` that has none: on a wide-gamut display the values are
//! shown in the display's own primaries, more saturated than written
//! (#192). Ljós's colours are sRGB, which is what the processor painter
//! already tells the system. egui-wgpu has no way to choose the colour
//! space, and configures the surface again when it is given the window
//! and when the window is resized, each time taking the tag away. A frame
//! is composited with the colour space its layer had when it was
//! presented, so the layer is looked at before every frame the painter
//! shows (a few messages while it is still tagged) and tagged again when
//! it is not. A surface egui-wgpu finds stale and configures inside a
//! paint shows that one frame untagged.

use egui_winit::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use egui_winit::winit::window::Window;
use objc2_app_kit::NSView;
use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};
use objc2_quartz_core::{CALayer, CAMetalLayer};

/// Tags the Metal layers of `window`'s view sRGB.
pub(crate) fn tag(window: &Window) {
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
        tag_tree(&root);
    }
}

/// Tags `root` and its sublayers that are Metal layers: wgpu draws into a
/// view's own layer when that is a Metal layer, and into one it adds
/// under it when it is not.
pub(crate) fn tag_tree(root: &CALayer) {
    if let Some(metal) = root.downcast_ref::<CAMetalLayer>() {
        tag_layer(metal);
    }
    // SAFETY: the sublayers are only read, on the thread that draws.
    let Some(sublayers) = (unsafe { root.sublayers() }) else {
        return;
    };
    for layer in sublayers.iter() {
        if let Some(metal) = layer.downcast_ref::<CAMetalLayer>() {
            tag_layer(metal);
        }
    }
}

fn tag_layer(layer: &CAMetalLayer) {
    // SAFETY: an extern static of CoreGraphics, always present.
    let srgb = unsafe { kCGColorSpaceSRGB };
    let named = layer
        .colorspace()
        .and_then(|space| CGColorSpace::name(Some(&space)));
    if named.as_deref() != Some(srgb) {
        let space = CGColorSpace::with_name(Some(srgb));
        layer.setColorspace(space.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};
    use objc2_quartz_core::{CALayer, CAMetalLayer};

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

        super::tag_tree(&root);
        super::tag_tree(&own);
        assert!(named_srgb(&under));
        assert!(named_srgb(&own));

        // As a configure by wgpu leaves it, again.
        under.setColorspace(None);
        super::tag_tree(&root);
        assert!(named_srgb(&under));
    }
}
