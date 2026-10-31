//! The events a hook is given: the desktop's shapes, from what the runtime
//! delivers, whose numbers are already Qt's.

use super::kurbo;

macro_rules! bits {
    ($name:ident { $($key:ident = $value:expr),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
        pub struct $name(u32);

        impl $name {
            $(pub const $key: Self = Self($value);)*
            pub const fn empty() -> Self {
                Self(0)
            }
            pub const fn from_bits_truncate(bits: u32) -> Self {
                Self(bits)
            }
            pub const fn bits(self) -> u32 {
                self.0
            }
            pub const fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }
            pub const fn is_empty(self) -> bool {
                self.0 == 0
            }
        }
    };
}

bits!(Modifiers {
    SHIFT = 0x0200_0000,
    CONTROL = 0x0400_0000,
    ALT = 0x0800_0000,
    META = 0x1000_0000,
    KEYPAD = 0x2000_0000,
});

bits!(MouseButtons {
    LEFT = 0x1,
    RIGHT = 0x2,
    MIDDLE = 0x4,
});

/// Milliseconds of the page's clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Time(u64);

impl Time {
    pub const ZERO: Self = Self(0);
    pub const fn from_millis(ms: u64) -> Self {
        Self(ms)
    }
    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Press,
    Release,
    Move,
    Cancel,
    Leave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct PointerId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerSource {
    pub device: i32,
    pub pointer: i32,
}

impl PointerSource {
    pub const MOUSE: Self = Self {
        device: 0x1,
        pointer: 0x1,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    pub kind: PointerKind,
    pub pos: kurbo::Point,
    pub button: Option<MouseButtons>,
    pub buttons: MouseButtons,
    pub modifiers: Modifiers,
    pub id: PointerId,
    pub source: PointerSource,
    pub time: Time,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScrollPhase {
    #[default]
    NoPhase,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelEvent {
    pub pos: kurbo::Point,
    pub angle_delta: kurbo::Vec2,
    pub pixel_delta: kurbo::Vec2,
    pub phase: ScrollPhase,
    pub modifiers: Modifiers,
    pub time: Time,
}

impl WheelEvent {
    /// The angle of one notch of a wheel, as natively.
    pub const NOTCH: f64 = 120.0;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    Press,
    Release,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub kind: KeyKind,
    pub key: i32,
    pub modifiers: Modifiers,
    pub text: String,
    pub auto_repeat: bool,
    pub count: u32,
    pub native_scan_code: u32,
}
