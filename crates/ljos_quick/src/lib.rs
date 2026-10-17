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
mod canvas;
mod drag;
mod drop_area;
mod effects;
pub mod enums;
mod error_boundary;
pub mod flickable;
mod flipable;
pub mod focus;
mod font_enums;
mod font_loader;
mod gradient;
mod handlers;
mod image;
mod incubation;
mod input;
mod item;
pub mod keys;
mod layer;
mod layouts;
mod loader;
mod log;
mod long_press;
mod metrics;
mod mirroring;
mod mouse_drag;
mod paint_item;
mod particles;
mod pointer_item;
mod positioners;
mod press_and_hold;
mod rectangle;
mod repeater;
mod safe_area;
mod scene;
mod shapes;
mod sprites;
mod states;
mod system_palette;
mod system_prefs;
mod text;
pub mod text_input;
pub mod transform;
mod tree;
mod views;
mod wheel;
mod wheel_handler;

pub use accessible::{Accessible, AccessibleAttached};
pub use accessible::{AccessibleAction, AccessibleItem, register_accessible_item};
pub use anchors::CATEGORY as ANCHORS_CATEGORY;
pub use animation::SpringAnimation;
pub use animation::{AnchorAnimation, ParentAnimation};
pub use animation::{
    Animation, AnimationController, Behavior, BehaviorTarget, ColorAnimation, Easing, EasingGroup,
    NumberAnimation, ParallelAnimation, PauseAnimation, PropertyAction, PropertyAnimation,
    RotationAnimation, ScriptAction, SequentialAnimation, SmoothedAnimation, easing,
    running_animation_count, set_animation_frame_period,
};
pub use animation::{Transition, run_transition, stop_transition};
pub use drag::{Drag, DragAttached, drag_attached, drag_type};
pub use drop_area::{
    DragData, DragEvent, DropArea, DropAreaDrag, PlatformDrag, PlatformDragKind, platform_drag,
};
pub use error_boundary::{BoundaryFault, ErrorBoundary};
pub use flickable::Flickable;
pub use font_loader::{FontFamilies, FontLoader};
pub use gradient::{Gradient, GradientPaint, GradientStop};
pub use handlers::{
    DragAxis, DragHandler, HoverHandler, PinchGesture, PinchHandler, PinchPhase, PointHandler,
    PointerHandler, grab_permission, grab_transition, snap_mode,
};
pub use image::{
    AnimatedImage, BorderImage, BorderImageBorder, Image, ImageBase, ImageKey, ImagePaint,
    ImageReply, ImageStore, LoadError, Request as ImageRequest, fill_mode, image_status,
    requested_size, tile_mode,
};
pub use image::{ImageTint, register_image_tint};
pub use incubation::{BASE_REFRESH_HZ, MIN_INCUBATION_BUDGET, incubation_budget};
pub use input::{HandlerPoint, WheelEventObject};
pub use input::{KeyEvent, KeyKind};
pub use input::{
    Modifiers, MouseArea, MouseButtons, MouseEvent, PointerEvent, PointerId, PointerKind,
    TapHandler, tap_handler_target,
};
pub use input::{ScrollPhase, WheelEvent};
pub use item::{
    AccessibleInfo, AnchorLine, AnchorProps, Anchors, Edge, Item, WindowPosition, accessible,
    effective_margin, init_item, is_item, local_transform, register_window_position,
    scene_transform, set_accessible,
};
pub use layer::{
    LayerHooks, LayerTake, layer_host, layer_of, set_layer, set_layer_hooks, set_layer_take,
    tab_scope, window_item, window_of,
};
pub use layouts::{
    ColumnLayout, GridLayout, Layout, LayoutAttached, LayoutItemProxy, RowLayout, StackLayout,
    layout_runs,
};
pub use ljos_a11y::ColorScheme;
pub use loader::Loader;
#[allow(deprecated)]
pub use log::{LogEntry, QuickLog};
pub use metrics::{FontMetrics, TextMetrics};
pub use mirroring::{LayoutMirroring, LayoutMirroringAttached};
pub use paint_item::{ItemPaint, ReadPaint, register_paint_item};
pub use pointer_item::{
    HoverMove, PointerItem, contains_point, follows_implicit_size, give_size, is_mirrored,
    register_hover_move, register_pointer_item,
};
pub use pointer_item::{PointerCondition, register_pointer_condition};
pub use positioners::{Axis, Column, Flow, Grid, Positioner, PositionerAttached, Row};
pub use rectangle::{Rectangle, RectangleBorder, RectanglePaint, to_rgba};
pub use repeater::Repeater;
pub use safe_area::{SafeArea, SafeAreaAttached, safe_area_insets, set_safe_area_insets};
pub use scene::{
    DOUBLE_CLICK_INTERVAL, DRAG_THRESHOLD, FrameOutput, INCUBATION_BUDGET, MAX_POLISH_PASSES,
    POLISH_CATEGORY, QuickScene, SCENE_CATEGORY, TimerHook,
};
pub use shapes::{Shape, ShapePaint, ShapePath};
pub use states::{
    AnchorChanges, AnchorChangesAnchors, Binding, ParentChange, PropertyChanges, State,
    StateChangeScript, StateGroup, defer_states, start_states,
};
pub use system_palette::{SystemPalette, color_group};
pub use system_prefs::{
    AccessibilityHints, PRESS_AND_HOLD_INTERVAL, START_DRAG_DISTANCE, StyleHints, color_scheme,
    contrast, font_scale, press_and_hold_interval, reduced_motion, screen_reader_active,
    set_start_drag_distance, set_system_prefs, start_drag_distance, style_hints, system_prefs,
    update_system_prefs,
};
pub use text::{
    DEFAULT_PIXEL_SIZE, SharedTextEngine, Text, TextInputs, font_families, set_text_engine,
    text_engine, text_style,
};
pub use text::{RichHandle, SelectionScope};
pub use text_input::{
    AccessibleTextAction, Clipboard, DoubleValidator, ImeState, IntValidator, PlatformTextInput,
    RegularExpressionValidator, TextEdit, TextEditor, TextInput,
};
pub use tree::{add_data, child_items, resources, set_parent_item, sync_parent};
pub use views::{
    DelegateModel, DelegateModelAttached, DelegateModelGroup, ObjectModel, drive_delegate_model,
    object_model_changed, object_model_rows,
};
pub use views::{
    GridView, ItemView, ListView, TableView, TableViewAttached, TreeView, ViewAttached,
    ViewSection, ViewTransition, ViewTransitionAttached,
};
pub use views::{PathView, PathViewAttached, path_view};
pub use views::{view_index_of, view_item_at, view_layout_passes};
pub use wheel_handler::WheelHandler;

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
        Grid, Flow, Positioner, LayoutMirroring, SafeArea,
        Image, BorderImage, AnimatedImage,
        Gradient, GradientStop,
        focus::FocusScope, focus::Window,
        keys::Keys, keys::KeyNavigation, keys::KeyEventObject, keys::Shortcut,
        keys::StandardKey,
        WheelEventObject, WheelHandler,
    ]));
    // A file that imports `QtQuick` has `Timer` and `QtObject` too.
    engine.register_import("QtQuick", "QtQml");
    engine.register_module(ljos_engine::module!("QtQuick.Layouts" 1.15 => [
        Layout, RowLayout, ColumnLayout, GridLayout, StackLayout, LayoutItemProxy,
    ]));
    mirroring::register(engine);
    safe_area::register(engine);
    positioners::register(engine);
    layouts::register(engine);
    accessible::register(engine);
    system_prefs::register(engine);
    animation::register(engine);
    states::register(engine);
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
    views::register(engine);
    text_input::register(engine);
    text::register(engine);
    // Module `Ljos`, and Loader and ErrorBoundary as UI boundaries.
    error_boundary::register(engine);
    loader::register(engine);
    shapes::register(engine);
    transform::register(engine);
    flipable::register(engine);
    canvas::register(engine);
    effects::register(engine);
    particles::register(engine);
    sprites::register(engine);
    handlers::register(engine);
    drag::register(engine);
    drop_area::register(engine);
    // FontLoader, SystemPalette, Qt.fontFamilies, and the platform's
    // modules (QtQuick.Dialogs, QtCore) whose drawn dialogs are items.
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [
        FontLoader, SystemPalette,
    ]));
    font_loader::register(engine);
    ljos_platform::register(engine);
}
