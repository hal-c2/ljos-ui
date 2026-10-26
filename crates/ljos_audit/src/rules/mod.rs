//! The checks, one file each, and the one table that says which rules
//! the audit implements and in which order they run.

mod color_text_contrast;
mod duplicates;
mod hidden;
mod images;
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
];

/// The codes the audit implements, each once, in the order they run.
pub fn implemented() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for code in CHECKS.iter().flat_map(|c| c.codes.iter().copied()) {
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
