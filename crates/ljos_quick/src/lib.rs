//! QtQuick for Ljós: `Item`, `Rectangle`, `Text`, anchors, `Row`/`Column`,
//! `MouseArea`, `TapHandler`, `Loader`, the attached `Accessible`, and
//! [`QuickScene`], which mirrors an item tree into an `ljos_render`
//! [`RenderTree`](ljos_render::RenderTree), runs the frame pipeline
//! (ARCHITECTURE §6) and keeps the accessibility tree (§14).
//!
//! Behaviour follows the documentation at doc.qt.io; this crate never sees
//! egui (the shell turns its input into [`PointerEvent`]s).

mod accessible;
mod anchors;
mod animation;
pub mod enums;
mod input;
mod item;
mod layouts;
mod loader;
mod log;
mod metrics;
mod mirroring;
mod positioners;
mod rectangle;
mod repeater;
mod scene;
mod text;
mod tree;

pub use accessible::{Accessible, AccessibleAttached};
pub use anchors::CATEGORY as ANCHORS_CATEGORY;
pub use animation::{
    Animation, AnimationController, Behavior, BehaviorTarget, ColorAnimation, Easing, EasingGroup,
    NumberAnimation, ParallelAnimation, PauseAnimation, PropertyAction, PropertyAnimation,
    RotationAnimation, ScriptAction, SequentialAnimation, SmoothedAnimation, easing,
    running_animation_count, set_animation_frame_period,
};
pub use input::{
    Modifiers, MouseArea, MouseButtons, MouseEvent, PointerEvent, PointerId, PointerKind,
    TapHandler, tap_handler_target,
};
pub use item::{
    AccessibleInfo, AnchorLine, AnchorProps, Anchors, Edge, Item, accessible, effective_margin,
    init_item, is_item, local_transform, scene_transform, set_accessible,
};
pub use layouts::{
    ColumnLayout, GridLayout, Layout, LayoutAttached, RowLayout, StackLayout, layout_runs,
};
pub use loader::Loader;
#[allow(deprecated)]
pub use log::{LogEntry, QuickLog};
pub use metrics::{FontMetrics, TextMetrics};
pub use mirroring::{LayoutMirroring, LayoutMirroringAttached};
pub use positioners::{Axis, Column, Flow, Grid, Positioner, PositionerAttached, Row};
pub use rectangle::{Rectangle, RectangleBorder, RectanglePaint, to_rgba};
pub use repeater::Repeater;
pub use scene::{
    DOUBLE_CLICK_INTERVAL, DRAG_THRESHOLD, FrameOutput, INCUBATION_BUDGET, MAX_POLISH_PASSES,
    POLISH_CATEGORY, QuickScene, SCENE_CATEGORY, TimerHook,
};
pub use text::{
    DEFAULT_PIXEL_SIZE, SharedTextEngine, Text, TextInputs, set_text_engine, text_engine,
    text_style,
};
pub use tree::{add_data, child_items, resources, set_parent_item, sync_parent};

/// The text engine's types, for hosts that configure fonts.
pub use ljos_text::{TextEngine, TextStyle};

use ljos_engine::Engine;

/// Registers module `QtQuick` (contract item 3).
///
/// Also keeps `Item.parent`, `children` and `data` in step with the
/// engine's visual parent, whoever changes it.
pub fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [
        Item, Rectangle, Text, MouseArea, TapHandler, Row, Column, Loader, MouseEvent,
        Accessible, Repeater,
        FontMetrics, TextMetrics,
        Grid, Flow, Positioner, LayoutMirroring,
    ]));
    engine.register_module(ljos_engine::module!("QtQuick.Layouts" 1.15 => [
        Layout, RowLayout, ColumnLayout, GridLayout, StackLayout,
    ]));
    mirroring::register(engine);
    positioners::register(engine);
    layouts::register(engine);
    accessible::register(engine);
    animation::register(engine);
    // Idempotent, so registering twice does no harm.
    engine.on_visual_parent_changed(|e, obj, parent| {
        if let Err(err) = tree::sync_parent(e, obj, parent) {
            e.log(
                ljos_engine::LogLevel::Warning,
                scene::SCENE_CATEGORY,
                format!("cannot follow a reparent: {err}"),
            );
        }
    });
}
