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
mod app_font;
mod application_palette;
mod by_type;
pub use app_font::application_font_changed;
mod canvas;
mod context_menu;
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
mod long_press;
mod metrics;
mod mirroring;
mod mouse_drag;
mod off_thread;
mod paint_item;
pub mod palette;
mod particles;
mod pinch_area;
#[doc(hidden)]
pub use particles::{emitters_looked_at, particle_reads, particles_painted};
mod frame_times;
mod pointer_item;
mod positioners;
mod press_and_hold;
mod rectangle;
mod rectangular_shadow;
mod repeater;
pub mod roots;
mod safe_area;
mod scene;
mod shader_effect;
mod shapes;
mod sprites;
mod states;
mod system_palette;
mod system_prefs;
mod text;
pub mod text_input;
pub mod touch;
pub mod transform;
mod tree;
mod views;
mod wheel;
mod wheel_handler;

pub use accessible::{Accessible, AccessibleAttached};
pub use accessible::{AccessibleAction, AccessibleItem, register_accessible_item};
pub use accessible::{AccessibleDefaults, register_accessible_defaults};
pub use anchors::CATEGORY as ANCHORS_CATEGORY;
pub use animation::SpringAnimation;
pub use animation::{AnchorAnimation, ParentAnimation};
pub use animation::{
    Animation, AnimationController, Behavior, BehaviorTarget, ColorAnimation, Easing, EasingGroup,
    FrameAnimation, NumberAnimation, ParallelAnimation, PauseAnimation, PropertyAction,
    PropertyAnimation, RotationAnimation, ScriptAction, SequentialAnimation, SmoothedAnimation,
    easing, running_animation_count, running_animations, set_animation_frame_period,
};
pub use animation::{
    Passage, Transition, run_passage, run_transition, run_transition_to, stop_transition,
};
pub use animation::{QuaternionAnimation, Vector3dAnimation};
pub use application_palette::{ApplicationPalette, application_palette, set_application_palette};
pub use context_menu::{ContextMenuHook, ContextMenuReason, register_context_menu};
pub use drag::{Drag, DragAttached, drag_attached, drag_type};
pub use drop_area::{
    DragActions, DragData, DragEvent, DropAction, DropActions, DropArea, DropAreaDrag,
    IGNORE_ACTION, PlatformDrag, PlatformDragKind, platform_drag, platform_drag_offered,
    platform_drag_target,
};
pub use error_boundary::{BoundaryFault, ErrorBoundary};
pub use flickable::Flickable;
pub use font_loader::{FontFamilies, FontLoader};
pub use frame_times::{FrameTimes, Woke};
pub use gradient::{Gradient, GradientPaint, GradientStop};
pub use handlers::{
    DragAxis, DragHandler, HoverHandler, PinchGesture, PinchHandler, PinchPhase, PointHandler,
    PointerHandler, grab_permission, grab_transition, snap_mode,
};
pub use image::{
    AnimatedImage, BorderImage, BorderImageBorder, Image, ImageBase, ImageKey, ImageLoad,
    ImagePaint, ImageReply, ImageStore, LoadError, Request as ImageRequest, fill_mode,
    image_status, kept_pixels, requested_size, tile_mode,
};
pub use image::{ImageTint, register_image_tint};
pub use image::{VectorImage, vector_fill_mode};
pub use incubation::{BASE_REFRESH_HZ, MIN_INCUBATION_BUDGET, incubation_budget};
pub use input::{HandlerPoint, WheelEventObject};
pub use input::{KeyEvent, KeyKind};
pub use input::{
    Modifiers, MouseArea, MouseButtons, MouseEvent, PointerEvent, PointerId, PointerKind,
    PointerSource, TapHandler, tap_handler_target,
};
pub use input::{ScrollPhase, WheelEvent};
pub use item::{
    AnchorLine, AnchorProps, Anchors, Edge, Item, ItemLayer, WindowPosition, effective_margin,
    init_item, is_item, local_transform, map_to_scene, register_window_position, scene_transform,
};
pub use layer::{
    LayerHooks, LayerTake, layer_host, layer_of, set_layer, set_layer_hooks, set_layer_take,
    tab_scope, window_item, window_of,
};
pub use layouts::{
    ColumnLayout, GridLayout, Layout, LayoutAttached, LayoutItemProxy, RowLayout, StackLayout,
    layout_runs,
};
pub use ljos_a11y::{ColorScheme, Contrast, DesktopSettings, HostPrefs, SettingValue, SystemPrefs};
pub use loader::Loader;
#[allow(deprecated)]
pub use metrics::{FontMetrics, TextMetrics};
pub use mirroring::{LayoutMirroring, LayoutMirroringAttached};
pub use paint_item::{ItemPaint, ReadPaint, register_paint_item, request_repaint};
pub use palette::{ColorGroup, Palette};
pub use pinch_area::{Pinch, PinchArea, PinchEvent};
pub use pointer_item::{
    HoverMove, PointerItem, contains_point, follows_implicit_size, give_size, is_mirrored,
    register_hover_move, register_pointer_item, takes_pointer,
};
pub use pointer_item::{PointerCondition, register_pointer_condition};
pub use positioners::{Column, Flow, Grid, Positioner, PositionerAttached, Row};
pub use rectangle::{Rectangle, RectangleBorder, RectanglePaint, to_rgba};
pub use repeater::Repeater;
pub use safe_area::{SafeArea, SafeAreaAttached, safe_area_insets, set_safe_area_insets};
pub use scene::{
    DOUBLE_CLICK_INTERVAL, DRAG_THRESHOLD, FrameOutput, INCUBATION_BUDGET, MAX_POLISH_PASSES,
    POLISH_CATEGORY, QuickScene, SCENE_CATEGORY, SceneMemory, TimerHook,
};
pub use shader_effect::{GridMesh, ShaderEffect, ShaderEffectSource};
pub use shapes::{Shape, ShapePaint, ShapePath};
pub use states::{
    AnchorChanges, AnchorChangesAnchors, Binding, ParentChange, PropertyChanges, State,
    StateChangeScript, StateGroup, defer_states, start_states,
};
pub use system_palette::{SystemPalette, color_group};
pub use system_prefs::{
    AccessibilityHints, HeldPrefs, PRESS_AND_HOLD_INTERVAL, START_DRAG_DISTANCE, StyleHints,
    color_scheme, contrast, font_scale, hold_system_prefs, press_and_hold_interval, reduced_motion,
    report_system_prefs, restore_system_prefs, screen_reader_active, screen_reader_told,
    set_screen_reader_from_platform, set_start_drag_distance, set_system_prefs,
    start_drag_distance, style_hints, system_prefs, update_system_prefs,
};
pub use text::{
    DEFAULT_PIXEL_SIZE, SharedTextEngine, Text, TextInputs, font_families, set_text_engine,
    set_text_engine_later, shown_text_style, text_engine, text_engine_in_use, text_style,
    texts_measured,
};
pub use text::{RichHandle, SelectionScope};
pub use text_input::{
    AccessibleTextAction, Clipboard, DoubleValidator, ImeState, IntValidator, PlatformTextInput,
    RegularExpressionValidator, TextEdit, TextEditor, TextInput,
};
pub use tree::{
    add_data, child_items, list_put_before, resources, set_parent_item, set_parent_item_before,
    sync_parent,
};
pub use views::table::{CellBlock, CellSelector, cell_block_rect, select_by, set_cell_selector};
pub use views::{
    DelegateModel, DelegateModelAttached, DelegateModelGroup, ObjectModel, drive_delegate_model,
    model_of_object, object_model_changed, object_model_rows,
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
        PinchArea, Pinch,
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
    // What a scene shows of an item goes with it, and a child made in
    // slices is shown once it is whole.
    engine.on_destroyed(scene::destroyed);
    engine.on_destroyed(views::delegate_groups::destroyed);
    engine.on_incubated(scene::incubated);
    // Idempotent, so registering twice does no harm.
    engine.on_visual_parent_changed(|e, obj, parent| {
        if let Err(err) = tree::sync_parent(e, obj, parent) {
            e.log(
                ljos_engine::LogLevel::Warning,
                scene::SCENE_CATEGORY,
                format!("cannot follow a reparent: {err}"),
            );
        }
        scene::reparented(e, obj);
    });
    // An object made under an item goes in it only when it is an item
    // itself, as its visual child, and in no property of the item's own:
    // not in a default property a component declares, nor in `resources`.
    // As Qt's `createObject` and `Qt.createQmlObject` do.
    engine.on_placed_under(|e, made, parent| {
        if !item::is_item(e, parent) {
            return false;
        }
        // The item's own `data`, which a Loader also hears of.
        if item::is_item(e, made)
            && let Err(err) = tree::add_data(e, parent, made)
        {
            e.log(
                ljos_engine::LogLevel::Warning,
                scene::SCENE_CATEGORY,
                format!("cannot place a made item: {err}"),
            );
        }
        true
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
    engine.register_module(ljos_engine::module!("QtQuick.VectorImage" 1.0 => [VectorImage]));
    shader_effect::register(engine);
    particles::register(engine);
    sprites::register(engine);
    handlers::register(engine);
    drag::register(engine);
    drop_area::register(engine);
    // FontLoader, SystemPalette, a Palette to give whole, Qt.fontFamilies,
    // and the platform's modules (QtQuick.Dialogs, QtCore) whose drawn
    // dialogs are items.
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [
        FontLoader, SystemPalette, Palette,
    ]));
    font_loader::register(engine);
    ljos_platform::register(engine);
}
