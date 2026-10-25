//! Windows as an application describes them, and what a platform says
//! about itself.

use std::sync::Arc;

/// A window of a host. Ids are never reused within a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub u64);

/// Whether a window is shown as it is, as an icon, or over the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowState {
    #[default]
    Normal,
    Minimized,
    Maximized,
    FullScreen,
}

/// Where a window is stacked among the others.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowLevel {
    #[default]
    Normal,
    AlwaysOnTop,
    AlwaysOnBottom,
}

/// What a window is for. A hint: only some window systems have the idea.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowKind {
    #[default]
    Normal,
    Dialog,
    /// A palette of tools beside the window it belongs to.
    Tool,
    Popup,
    ToolTip,
    Splash,
}

/// What the window system draws behind a window, seen through what is
/// transparent in it. A request: a window system without the effect
/// draws nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backdrop {
    #[default]
    None,
    /// What is behind the window, blurred.
    Blur,
    /// The window system's own glass (macOS 26's); the blur where it has
    /// none.
    Glass,
}

/// Light or dark: which of its materials the window system puts behind a
/// window ([`Backdrop`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    /// The system's, following it as it changes.
    #[default]
    System,
    Light,
    Dark,
}

/// A picture the desktop shows for a window or the application: straight
/// (not premultiplied) RGBA, `width` × `height` pixels row by row from the
/// top. Cloning it shares the pixels.
#[derive(Clone, PartialEq, Eq)]
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

impl std::fmt::Debug for Icon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Icon")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// Everything about a window that is decided when it is opened. Sizes and
/// positions are in logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowAttributes {
    pub title: String,
    /// The size of what is drawn, without the frame; `None` leaves it to
    /// the platform.
    pub size: Option<[f64; 2]>,
    /// Of the frame's top left corner on the desktop; `None` leaves it to
    /// the platform.
    pub position: Option<[f64; 2]>,
    pub min_size: Option<[f64; 2]>,
    pub max_size: Option<[f64; 2]>,
    pub visible: bool,
    pub state: WindowState,
    /// The title bar and the border.
    pub decorations: bool,
    pub resizable: bool,
    pub level: WindowLevel,
    pub kind: WindowKind,
    /// The window's background may show what is behind it.
    pub transparent: bool,
    pub backdrop: Backdrop,
    pub appearance: Appearance,
    /// Takes the keyboard when it is shown.
    pub active: bool,
    /// The application the window belongs to, as the desktop names it
    /// (the app id on Wayland, the class on X11): what its rules, its
    /// task bar and its icon lookup go by. Platforms that group windows
    /// by process ignore it.
    pub app_id: Option<String>,
    /// What the desktop shows for the window (its title bar, the task
    /// bar, a window switcher); `None` leaves it to the desktop, which
    /// goes by [`WindowAttributes::app_id`].
    pub icon: Option<Icon>,
}

impl Default for WindowAttributes {
    fn default() -> Self {
        Self {
            title: String::new(),
            size: None,
            position: None,
            min_size: None,
            max_size: None,
            visible: true,
            state: WindowState::Normal,
            decorations: true,
            resizable: true,
            level: WindowLevel::Normal,
            kind: WindowKind::Normal,
            transparent: false,
            backdrop: Backdrop::None,
            appearance: Appearance::System,
            active: true,
            app_id: None,
            icon: None,
        }
    }
}

/// One change to an open window.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowChange {
    Title(String),
    Size([f64; 2]),
    Position([f64; 2]),
    MinSize(Option<[f64; 2]>),
    MaxSize(Option<[f64; 2]>),
    Visible(bool),
    State(WindowState),
    Decorations(bool),
    Resizable(bool),
    Level(WindowLevel),
    /// The backdrop and its appearance, told together: a platform picks
    /// its material from both.
    Backdrop(Backdrop, Appearance),
    /// Take the keyboard.
    Activate,
    /// Ask for the user's attention without taking the keyboard.
    Alert,
    /// Let the user move the window with the pointer that is pressed now;
    /// the platform ends the move when the button is released.
    StartMove,
    /// The same for resizing the window by an edge or a corner.
    StartResize(ResizeEdge),
    /// What the desktop shows for the window; `None` gives it back the
    /// desktop's.
    Icon(Option<Icon>),
}

/// An edge or a corner of a window, by the compass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

/// A display.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Screen {
    pub name: String,
    /// In physical pixels.
    pub size: [u32; 2],
    /// Of its top left corner among the displays, in physical pixels.
    pub position: [i32; 2],
    /// Physical pixels per logical pixel. Never snapped.
    pub scale: f64,
    /// Pictures a second; `None` when the platform does not say.
    pub refresh_hz: Option<f64>,
}

impl Screen {
    /// The size in logical pixels.
    pub fn logical_size(&self) -> [f64; 2] {
        let s = if self.scale > 0.0 { self.scale } else { 1.0 };
        [f64::from(self.size[0]) / s, f64::from(self.size[1]) / s]
    }
}

/// What a platform can do. An application reads this before it promises
/// something to its user; asking for what is missing does nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// More than one window at a time.
    pub multi_window: bool,
    /// A window can be put at a place on the desktop and says where it is.
    /// Wayland has neither.
    pub window_position: bool,
    /// A window can ask for the keyboard by itself.
    pub activation: bool,
    /// Windows can be kept above or below the others.
    pub window_levels: bool,
    /// The platform knows dialogs, tool windows and the like apart.
    pub window_kinds: bool,
    /// A window can keep its user out of the application's other windows.
    /// No host built so far can: the application does it.
    pub modal_windows: bool,
    /// An input method composes text ([`crate::TextEvent`]).
    pub ime: bool,
    pub touch: bool,
    /// Safe-area insets are reported ([`crate::WindowEvent::Insets`]).
    pub insets: bool,
    /// The system's back gesture is reported ([`crate::WindowEvent::Back`]).
    pub back: bool,
    /// Files dragged in from other applications reach the window, in its
    /// raw events. winit 0.30 has no drag-and-drop on Wayland; on X11,
    /// XWayland included, it has.
    pub file_drops: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screen_gives_its_size_in_logical_pixels() {
        let s = Screen {
            size: [3000, 2000],
            scale: 1.5,
            ..Screen::default()
        };
        assert_eq!(s.logical_size(), [2000.0, 2000.0 / 1.5]);
    }

    #[test]
    fn a_screen_without_a_scale_is_not_divided_by_zero() {
        let s = Screen {
            size: [800, 600],
            scale: 0.0,
            ..Screen::default()
        };
        assert_eq!(s.logical_size(), [800.0, 600.0]);
    }

    #[test]
    fn a_window_is_shown_decorated_and_resizable_unless_told() {
        let a = WindowAttributes::default();
        assert!(a.visible && a.decorations && a.resizable && a.active);
        assert_eq!(a.state, WindowState::Normal);
        assert_eq!(a.level, WindowLevel::Normal);
    }
}
