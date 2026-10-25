//! The numbers the rules hold an interface to. Each one names where it
//! comes from; `docs/design/` explains it. A check reads the number from
//! here and never writes its own, so tuning a limit is one edit.

/// Text against its background, WCAG 2.2 SC 1.4.3 (AA).
pub const TEXT_CONTRAST: f32 = 4.5;
/// Large text, WCAG 2.2 SC 1.4.3: at least 18 pt, or 14 pt bold.
pub const LARGE_TEXT_CONTRAST: f32 = 3.0;
/// 18 pt in logical pixels (at 96 per inch).
pub const LARGE_TEXT_PX: f32 = 24.0;
/// 14 pt bold in logical pixels.
pub const LARGE_BOLD_TEXT_PX: f32 = 18.66;
/// Weight from which text counts as bold.
pub const BOLD_WEIGHT: u16 = 700;
/// Boundaries, icons and state indicators, WCAG 2.2 SC 1.4.11.
pub const NONTEXT_CONTRAST: f32 = 3.0;

/// APCA lightness contrast, told beside the WCAG ratio as advice and
/// never failed on: body text, large or bold text, and non-text marks.
/// From the APCA readability guidance (WCAG 3 working draft).
pub const APCA_BODY_LC: f32 = 60.0;
pub const APCA_LARGE_LC: f32 = 45.0;
pub const APCA_NONTEXT_LC: f32 = 30.0;

/// The least opacity a popup, menu or dialog surface may have without a
/// blur behind it.
pub const SURFACE_MIN_ALPHA: f32 = 0.95;
/// The least OKLab distance between a floating surface and what is behind
/// it when it has neither a border nor a shadow.
pub const SURFACE_MIN_DELTA_E: f32 = 0.02;

/// Pointer targets in logical pixels: the WCAG 2.2 SC 2.5.8 floor on
/// desktop, and the touch size of Apple's and Material's guidelines.
pub const TARGET_MIN_DESKTOP: f32 = 24.0;
pub const TARGET_MIN_TOUCH: f32 = 44.0;

/// A popup whose content fills less than this share of it is too large.
pub const POPUP_MIN_FILL: f32 = 0.5;
/// Controls that start this many pixels apart or fewer, but not 0, read as
/// misaligned rather than placed apart on purpose.
pub const MISALIGN_MAX_PX: f32 = 8.0;
/// The spacing scale's unit.
pub const SPACING_UNIT_PX: f32 = 4.0;

/// Interface motion in milliseconds, by how often a user meets it. After
/// the frequency rule of animations.dev and the durations of Material 3
/// and Apple's guidelines.
pub mod motion {
    /// Done many times a day (selection, tab switch, focus): instant, or at
    /// most this.
    pub const FREQUENT_MAX_MS: u32 = 100;
    /// Small feedback (press, check, tooltip, dropdown).
    pub const MICRO_MAX_MS: u32 = 200;
    /// Panels, dialogs, drawers, page changes.
    pub const STANDARD_MAX_MS: u32 = 350;
    /// Rare moments that are allowed to be watched.
    pub const EMPHASIS_MAX_MS: u32 = 500;
    /// Shorter than this and not 0 reads as a flicker.
    pub const MIN_VISIBLE_MS: u32 = 60;
    /// One step of a stagger, and the whole stagger.
    pub const STAGGER_STEP_MAX_MS: u32 = 50;
    pub const STAGGER_TOTAL_MAX_MS: u32 = 200;
    /// An entrance that scales starts at or above this.
    pub const ENTER_SCALE_MIN: f32 = 0.5;
    /// The scale the guidelines recommend an entrance start from.
    pub const ENTER_SCALE_RECOMMENDED: f32 = 0.95;
    /// A spring for interface motion is damped at least this much (1 is
    /// critical damping: no overshoot).
    pub const SPRING_MIN_DAMPING_RATIO: f32 = 0.7;
    /// One frame at 60 Hz.
    pub const FRAME_BUDGET_MS: f32 = 16.7;
}

/// What the runtime audit looks at besides the window as it is.
pub mod audit {
    /// Font scales, up to the 200 % of WCAG 2.2 SC 1.4.4.
    pub const FONT_SCALES: &[f32] = &[1.0, 1.5, 2.0];
    /// Without a minimum size of its own, a window is also audited at this
    /// share of its width and of its height.
    pub const NARROW_FRACTION: f32 = 0.6;
    pub const SHORT_FRACTION: f32 = 0.6;
}

/// Content that moves this far or more after it was shown counts as a
/// layout shift.
pub const LAYOUT_SHIFT_MIN_PX: f32 = 4.0;
/// A menu with more items than this needs a separator or a submenu.
pub const MENU_MAX_UNGROUPED: usize = 10;
/// This many rows of one list with one label cannot be told apart.
pub const IDENTICAL_ROWS_MIN: usize = 3;
