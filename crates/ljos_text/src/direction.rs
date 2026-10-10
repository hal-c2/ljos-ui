//! Which way a text reads, without shaping it.

/// True when the first character with a direction of its own is a
/// right-to-left one: the rule the Unicode bidi algorithm uses for the
/// direction of a paragraph, and the rule Qt uses to align a `Text` whose
/// alignment was never set.
///
/// Digits, spaces and punctuation have no direction of their own, so
/// `"123 שלום"` reads right to left and `"123"` does not.
///
/// A layout says the same in [`crate::TextLayout::rtl`]; this is for callers
/// that need the answer before, or without, a layout.
pub fn reads_right_to_left(text: &str) -> bool {
    for c in text.chars() {
        match c {
            // The marks exist to give a direction where no letter does.
            '\u{200E}' => return false,
            '\u{200F}' | '\u{061C}' => return true,
            _ => {}
        }
        if !c.is_alphabetic() {
            continue;
        }
        return in_right_to_left_block(c);
    }
    false
}

/// The blocks Unicode sets aside for right-to-left scripts. Unassigned code
/// points in them are right-to-left by default too, so the ranges are whole
/// blocks rather than lists of letters.
fn in_right_to_left_block(c: char) -> bool {
    matches!(
        u32::from(c),
        // Hebrew, Arabic, Syriac, Thaana, NKo, Samaritan, Mandaic and the
        // Arabic extensions.
        0x0590..=0x08FF
        // Hebrew and Arabic presentation forms.
        | 0xFB1D..=0xFDFF
        | 0xFE70..=0xFEFE
        // Historic right-to-left scripts, then Adlam, Mende Kikakui and the
        // Arabic mathematical letters.
        | 0x1_0800..=0x1_0FFF
        | 0x1_E800..=0x1_EFFF
    )
}

#[cfg(test)]
mod tests {
    use super::reads_right_to_left as rtl;

    #[test]
    fn the_first_letter_decides() {
        assert!(!rtl("Hello"));
        assert!(rtl("שלום"));
        assert!(rtl("مرحبا"));
        assert!(rtl("שלום world"));
        assert!(!rtl("hello עולם"));
    }

    #[test]
    fn digits_and_punctuation_do_not_decide() {
        assert!(!rtl(""));
        assert!(!rtl("123 (4)"));
        assert!(rtl("123 שלום"));
        assert!(!rtl("… abc"));
        // Arabic-Indic digits are numbers, not letters.
        assert!(!rtl("\u{0661}\u{0662} abc"));
    }

    #[test]
    fn a_direction_mark_decides() {
        assert!(rtl("\u{200F}abc"));
        assert!(!rtl("\u{200E}שלום"));
    }
}
