//! QtQuick for Ljós: `Item`, `Rectangle`, `Text`, anchors, `Row`/`Column`,
//! `MouseArea`, `TapHandler`, `Loader`, and [`QuickScene`], which mirrors an
//! item tree into an `ljos_render` [`RenderTree`](ljos_render::RenderTree)
//! and runs the frame pipeline (ARCHITECTURE §6).
//!
//! Behaviour follows the documentation at doc.qt.io; this crate never sees
//! egui (the shell turns its input into [`PointerEvent`]s).

mod anchors;
pub mod enums;
mod input;
mod item;
mod loader;
mod log;
mod positioners;
mod rectangle;
mod scene;
mod text;
mod tree;

pub use anchors::CATEGORY as ANCHORS_CATEGORY;
pub use input::{
    Modifiers, MouseArea, MouseButtons, MouseEvent, PointerEvent, PointerId, PointerKind,
    TapHandler, tap_handler_target,
};
pub use item::{
    AccessibleInfo, AnchorLine, AnchorProps, Anchors, Edge, Item, accessible, effective_margin,
    init_item, is_item, local_transform, scene_transform, set_accessible,
};
pub use loader::Loader;
pub use log::{LogEntry, QuickLog};
pub use positioners::{Axis, Column, Row};
pub use rectangle::{Rectangle, RectangleBorder, RectanglePaint, to_rgba};
pub use scene::{
    DOUBLE_CLICK_INTERVAL, DRAG_THRESHOLD, FrameOutput, MAX_POLISH_PASSES, POLISH_CATEGORY,
    QuickScene, SCENE_CATEGORY, TimerHook,
};
pub use text::{
    DEFAULT_PIXEL_SIZE, FixedAdvanceMeasurer, FontMetrics, FontSpec, Text, TextInputs, TextLayout,
    TextLine, TextMeasurer, layout as layout_text,
};
pub use tree::{add_data, child_items, resources, set_parent_item};

use ljos_engine::Engine;

/// Registers module `QtQuick` (contract item 3).
pub fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [
        Item, Rectangle, Text, MouseArea, TapHandler, Row, Column, Loader, MouseEvent,
    ]));
}
