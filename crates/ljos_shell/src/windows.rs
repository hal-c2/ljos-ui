//! What a QML `Window` wants ([`WindowSpec`]) said in the host's words
//! ([`WindowAttributes`], [`WindowChange`]).
//!
//! Nothing here touches a window: the functions say what to ask of the
//! host, so that they can be tested without a display.

use ljos_a11y::ColorScheme;
use ljos_controls::{
    FRAMELESS, KIND_MASK, STAYS_ON_BOTTOM, STAYS_ON_TOP, WindowBackdrop, WindowShow, WindowSpec,
};
use ljos_host::{
    Appearance, Backdrop, Capabilities, WindowAttributes, WindowChange, WindowKind, WindowLevel,
    WindowState,
};

/// `Qt.WindowDoesNotAcceptFocus`.
const NO_FOCUS: u32 = 0x0020_0000;

/// Stands for "no limit" on one axis when the other has one: a platform
/// takes a maximum size for both axes or for neither.
const UNLIMITED: f64 = 16_777_215.0;

/// Half a logical pixel: sizes closer than this are the same size, since
/// a platform rounds to its own pixels.
const SAME: f64 = 0.5;

/// What kind of window the type bits of `Qt.WindowFlags` name.
pub(crate) fn kind(flags: u32) -> WindowKind {
    match flags & KIND_MASK {
        // `Qt.Dialog`, and `Qt.Sheet` and `Qt.Drawer`, which are dialogs
        // that hang at their window where a platform has the idea.
        3 | 5 | 7 => WindowKind::Dialog,
        9 => WindowKind::Popup,
        11 => WindowKind::Tool,
        13 => WindowKind::ToolTip,
        15 => WindowKind::Splash,
        _ => WindowKind::Normal,
    }
}

pub(crate) fn level(flags: u32) -> WindowLevel {
    if flags & STAYS_ON_TOP != 0 {
        WindowLevel::AlwaysOnTop
    } else if flags & STAYS_ON_BOTTOM != 0 {
        WindowLevel::AlwaysOnBottom
    } else {
        WindowLevel::Normal
    }
}

/// Whether the window has a title bar and a border. Qt gives a popup, a
/// tool tip and a splash screen none.
pub(crate) fn decorated(flags: u32) -> bool {
    flags & FRAMELESS == 0
        && !matches!(
            kind(flags),
            WindowKind::Popup | WindowKind::ToolTip | WindowKind::Splash
        )
}

pub(crate) fn state(show: WindowShow) -> WindowState {
    match show {
        WindowShow::Windowed => WindowState::Normal,
        WindowShow::Minimized => WindowState::Minimized,
        WindowShow::Maximized => WindowState::Maximized,
        WindowShow::FullScreen => WindowState::FullScreen,
    }
}

/// The other way: what the platform shows said as a `Window.Visibility`.
pub(crate) fn show(state: WindowState) -> WindowShow {
    match state {
        WindowState::Normal => WindowShow::Windowed,
        WindowState::Minimized => WindowShow::Minimized,
        WindowState::Maximized => WindowShow::Maximized,
        WindowState::FullScreen => WindowShow::FullScreen,
    }
}

fn min_size(spec: &WindowSpec) -> Option<[f64; 2]> {
    (spec.min_size[0] > 0.0 || spec.min_size[1] > 0.0).then_some(spec.min_size)
}

fn max_size(spec: &WindowSpec) -> Option<[f64; 2]> {
    let [w, h] = spec.max_size;
    (w.is_some() || h.is_some()).then(|| [w.unwrap_or(UNLIMITED), h.unwrap_or(UNLIMITED)])
}

/// A window whose smallest and largest size are the same cannot be
/// resized by its user, which a platform shows in its frame.
fn resizable(spec: &WindowSpec) -> bool {
    let fixed = |axis: usize| {
        spec.max_size[axis].is_some_and(|max| max > 0.0 && max <= spec.min_size[axis])
    };
    !(fixed(0) && fixed(1))
}

/// The window's background lets through what is behind it.
fn backdrop(spec: &WindowSpec) -> Backdrop {
    match spec.backdrop {
        WindowBackdrop::None => Backdrop::None,
        WindowBackdrop::Blur => Backdrop::Blur,
    }
}

/// The scheme in effect, said rather than left to the system: the
/// application's own may differ from the system's.
fn appearance(spec: &WindowSpec) -> Appearance {
    match spec.color_scheme {
        ColorScheme::Light => Appearance::Light,
        ColorScheme::Dark => Appearance::Dark,
        ColorScheme::Unknown => Appearance::System,
    }
}

pub(crate) fn transparent(spec: &WindowSpec) -> bool {
    spec.color[3] < 255
}

/// A size the application gave; a window without one leaves it to the
/// platform.
fn asked_size(spec: &WindowSpec) -> Option<[f64; 2]> {
    (spec.size[0] > 0.0 && spec.size[1] > 0.0).then_some(spec.size)
}

fn same_size(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < SAME && (a[1] - b[1]).abs() < SAME
}

/// The window to open for `spec`. It is opened unseen: the shell shows it
/// when it has drawn into it, so that no empty window flashes.
pub(crate) fn attributes(spec: &WindowSpec, caps: Capabilities) -> WindowAttributes {
    let kind = kind(spec.flags);
    WindowAttributes {
        title: spec.title.clone(),
        size: asked_size(spec),
        position: spec.position.filter(|_| caps.window_position),
        min_size: min_size(spec),
        max_size: max_size(spec),
        visible: false,
        state: state(spec.show),
        decorations: decorated(spec.flags),
        resizable: resizable(spec),
        level: level(spec.flags),
        kind,
        transparent: transparent(spec),
        backdrop: backdrop(spec),
        appearance: appearance(spec),
        active: spec.flags & NO_FOCUS == 0 && kind != WindowKind::ToolTip,
        // The application's, not the window's: the shell says it.
        app_id: None,
    }
}

/// What can only be said when a window is opened changed: the shell opens
/// a new one in its place.
pub(crate) fn needs_new_window(old: &WindowSpec, new: &WindowSpec, caps: Capabilities) -> bool {
    transparent(old) != transparent(new)
        || (caps.window_kinds && kind(old.flags) != kind(new.flags))
}

/// What to tell an open window that was opened (or last told) for `old`
/// and now is to be `new`. The size is not in here: see [`size_request`].
pub(crate) fn changes(old: &WindowSpec, new: &WindowSpec, caps: Capabilities) -> Vec<WindowChange> {
    let mut out = Vec::new();
    if old.title != new.title {
        out.push(WindowChange::Title(new.title.clone()));
    }
    // Before the size and the state: a platform fits a window to its
    // limits when they are set.
    if min_size(old) != min_size(new) {
        out.push(WindowChange::MinSize(min_size(new)));
    }
    if max_size(old) != max_size(new) {
        out.push(WindowChange::MaxSize(max_size(new)));
    }
    if resizable(old) != resizable(new) {
        out.push(WindowChange::Resizable(resizable(new)));
    }
    if decorated(old.flags) != decorated(new.flags) {
        out.push(WindowChange::Decorations(decorated(new.flags)));
    }
    if level(old.flags) != level(new.flags) {
        out.push(WindowChange::Level(level(new.flags)));
    }
    // A scheme changed under no backdrop is told with the next one.
    if old.backdrop != new.backdrop
        || (new.backdrop != WindowBackdrop::None && appearance(old) != appearance(new))
    {
        out.push(WindowChange::Backdrop(backdrop(new), appearance(new)));
    }
    if old.show != new.show {
        out.push(WindowChange::State(state(new.show)));
    }
    if caps.window_position
        && old.position != new.position
        && let Some(at) = new.position
    {
        out.push(WindowChange::Position(at));
    }
    out
}

/// The size to ask the platform for: the one the application wrote, when
/// it wrote one since the window was last told and the platform's window
/// is not that large already. `platform` is the size the window has.
///
/// Without the first condition the platform's own resizes, which are
/// written to the window too, would be asked back of it.
pub(crate) fn size_request(
    old: &WindowSpec,
    new: &WindowSpec,
    platform: [f64; 2],
) -> Option<[f64; 2]> {
    let asked = asked_size(new)?;
    (!same_size(old.size, new.size) && !same_size(asked, platform)).then_some(asked)
}

#[cfg(test)]
mod tests {
    use ljos_controls::Modality;

    use super::*;

    fn spec() -> WindowSpec {
        WindowSpec {
            title: "Main".to_owned(),
            size: [640.0, 480.0],
            position: None,
            min_size: [0.0, 0.0],
            max_size: [None, None],
            visible: true,
            show: WindowShow::Windowed,
            flags: 1,
            modality: Modality::NonModal,
            color: [255; 4],
            opacity: 1.0,
            transient_parent: None,
            backdrop: WindowBackdrop::None,
            color_scheme: ColorScheme::Light,
        }
    }

    fn desktop() -> Capabilities {
        Capabilities {
            multi_window: true,
            window_position: true,
            window_kinds: true,
            ..Capabilities::default()
        }
    }

    fn wayland() -> Capabilities {
        Capabilities {
            multi_window: true,
            ..Capabilities::default()
        }
    }

    #[test]
    fn a_plain_window_is_decorated_resizable_and_opened_unseen() {
        let a = attributes(&spec(), desktop());
        assert_eq!(a.title, "Main");
        assert_eq!(a.size, Some([640.0, 480.0]));
        assert!(a.decorations && a.resizable && a.active);
        assert!(!a.visible, "shown by the shell after the first picture");
        assert_eq!(a.kind, WindowKind::Normal);
        assert_eq!(a.level, WindowLevel::Normal);
        assert_eq!((a.min_size, a.max_size, a.position), (None, None, None));
        assert!(!a.transparent);
    }

    #[test]
    fn a_window_without_a_size_leaves_it_to_the_platform() {
        let mut s = spec();
        s.size = [0.0, 0.0];
        assert_eq!(attributes(&s, desktop()).size, None);
        s.size = [300.0, 0.0];
        assert_eq!(attributes(&s, desktop()).size, None);
    }

    #[test]
    fn the_type_bits_of_the_flags_name_the_kind() {
        for (flags, k) in [
            (1, WindowKind::Normal),
            (3, WindowKind::Dialog),
            (9, WindowKind::Popup),
            (11, WindowKind::Tool),
            (13, WindowKind::ToolTip),
            (15, WindowKind::Splash),
            (3 | FRAMELESS, WindowKind::Dialog),
        ] {
            assert_eq!(kind(flags), k, "flags {flags:#x}");
        }
    }

    #[test]
    fn frameless_popups_tool_tips_and_splash_screens_have_no_frame() {
        assert!(decorated(1));
        assert!(decorated(3), "a dialog has a title bar");
        assert!(decorated(11), "a tool window has a title bar");
        for flags in [1 | FRAMELESS, 9, 13, 15] {
            assert!(!decorated(flags), "flags {flags:#x}");
        }
    }

    #[test]
    fn the_stacking_hints_name_the_level_and_on_top_wins() {
        assert_eq!(level(1), WindowLevel::Normal);
        assert_eq!(level(1 | STAYS_ON_TOP), WindowLevel::AlwaysOnTop);
        assert_eq!(level(1 | STAYS_ON_BOTTOM), WindowLevel::AlwaysOnBottom);
        assert_eq!(
            level(STAYS_ON_TOP | STAYS_ON_BOTTOM),
            WindowLevel::AlwaysOnTop
        );
    }

    #[test]
    fn a_window_that_refuses_the_keyboard_is_opened_without_it() {
        let mut s = spec();
        s.flags = 1 | NO_FOCUS;
        assert!(!attributes(&s, desktop()).active);
        s.flags = 13;
        assert!(!attributes(&s, desktop()).active, "a tool tip never types");
    }

    #[test]
    fn a_limit_on_one_axis_leaves_the_other_free() {
        let mut s = spec();
        s.min_size = [200.0, 0.0];
        s.max_size = [Some(900.0), None];
        let a = attributes(&s, desktop());
        assert_eq!(a.min_size, Some([200.0, 0.0]));
        assert_eq!(a.max_size, Some([900.0, UNLIMITED]));
        assert!(a.resizable);
    }

    #[test]
    fn a_window_of_one_size_only_is_not_resizable() {
        let mut s = spec();
        s.min_size = [640.0, 480.0];
        s.max_size = [Some(640.0), Some(480.0)];
        assert!(!attributes(&s, desktop()).resizable);
        // One free axis is enough to resize.
        s.max_size = [Some(640.0), None];
        assert!(attributes(&s, desktop()).resizable);
    }

    #[test]
    fn a_place_is_only_asked_for_where_windows_have_places() {
        let mut s = spec();
        s.position = Some([40.0, 60.0]);
        assert_eq!(attributes(&s, desktop()).position, Some([40.0, 60.0]));
        assert_eq!(attributes(&s, wayland()).position, None);
        let mut moved = s.clone();
        moved.position = Some([10.0, 10.0]);
        assert_eq!(
            changes(&s, &moved, desktop()),
            vec![WindowChange::Position([10.0, 10.0])]
        );
        assert_eq!(changes(&s, &moved, wayland()), vec![]);
    }

    #[test]
    fn a_translucent_colour_asks_for_a_transparent_window() {
        let mut s = spec();
        s.color = [0, 0, 0, 128];
        assert!(attributes(&s, desktop()).transparent);
    }

    #[test]
    fn a_backdrop_is_asked_for_when_the_window_opens_and_when_it_changes() {
        let mut blurred = spec();
        blurred.color = [0, 0, 0, 128];
        blurred.backdrop = WindowBackdrop::Blur;
        let attrs = attributes(&blurred, desktop());
        assert_eq!(attrs.backdrop, Backdrop::Blur);
        assert!(attrs.transparent);
        assert_eq!(attributes(&spec(), desktop()).backdrop, Backdrop::None);

        let mut plain = blurred.clone();
        plain.backdrop = WindowBackdrop::None;
        assert_eq!(
            changes(&blurred, &plain, desktop()),
            vec![WindowChange::Backdrop(Backdrop::None, Appearance::Light)]
        );
        assert!(!needs_new_window(&blurred, &plain, desktop()));
    }

    #[test]
    fn a_backdrop_is_told_the_scheme_it_is_shown_in() {
        let mut blurred = spec();
        blurred.color = [0, 0, 0, 0];
        blurred.backdrop = WindowBackdrop::Blur;
        blurred.color_scheme = ColorScheme::Dark;
        let attrs = attributes(&blurred, desktop());
        assert_eq!(
            (attrs.backdrop, attrs.appearance),
            (Backdrop::Blur, Appearance::Dark)
        );

        let mut light = blurred.clone();
        light.color_scheme = ColorScheme::Light;
        assert_eq!(
            changes(&blurred, &light, desktop()),
            vec![WindowChange::Backdrop(Backdrop::Blur, Appearance::Light)]
        );
        let mut unsaid = blurred.clone();
        unsaid.color_scheme = ColorScheme::Unknown;
        assert_eq!(
            changes(&blurred, &unsaid, desktop()),
            vec![WindowChange::Backdrop(Backdrop::Blur, Appearance::System)]
        );

        // Without a backdrop the scheme waits for one.
        let (mut a, mut b) = (spec(), spec());
        a.color_scheme = ColorScheme::Dark;
        b.color_scheme = ColorScheme::Light;
        assert_eq!(changes(&a, &b, desktop()), vec![]);
    }

    #[test]
    fn an_unchanged_window_is_told_nothing() {
        assert_eq!(changes(&spec(), &spec(), desktop()), vec![]);
    }

    #[test]
    fn each_change_is_told_once_and_limits_come_before_the_state() {
        let old = spec();
        let mut new = spec();
        new.title = "Other".to_owned();
        new.min_size = [100.0, 100.0];
        new.max_size = [Some(100.0), Some(100.0)];
        new.flags = 1 | FRAMELESS | STAYS_ON_TOP;
        new.show = WindowShow::Maximized;
        assert_eq!(
            changes(&old, &new, desktop()),
            vec![
                WindowChange::Title("Other".to_owned()),
                WindowChange::MinSize(Some([100.0, 100.0])),
                WindowChange::MaxSize(Some([100.0, 100.0])),
                WindowChange::Resizable(false),
                WindowChange::Decorations(false),
                WindowChange::Level(WindowLevel::AlwaysOnTop),
                WindowChange::State(WindowState::Maximized),
            ]
        );
    }

    #[test]
    fn a_limit_that_is_lifted_is_told_as_none() {
        let mut old = spec();
        old.min_size = [100.0, 100.0];
        old.max_size = [Some(500.0), Some(500.0)];
        assert_eq!(
            changes(&old, &spec(), desktop()),
            vec![WindowChange::MinSize(None), WindowChange::MaxSize(None)]
        );
    }

    #[test]
    fn a_size_the_application_wrote_is_asked_of_the_platform() {
        let old = spec();
        let mut new = spec();
        new.size = [800.0, 600.0];
        assert_eq!(
            size_request(&old, &new, [640.0, 480.0]),
            Some([800.0, 600.0])
        );
    }

    #[test]
    fn a_size_the_platform_gave_is_not_asked_back() {
        // The user resized: the window was told 700 by 500, which is what
        // the platform has.
        let old = spec();
        let mut new = spec();
        new.size = [700.0, 500.0];
        assert_eq!(size_request(&old, &new, [700.0, 500.0]), None);
        // Rounded to the platform's pixels it is still the same size.
        assert_eq!(size_request(&old, &new, [700.2, 499.8]), None);
    }

    #[test]
    fn an_unchanged_size_is_not_asked_again_when_the_platform_refused_it() {
        // A tiling desktop gave 1200 by 900 for the 640 by 480 asked.
        assert_eq!(size_request(&spec(), &spec(), [1200.0, 900.0]), None);
    }

    #[test]
    fn a_window_is_opened_anew_for_what_cannot_be_changed() {
        let old = spec();
        let mut dialog = spec();
        dialog.flags = 3;
        assert!(needs_new_window(&old, &dialog, desktop()));
        assert!(
            !needs_new_window(&old, &dialog, wayland()),
            "a platform without kinds of window has nothing to change"
        );
        let mut clear = spec();
        clear.color = [0, 0, 0, 0];
        assert!(needs_new_window(&old, &clear, wayland()));
        assert!(!needs_new_window(&old, &spec(), desktop()));
    }

    #[test]
    fn the_platform_state_reads_back_as_a_visibility() {
        for s in [
            WindowShow::Windowed,
            WindowShow::Minimized,
            WindowShow::Maximized,
            WindowShow::FullScreen,
        ] {
            assert_eq!(show(state(s)), s);
        }
    }
}
