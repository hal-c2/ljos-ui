//! The checks, one file each, and the one table that says which rules
//! the audit implements and in which order they run.

mod color_text_contrast;
mod duplicates;
mod hidden;
mod images;
mod layout_content_overflow;
mod layout_fill_crushed;
mod layout_occluded;
mod layout_offscreen;
mod layout_popup_fit;
mod layout_under_minimum;
mod names;
mod sets;
mod tab;
mod target_size;
mod text_clipped;
mod text_elided_hidden;
// Colour, focus and motion, the second batch.
mod color_accent_weight;
mod color_nontext_contrast;
mod color_semantic_hue;
mod color_state_only;
mod color_surface_merge;
mod color_translucent_surface;
mod focus_arrow_keys;
mod focus_escape_dismiss;
mod focus_indicator_missing;
mod focus_order;
mod focus_popup_stranded;
mod focus_shortcut_under_modal;
mod motion_dropped_frames;
mod motion_duration;
mod motion_ease_in_enter;
mod motion_frequent;
mod motion_reduced_ignored;
mod motion_scale_from_zero;
mod motion_stagger;
mod motion_theme_switch;

use crate::acting::Acting;
use crate::cx::Cx;

/// A check: the rules it reports and what runs it.
pub(crate) struct Check {
    pub codes: &'static [&'static str],
    pub run: fn(&mut Cx),
}

/// Every check, in the order a report lists them. The layout comes before
/// the colours: text that is not on the screen has no contrast to speak of.
pub(crate) const CHECKS: &[Check] = &[
    Check {
        codes: &["a11y-unnamed-interactive"],
        run: names::check,
    },
    Check {
        codes: &["a11y-unnamed-image"],
        run: images::check,
    },
    Check {
        codes: &["a11y-duplicate-name"],
        run: duplicates::check,
    },
    Check {
        codes: &["a11y-target-size"],
        run: target_size::check,
    },
    Check {
        codes: &["a11y-hidden-exposed"],
        run: hidden::check,
    },
    // The Tab walk also finds a stop that says nothing of itself.
    Check {
        codes: &[
            "a11y-tab-trap",
            "a11y-tab-unreachable",
            "a11y-unnamed-interactive",
        ],
        run: tab::check,
    },
    Check {
        codes: &["layout-offscreen"],
        run: layout_offscreen::check,
    },
    Check {
        codes: &["layout-occluded"],
        run: layout_occluded::check,
    },
    Check {
        codes: &["layout-under-minimum"],
        run: layout_under_minimum::check,
    },
    Check {
        codes: &["layout-fill-crushed"],
        run: layout_fill_crushed::check,
    },
    Check {
        codes: &["layout-content-overflow"],
        run: layout_content_overflow::check,
    },
    Check {
        codes: &["layout-popup-fit"],
        run: layout_popup_fit::check,
    },
    Check {
        codes: &["text-clipped"],
        run: text_clipped::check,
    },
    Check {
        codes: &["text-elided-hidden"],
        run: text_elided_hidden::check,
    },
    Check {
        codes: &["color-text-contrast"],
        run: color_text_contrast::check,
    },
    Check {
        codes: &["a11y-set-size"],
        run: sets::check,
    },
    // Names, layout and content, read off the live tree (batch 2b).
    Check {
        codes: &["a11y-unnamed-icon-button", "a11y-icon-button-no-tooltip"],
        run: icon_buttons::check,
    },
    Check {
        codes: &["a11y-name-mismatch"],
        run: name_mismatch::check,
    },
    Check {
        codes: &["a11y-nested-interactive"],
        run: nested_interactive::check,
    },
    Check {
        codes: &["a11y-ignored-focusable"],
        run: ignored_focusable::check,
    },
    Check {
        codes: &["a11y-unnamed-dialog"],
        run: unnamed_dialog::check,
    },
    Check {
        codes: &["a11y-ambiguous-name"],
        run: ambiguous_name::check,
    },
    Check {
        codes: &["a11y-current-unmarked"],
        run: current_unmarked::check,
    },
    Check {
        codes: &["a11y-live-status"],
        run: live_status::check,
    },
    Check {
        codes: &["layout-overlap"],
        run: overlap::check,
    },
    Check {
        codes: &["layout-misaligned"],
        run: misaligned::check,
    },
    Check {
        codes: &["layout-stacked-scrims"],
        run: stacked_scrims::check,
    },
    Check {
        codes: &["layout-window-controls"],
        run: window_controls::check,
    },
    Check {
        codes: &["layout-wrap-separator"],
        run: wrap_separator::check,
    },
    Check {
        codes: &["layout-menu-ungrouped"],
        run: menu_ungrouped::check,
    },
    Check {
        codes: &["layout-reflow"],
        run: reflow::check,
    },
    Check {
        codes: &["layout-resize-handle"],
        run: resize_handle::check,
    },
    Check {
        codes: &["layout-shift"],
        run: layout_shift::check,
    },
    Check {
        codes: &["content-raw-identifier"],
        run: identifiers::check,
    },
    Check {
        codes: &["text-raw-markup"],
        run: raw_markup::check,
    },
    Check {
        codes: &["content-empty-state"],
        run: empty_state::check,
    },
    Check {
        codes: &["content-repeated-text"],
        run: repeated_text::check,
    },
    Check {
        codes: &["content-identical-rows"],
        run: identical_rows::check,
    },
    Check {
        codes: &["content-inconsistent-words"],
        run: inconsistent_words::check,
    },
    Check {
        codes: &["content-disabled-no-reason"],
        run: disabled_no_reason::check,
    },
    Check {
        codes: &["text-too-small"],
        run: text_too_small::check,
    },
    Check {
        codes: &["text-line-length"],
        run: line_length::check,
    },
    // Colour, focus and motion, the second batch.
    Check {
        codes: &["color-nontext-contrast"],
        run: color_nontext_contrast::check,
    },
    Check {
        codes: &["color-semantic-hue"],
        run: color_semantic_hue::check,
    },
    Check {
        codes: &["color-accent-weight"],
        run: color_accent_weight::check,
    },
    Check {
        codes: &["focus-order"],
        run: focus_order::check,
    },
    Check {
        codes: &["focus-popup-stranded"],
        run: focus_popup_stranded::check,
    },
    Check {
        codes: &["focus-shortcut-under-modal"],
        run: focus_shortcut_under_modal::check,
    },
    Check {
        codes: &["focus-arrow-keys"],
        run: focus_arrow_keys::check,
    },
    Check {
        codes: &["motion-stagger"],
        run: motion_stagger::check,
    },
];

/// The checks that act on the scene to see what it does (focus an item,
/// flip a state, run an animation), after all of [`CHECKS`], in the order
/// a report lists them. Each puts back what it changed.
pub(crate) const ACTS: &[Acting] = &[
    Acting {
        codes: &["color-state-only"],
        run: color_state_only::check,
    },
    Acting {
        codes: &["color-translucent-surface"],
        run: color_translucent_surface::check,
    },
    Acting {
        codes: &["color-surface-merge"],
        run: color_surface_merge::check,
    },
    Acting {
        codes: &["focus-indicator-missing"],
        run: focus_indicator_missing::check,
    },
    Acting {
        codes: &["focus-escape-dismiss"],
        run: focus_escape_dismiss::check,
    },
    Acting {
        codes: &["motion-duration"],
        run: motion_duration::check,
    },
    Acting {
        codes: &["motion-ease-in-enter"],
        run: motion_ease_in_enter::check,
    },
    Acting {
        codes: &["motion-scale-from-zero"],
        run: motion_scale_from_zero::check,
    },
    Acting {
        codes: &["motion-frequent"],
        run: motion_frequent::check,
    },
    Acting {
        codes: &["motion-theme-switch"],
        run: motion_theme_switch::check,
    },
    Acting {
        codes: &["motion-reduced-ignored"],
        run: motion_reduced_ignored::check,
    },
    Acting {
        codes: &["motion-dropped-frames"],
        run: motion_dropped_frames::check,
    },
];

/// The codes the audit implements, each once, in the order they run.
pub fn implemented() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    let acts = ACTS.iter().flat_map(|c| c.codes.iter().copied());
    for code in CHECKS
        .iter()
        .flat_map(|c| c.codes.iter().copied())
        .chain(acts)
    {
        if !out.contains(&code) {
            out.push(code);
        }
    }
    out
}

// Names, layout and content (batch 2b).
mod ambiguous_name;
mod controls;
mod current_unmarked;
mod icon_buttons;
mod ignored_focusable;
mod live_status;
mod name_mismatch;
mod nested_interactive;
mod unnamed_dialog;

mod layout_shift;
mod menu_ungrouped;
mod misaligned;
mod overlap;
mod reflow;
mod resize_handle;
mod stacked_scrims;
mod window_controls;
mod wrap_separator;

mod disabled_no_reason;
mod empty_state;
mod identical_rows;
mod identifiers;
mod inconsistent_words;
mod line_length;
mod raw_markup;
mod repeated_text;
mod text_too_small;
