//! What Android's on-screen keyboard is told about an editor: the bits of
//! `EditorInfo` (`inputType`, the action, `imeOptions`), from an
//! [`EditorKind`]. Plain numbers, so that it is tested off Android too.
//!
//! The values are Android's public constants (`android.text.InputType`,
//! `android.view.inputmethod.EditorInfo`).

use crate::{EditorKind, TextKind};

const CLASS_TEXT: u32 = 0x1;
const CLASS_NUMBER: u32 = 0x2;
const CLASS_PHONE: u32 = 0x3;
const TEXT_VARIATION_URI: u32 = 0x10;
const TEXT_VARIATION_EMAIL_ADDRESS: u32 = 0x20;
const TEXT_VARIATION_PASSWORD: u32 = 0x80;
const TEXT_FLAG_CAP_SENTENCES: u32 = 0x4000;
const TEXT_FLAG_AUTO_CORRECT: u32 = 0x8000;
const TEXT_FLAG_MULTI_LINE: u32 = 0x2_0000;
const TEXT_FLAG_NO_SUGGESTIONS: u32 = 0x8_0000;
const NUMBER_FLAG_SIGNED: u32 = 0x1000;
const NUMBER_FLAG_DECIMAL: u32 = 0x2000;
const NUMBER_VARIATION_PASSWORD: u32 = 0x10;

const ACTION_UNSPECIFIED: i32 = 0;
const ACTION_DONE: i32 = 6;

const FLAG_NO_PERSONALIZED_LEARNING: u32 = 0x100_0000;
const FLAG_NO_FULLSCREEN: u32 = 0x200_0000;
const FLAG_NO_EXTRACT_UI: u32 = 0x1000_0000;
const FLAG_NO_ENTER_ACTION: u32 = 0x4000_0000;

/// `EditorInfo`'s `inputType`, action and `imeOptions` for `editor`.
pub(crate) fn editor_info(editor: &EditorKind) -> (u32, i32, u32) {
    let input_type = match editor.kind {
        TextKind::Digits if editor.hidden => CLASS_NUMBER | NUMBER_VARIATION_PASSWORD,
        TextKind::Digits => CLASS_NUMBER,
        TextKind::Number => CLASS_NUMBER | NUMBER_FLAG_SIGNED | NUMBER_FLAG_DECIMAL,
        TextKind::Phone => CLASS_PHONE,
        kind => {
            let variation = match kind {
                _ if editor.hidden => TEXT_VARIATION_PASSWORD,
                TextKind::Email => TEXT_VARIATION_EMAIL_ADDRESS,
                TextKind::Url => TEXT_VARIATION_URI,
                _ => 0,
            };
            let mut flags = 0;
            if editor.multi_line {
                flags |= TEXT_FLAG_MULTI_LINE;
            }
            // A password, an address or a link is neither a sentence nor
            // a word to correct.
            let prose = !editor.hidden && variation == 0;
            if prose && !editor.no_capitals {
                flags |= TEXT_FLAG_CAP_SENTENCES;
            }
            if prose && !editor.no_suggestions {
                flags |= TEXT_FLAG_AUTO_CORRECT;
            } else if editor.no_suggestions {
                flags |= TEXT_FLAG_NO_SUGGESTIONS;
            }
            CLASS_TEXT | variation | flags
        }
    };
    // The application draws its own editor: a keyboard that took the
    // whole screen in landscape would hide it behind one of its own.
    let mut options = FLAG_NO_FULLSCREEN | FLAG_NO_EXTRACT_UI;
    if editor.hidden || editor.private {
        options |= FLAG_NO_PERSONALIZED_LEARNING;
    }
    let action = if editor.multi_line {
        // Enter types a new line.
        options |= FLAG_NO_ENTER_ACTION;
        ACTION_UNSPECIFIED
    } else {
        ACTION_DONE
    };
    (input_type, action, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_is_capitalised_and_corrected_and_done_with() {
        let (t, a, o) = editor_info(&EditorKind::default());
        assert_eq!(t, CLASS_TEXT | TEXT_FLAG_CAP_SENTENCES | TEXT_FLAG_AUTO_CORRECT);
        assert_eq!(a, ACTION_DONE);
        assert_eq!(o, FLAG_NO_FULLSCREEN | FLAG_NO_EXTRACT_UI);
    }

    #[test]
    fn a_password_is_neither_learnt_nor_suggested() {
        let (t, _, o) = editor_info(&EditorKind {
            hidden: true,
            ..EditorKind::default()
        });
        assert_eq!(t, CLASS_TEXT | TEXT_VARIATION_PASSWORD);
        assert_ne!(o & FLAG_NO_PERSONALIZED_LEARNING, 0);
        let (t, ..) = editor_info(&EditorKind {
            kind: TextKind::Digits,
            hidden: true,
            ..EditorKind::default()
        });
        assert_eq!(t, CLASS_NUMBER | NUMBER_VARIATION_PASSWORD);
    }

    #[test]
    fn many_lines_keep_enter_for_new_lines() {
        let (t, a, o) = editor_info(&EditorKind {
            multi_line: true,
            no_suggestions: true,
            ..EditorKind::default()
        });
        assert_eq!(
            t,
            CLASS_TEXT | TEXT_FLAG_MULTI_LINE | TEXT_FLAG_CAP_SENTENCES | TEXT_FLAG_NO_SUGGESTIONS
        );
        assert_eq!(a, ACTION_UNSPECIFIED);
        assert_ne!(o & FLAG_NO_ENTER_ACTION, 0);
    }

    #[test]
    fn kinds_pick_their_keyboards() {
        let of = |kind| editor_info(&EditorKind { kind, ..EditorKind::default() }).0;
        assert_eq!(of(TextKind::Phone), CLASS_PHONE);
        assert_eq!(of(TextKind::Number), CLASS_NUMBER | NUMBER_FLAG_SIGNED | NUMBER_FLAG_DECIMAL);
        assert_eq!(of(TextKind::Email), CLASS_TEXT | TEXT_VARIATION_EMAIL_ADDRESS);
        assert_eq!(of(TextKind::Url), CLASS_TEXT | TEXT_VARIATION_URI);
    }
}
