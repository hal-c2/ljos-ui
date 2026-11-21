//! What a host tells its application (ARCHITECTURE.md §10.1).
//!
//! Positions and sizes of input are in physical pixels of the window,
//! with the scale beside them: the DPI scale is never snapped, and the UI
//! layer divides once.

use crate::WindowId;

/// One thing that happened.
///
/// `R` is the platform's own window event and `U` what other threads post
/// into the loop.
#[derive(Debug)]
pub enum Event<'a, R, U> {
    /// Windows can be drawn into from here on: at the start, and when the
    /// application comes back from suspension. The UI host makes its
    /// surfaces now.
    SurfaceCreated,
    /// Nothing can be drawn until the next [`Event::SurfaceCreated`]: the
    /// UI host drops its surfaces now. The application lives on.
    SurfaceDestroyed,
    /// Something about one window. `raw` is the platform's event it was
    /// made from, when there is one.
    Window {
        id: WindowId,
        event: WindowEvent,
        raw: Option<&'a R>,
    },
    AppState(AppState),
    /// The system is short of memory: caches go.
    MemoryPressure,
    /// The application was asked to open this.
    OpenUrl(String),
    /// The user opened the application again while it runs: on macOS a
    /// click on its icon in the Dock. With no window shown it is the time
    /// to show one; the system does nothing more itself.
    Reopen {
        has_visible_windows: bool,
    },
    /// The process may be killed after this: write what must survive.
    SaveState,
    /// The time given to `HostControl::wake_at` has come.
    Wake,
    /// A message another thread posted.
    User(U),
    /// Everything that waited was delivered; the loop sleeps after this.
    AboutToWait,
    /// The loop ends; no event follows.
    Exiting,
}

/// How much of the application its user has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppState {
    /// A window of the application has the keyboard.
    Active,
    /// Visible, but none has.
    Inactive,
    /// Running without a visible window.
    Hidden,
    /// Not running until it is resumed.
    Suspended,
}

/// Room at the edges of a window that the application should keep clear,
/// in physical pixels: left, top, right, bottom.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub system_bars: [f64; 4],
    pub cutout: [f64; 4],
    /// The on-screen keyboard.
    pub ime: [f64; 4],
}

impl Insets {
    /// What the application keeps clear while it lays out at the height
    /// [`Insets::keyboard`] leaves it: the system's bars and the cutout,
    /// whichever reaches further at each edge. With the keyboard up the
    /// bottom is the keyboard's, which the window gives up already.
    #[must_use]
    pub fn safe_area(&self) -> [f64; 4] {
        let mut v = [0.0; 4];
        for (i, out) in v.iter_mut().enumerate() {
            *out = clear(self.system_bars[i]).max(clear(self.cutout[i]));
        }
        if self.keyboard() > 0.0 {
            v[3] = 0.0;
        }
        v
    }

    /// How much of the window's bottom the on-screen keyboard covers, which
    /// the application gives up as a window that resizes for it does
    /// (Android's `adjustResize`); 0 while it is down.
    #[must_use]
    pub fn keyboard(&self) -> f64 {
        clear(self.ime[3])
    }
}

fn clear(n: f64) -> f64 {
    if n.is_finite() { n.max(0.0) } else { 0.0 }
}

/// Where the system's back gesture is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackPhase {
    Started,
    Progressed,
    Cancelled,
    /// Go back.
    Invoked,
}

/// Something about one window.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    /// The size of what is drawn, in physical pixels, and the scale.
    Resized {
        px: [u32; 2],
        scale: f64,
    },
    /// Where the window is on the desktop, in physical pixels.
    Moved {
        x: i32,
        y: i32,
    },
    Insets(Insets),
    /// The window has the keyboard, or lost it.
    Focus(bool),
    /// The user asked for the window to be closed. Nothing has closed.
    CloseRequested,
    /// Draw the window now.
    RedrawRequested,
    /// The window cannot be seen at all, or can again.
    Occluded(bool),
    /// The system went to a dark or a light look.
    Theme {
        dark: bool,
    },
    Back {
        phase: BackPhase,
        progress: f32,
    },
    Text(TextEvent),
    Touch(TouchEvent),
    Pointer(PointerEvent),
    Key(KeyEvent),
    Modifiers(Modifiers),
    /// The window is gone.
    Destroyed,
    /// Something only the platform's own event tells.
    Other,
}

/// What an input method does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextEvent {
    /// An input method took over, or let go.
    Enabled(bool),
    /// The text being composed, and the selection inside it in bytes
    /// (`None` hides the cursor). An empty text ends the composition.
    Composing {
        text: String,
        selection: Option<(usize, usize)>,
    },
    /// Text to insert; it ends the composition.
    Commit(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchEvent {
    /// Which finger; the same from `Started` to `Ended`.
    pub id: u64,
    pub phase: TouchPhase,
    pub pos: [f64; 2],
    /// From 0 to 1 when the device measures it.
    pub force: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Pressed,
    Released,
}

/// How far a wheel or a touchpad scrolled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelDelta {
    /// Notches of a wheel.
    Lines([f64; 2]),
    /// Physical pixels.
    Pixels([f64; 2]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerKind {
    Moved,
    Button(PointerButton, ButtonState),
    Wheel(WheelDelta),
    Entered,
    Left,
}

/// A mouse, or what acts as one. `pos` is where the pointer was last
/// seen; `None` before it was seen at all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    pub kind: PointerKind,
    pub pos: Option<[f64; 2]>,
}

/// Which key, by what it means under the current layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    /// A key with a name: `"Enter"`, `"ArrowLeft"`, `"F5"` (the names of
    /// the W3C UI Events specification).
    Named(String),
    /// A key that types this.
    Character(String),
    Unidentified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    /// The key's place on the keyboard, whatever the layout (`"KeyA"`).
    pub code: Option<String>,
    pub state: ButtonState,
    pub repeat: bool,
    /// What the press types, when it types.
    pub text: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    /// The Windows key, or Command.
    pub meta: bool,
}

#[cfg(test)]
mod insets_tests {
    use super::Insets;

    #[test]
    fn the_safe_area_is_the_bars_or_the_cutout_whichever_reaches_further() {
        let i = Insets {
            system_bars: [0.0, 63.0, 0.0, 42.0],
            cutout: [0.0, 80.0, 30.0, 0.0],
            ime: [0.0; 4],
        };
        assert_eq!(i.safe_area(), [0.0, 80.0, 30.0, 42.0]);
        assert!(i.keyboard() == 0.0);
    }

    #[test]
    fn with_the_keyboard_up_the_bottom_is_the_keyboards() {
        let i = Insets {
            system_bars: [0.0, 63.0, 0.0, 42.0],
            cutout: [0.0; 4],
            ime: [0.0, 0.0, 0.0, 700.0],
        };
        assert_eq!(i.safe_area(), [0.0, 63.0, 0.0, 0.0]);
        assert!((i.keyboard() - 700.0).abs() < f64::EPSILON);
    }

    #[test]
    fn nonsense_is_nothing_to_keep_clear() {
        let i = Insets {
            system_bars: [-5.0, f64::NAN, f64::INFINITY, 0.0],
            cutout: [0.0; 4],
            ime: [0.0, 0.0, 0.0, -1.0],
        };
        assert_eq!(i.safe_area(), [0.0; 4]);
        assert!(i.keyboard() == 0.0);
    }
}
