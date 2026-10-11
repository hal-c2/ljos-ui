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
mod error_boundary;
pub mod flickable;
pub mod focus;
mod font_enums;
mod image;
mod incubation;
mod input;
mod item;
pub mod keys;
mod layer;
mod layouts;
mod loader;
mod log;
mod metrics;
mod mirroring;
mod pointer_item;
mod positioners;
mod rectangle;
mod repeater;
mod scene;
mod system_prefs;
mod text;
pub mod text_input;
mod tree;

pub use accessible::{Accessible, AccessibleAttached};
pub use accessible::{AccessibleAction, AccessibleItem, register_accessible_item};
pub use anchors::CATEGORY as ANCHORS_CATEGORY;
pub use animation::{
    Animation, AnimationController, Behavior, BehaviorTarget, ColorAnimation, Easing, EasingGroup,
    NumberAnimation, ParallelAnimation, PauseAnimation, PropertyAction, PropertyAnimation,
    RotationAnimation, ScriptAction, SequentialAnimation, SmoothedAnimation, easing,
    running_animation_count, set_animation_frame_period,
};
pub use error_boundary::{BoundaryFault, ErrorBoundary};
pub use flickable::Flickable;
pub use image::{
    BorderImage, BorderImageBorder, Image, ImageBase, ImageKey, ImagePaint, ImageStore, LoadError,
    Request as ImageRequest, fill_mode, image_status, requested_size, tile_mode,
};
pub use incubation::{BASE_REFRESH_HZ, MIN_INCUBATION_BUDGET, incubation_budget};
pub use input::{KeyEvent, KeyKind};
pub use input::{
    Modifiers, MouseArea, MouseButtons, MouseEvent, PointerEvent, PointerId, PointerKind,
    TapHandler, tap_handler_target,
};
pub use input::{ScrollPhase, WheelEvent};
pub use item::{
    AccessibleInfo, AnchorLine, AnchorProps, Anchors, Edge, Item, accessible, effective_margin,
    init_item, is_item, local_transform, scene_transform, set_accessible,
};
pub use layer::{
    LayerHooks, layer_host, layer_of, set_layer, set_layer_hooks, tab_scope, window_item, window_of,
};
pub use layouts::{
    ColumnLayout, GridLayout, Layout, LayoutAttached, RowLayout, StackLayout, layout_runs,
};
pub use loader::Loader;
#[allow(deprecated)]
pub use log::{LogEntry, QuickLog};
pub use metrics::{FontMetrics, TextMetrics};
pub use mirroring::{LayoutMirroring, LayoutMirroringAttached};
pub use pointer_item::{
    HoverMove, PointerItem, contains_point, follows_implicit_size, is_mirrored,
    register_hover_move, register_pointer_item,
};
pub use positioners::{Axis, Column, Flow, Grid, Positioner, PositionerAttached, Row};
pub use rectangle::{Rectangle, RectangleBorder, RectanglePaint, to_rgba};
pub use repeater::Repeater;
pub use scene::{
    DOUBLE_CLICK_INTERVAL, DRAG_THRESHOLD, FrameOutput, INCUBATION_BUDGET, MAX_POLISH_PASSES,
    POLISH_CATEGORY, QuickScene, SCENE_CATEGORY, TimerHook,
};
pub use system_prefs::{
    AccessibilityHints, StyleHints, color_scheme, contrast, font_scale, reduced_motion,
    screen_reader_active, set_system_prefs, system_prefs, update_system_prefs,
};
pub use text::{
    DEFAULT_PIXEL_SIZE, SharedTextEngine, Text, TextInputs, set_text_engine, text_engine,
    text_style,
};
pub use text_input::{
    AccessibleTextAction, Clipboard, DoubleValidator, ImeState, IntValidator, PlatformTextInput,
    RegularExpressionValidator, TextEdit, TextEditor, TextInput,
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
        Image, BorderImage,
        focus::FocusScope, focus::Window,
        keys::Keys, keys::KeyNavigation, keys::KeyEventObject, keys::Shortcut,
        keys::StandardKey,
    ]));
    engine.register_module(ljos_engine::module!("QtQuick.Layouts" 1.15 => [
        Layout, RowLayout, ColumnLayout, GridLayout, StackLayout,
    ]));
    mirroring::register(engine);
    positioners::register(engine);
    layouts::register(engine);
    accessible::register(engine);
    system_prefs::register(engine);
    animation::register(engine);
    font_enums::register(engine);
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
    // After the listener above: focus reads the lists it keeps.
    focus::register(engine);
    keys::register(engine);
    flickable::register(engine);
    text_input::register(engine);
    // Module `Ljos`, and Loader and ErrorBoundary as UI boundaries.
    error_boundary::register(engine);
    loader::register(engine);
}
