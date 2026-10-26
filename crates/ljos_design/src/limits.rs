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
/// The smallest text in logical pixels: the smallest of Apple's minimum
/// text sizes (10 pt on macOS; 11 pt on iOS, as Material's smallest style
/// is 11 px), so anything below is too small on every platform.
pub const TEXT_MIN_PX: f32 = 10.0;
/// Boundaries, icons and state indicators, WCAG 2.2 SC 1.4.11.
pub const NONTEXT_CONTRAST: f32 = 3.0;

/// APCA lightness contrast, told beside the WCAG ratio as advice and
/// never failed on: body text, large or bold text, and non-text marks.
/// From "APCA in a Nutshell" (git.apcacontrast.com): Lc 75 is the least for
/// columns of body text (Lc 60 is for short content text that is not body
/// text), Lc 45 for headlines and large text, Lc 30 for solid icons and
/// placeholder text.
pub const APCA_BODY_LC: f32 = 75.0;
pub const APCA_LARGE_LC: f32 = 45.0;
pub const APCA_NONTEXT_LC: f32 = 30.0;

/// The least opacity a popup, menu or dialog surface may have without a
/// blur behind it. Ljós's own number: no guideline publishes one; at 0.95 what
/// is behind shows through at a twentieth, too faint to read as text.
pub const SURFACE_MIN_ALPHA: f32 = 0.95;
/// The least OKLab distance between a floating surface and what is behind
/// it when it has neither a border nor a shadow: about one just noticeable
/// difference in OKLab.
pub const SURFACE_MIN_DELTA_E: f32 = 0.02;

/// Pointer targets in logical pixels: the WCAG 2.2 SC 2.5.8 floor on
/// desktop, and Apple's default touch size (44 pt; Material asks 48 dp).
pub const TARGET_MIN_DESKTOP: f32 = 24.0;
pub const TARGET_MIN_TOUCH: f32 = 44.0;

/// A popup whose content fills less than this share of it is too large.
/// Ljós's own heuristic, like the two numbers below it.
pub const POPUP_MIN_FILL: f32 = 0.5;
/// Controls that start this many pixels apart or fewer, but not 0, read as
/// misaligned rather than placed apart on purpose.
pub const MISALIGN_MAX_PX: f32 = 8.0;
/// The spacing scale's unit: Material's 4 dp grid.
pub const SPACING_UNIT_PX: f32 = 4.0;

/// Interface motion in milliseconds, by how often a user meets it. After
/// the frequency rule of animations.dev and the durations of Material 3
/// and Apple's guidelines.
pub mod motion {
    /// Done many times a day (selection, tab switch, focus): instant, or at
    /// most this.
    pub const FREQUENT_MAX_MS: u32 = 100;
    /// Small feedback (press, check, toggle, tooltip).
    pub const MICRO_MAX_MS: u32 = 200;
    /// Menus and dropdowns (150 to 250 at animations.dev), panels,
    /// dialogs, drawers, page changes.
    pub const STANDARD_MAX_MS: u32 = 350;
    /// Rare moments that are allowed to be watched.
    pub const EMPHASIS_MAX_MS: u32 = 500;
    /// Shorter than this and not 0 reads as a flicker: Material 3's
    /// shortest duration token (`short1`).
    pub const MIN_VISIBLE_MS: u32 = 50;
    /// One step of a stagger (30 to 50 at ui-ux-pro-max; 30 to 80 at
    /// animations.dev), and the whole stagger: what is left of the 400 ms
    /// Doherty threshold once the last item's own micro animation has run.
    pub const STAGGER_STEP_MAX_MS: u32 = 50;
    pub const STAGGER_TOTAL_MAX_MS: u32 = 200;
    /// An entrance that scales starts at or above this. Lower than any
    /// guideline (0.8 to 0.97) so that a finding is certain.
    pub const ENTER_SCALE_MIN: f32 = 0.5;
    /// The scale the guidelines recommend an entrance start from.
    pub const ENTER_SCALE_RECOMMENDED: f32 = 0.95;
    /// A spring for interface motion is damped at least this much (1 is
    /// critical damping: no overshoot). SwiftUI's bounce is one less the
    /// ratio, and animations.dev keeps bounce within 0.1 to 0.3.
    pub const SPRING_MIN_DAMPING_RATIO: f32 = 0.7;
    /// One frame at 60 Hz.
    pub const FRAME_BUDGET_MS: f32 = 16.7;
    /// With reduced motion a fade or a change of colour is kept, as WCAG
    /// 2.2 SC 2.3.3's Understanding page, Apple's Reduce Motion and web.dev
    /// replace movement with a dissolve, and is at most this long: Material
    /// 3's `short3` duration token, the short end of animations.dev's menus,
    /// long enough to be seen (`MIN_VISIBLE_MS`) and quicker than any
    /// feedback (`MICRO_MAX_MS`).
    pub const REDUCED_FADE_MAX_MS: u32 = 150;
    /// One flash of anything is a pass out and back; three a second is the
    /// most WCAG 2.2 SC 2.3.1 allows, so a pass shorter than this flashes
    /// more often.
    pub const FLASH_MIN_PASS_MS: u32 = 333;
    /// Moving or changing content that starts by itself may run this long
    /// before it needs a way to pause or stop (WCAG 2.2 SC 2.2.2).
    pub const ENDLESS_MAX_MS: u32 = 5000;
    /// How far along an entrance is at half its time, at most, for its
    /// curve to be an ease-in: a quadratic ease-in is at 0.25 and a cubic
    /// at 0.125; linear and in-out curves are at 0.5, ease-out further.
    /// Between the two, so that neither a sampled frame nor a mild in-out
    /// curve tips it.
    pub const EASE_IN_HALFWAY_MAX: f32 = 0.35;
    /// Switching between light and dark is done every day, often by the
    /// system at dusk: instant, or as quick as anything frequent.
    pub const THEME_SWITCH_MAX_MS: u32 = FREQUENT_MAX_MS;
    /// A colour on the way from one theme to the other that is lighter or
    /// darker than both ends by more than this OKLab lightness is a flash:
    /// a tenth of the way from black to white, well past what an
    /// interpolation between the ends can wander (none, in OKLab; a little
    /// in sRGB).
    pub const THEME_FLASH_MIN_L: f32 = 0.1;
}

/// What the runtime audit looks at besides the window as it is.
pub mod audit {
    /// Font scales, up to the 200 % of WCAG 2.2 SC 1.4.4.
    pub const FONT_SCALES: &[f32] = &[1.0, 1.5, 2.0];
    /// Without a minimum size of its own, a window is also audited at this
    /// share of its width and of its height.
    pub const NARROW_FRACTION: f32 = 0.6;
    pub const SHORT_FRACTION: f32 = 0.6;
    /// What something reaches past, or comes short by, before it counts:
    /// less is the rounding of a layout to whole pixels.
    pub const SLACK_PX: f32 = 1.0;
    /// Something to operate counts as covered when another item takes
    /// this share of its visible area or more.
    pub const OCCLUDED_SHARE: f32 = 0.5;
    /// How long the audit lets an animation it started run before it
    /// looks: twice the longest an interface animation may take
    /// (`motion::EMPHASIS_MAX_MS`), so whatever keeps to the limits has
    /// ended, and what has not is too long.
    pub const ANIMATION_WAIT_MS: u32 = 1000;
    /// How far past a focused item's box its focus indicator is looked
    /// for: a ring drawn outside the control (Fusion's, Material's) lies
    /// within this.
    pub const FOCUS_MARGIN_PX: f32 = 4.0;
    /// A pixel counts as changed when its colour moves at least this far
    /// in OKLab: about one just noticeable difference, as
    /// `SURFACE_MIN_DELTA_E` has it.
    pub const VISIBLE_DELTA_E: f32 = super::SURFACE_MIN_DELTA_E;
    /// Fewer changed pixels than this are no visible change: anti-aliasing
    /// moving by a pixel or two at an edge. A one-pixel caret or ring
    /// along one side of the smallest target is many more.
    pub const VISIBLE_CHANGE_MIN_PX: usize = 8;
}

/// Content that moves this far or more after it was shown counts as a
/// layout shift.
pub const LAYOUT_SHIFT_MIN_PX: f32 = 4.0;
/// What moves this soon after the user's input is that input's doing
/// (a panel they open sliding into place), not a layout shift: the
/// window Cumulative Layout Shift gives a recent input.
pub const LAYOUT_SHIFT_INPUT_GRACE_MS: u32 = 500;
/// A menu with more items than this needs a separator or a submenu.
pub const MENU_MAX_UNGROUPED: usize = 10;
/// This many rows of one list with one label cannot be told apart.
pub const IDENTICAL_ROWS_MIN: usize = 3;

// Names, layout and content read off the live tree (runtime audit, batch 2b).

/// A frameless window's own controls lie at most this far from its top
/// corner: one desktop target (`TARGET_MIN_DESKTOP`), so the button a
/// pointer flung into the corner meets is the window's own.
pub const WINDOW_CONTROLS_MAX_INSET_PX: f32 = 24.0;
/// The width WCAG 2.2 SC 1.4.10 asks content to reflow at without
/// scrolling both ways: 320 CSS px, a logical pixel here.
pub const REFLOW_WIDTH_PX: f32 = 320.0;
/// A paragraph's lines run at most this many characters: WCAG 2.2 SC
/// 1.4.8 asks for 80 at most (typographers aim for 45 to 75).
pub const LINE_LENGTH_MAX_CHARS: usize = 80;
/// A paragraph is this many lines or more; a shorter text is a label or a
/// caption, whose line length is no matter.
pub const LINE_LENGTH_MIN_LINES: usize = 3;
/// A hexadecimal run at least this long, with letters and digits both,
/// is a hash or an address rather than a word or a number.
pub const HASH_MIN_CHARS: usize = 12;
/// An empty view smaller than this along either side is a strip (a bar,
/// a row of chips), not a place a user looks for content.
pub const EMPTY_VIEW_MIN_PX: f32 = 48.0;

/// What colour says of meaning and emphasis (`color-semantic-hue`,
/// `color-accent-weight`).
pub mod hue {
    /// A colour has a hue worth reading only with at least this OKLCH
    /// chroma; below it, it reads as a grey (a tinted surface, a neutral
    /// text colour).
    pub const MIN_CHROMA: f32 = 0.06;
    /// The OKLCH hues, in degrees, read as red: danger, error, destruction
    /// (ISO 3864-1's safety red; Apple's destructive role and Material 3's
    /// error colour are red). The band wraps past 360.
    pub const RED: (f32, f32) = (345.0, 45.0);
    /// The OKLCH hues read as green: safe, done, success (ISO 3864-1's
    /// safe condition green; Material and Apple's success green).
    pub const GREEN: (f32, f32) = (120.0, 175.0);
    /// At most this share of the window is the accent colour: the accent of
    /// the 60-30-10 rule that colour guides (Material's colour roles among
    /// them) give for a balanced screen.
    pub const ACCENT_MAX_SHARE: f32 = 0.10;
    /// At most this many highlighted (primary) buttons on one surface: one
    /// call to action, as Material 3 keeps its filled, high-emphasis button
    /// for the one most important action and Apple's guidelines give a
    /// window one default button.
    pub const PRIMARY_BUTTONS_MAX: usize = 1;
}

/// How keyboard focus moves (`focus-order`, `focus-arrow-keys`).
pub mod keyboard {
    /// Two Tab stops are on one row when they overlap vertically by at
    /// least this share of the shorter one: a label and a field beside it
    /// with their baselines a little apart are one row.
    pub const ROW_OVERLAP: f32 = 0.5;
    /// At most this many Tab stops inside one list, grid, tab bar or radio
    /// group. The WAI-ARIA Authoring Practices make a composite widget one
    /// Tab stop with the arrows inside it; Qt Quick Controls give each
    /// radio button and tab button a stop of its own, so a short group is
    /// everywhere and quick to Tab past. Past three, the arrows save the
    /// user keys.
    pub const COMPOSITE_TAB_STOPS_MAX: usize = 3;
}
