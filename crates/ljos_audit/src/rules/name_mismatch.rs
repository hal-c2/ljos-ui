//! `a11y-name-mismatch` (WCAG 2.5.3, label in name): someone who sees
//! "Save" says "Save" to speech control, and a button whose name is
//! "Store" does not answer.
//!
//! Only a button's own text is its visible label here: a label beside a
//! field is a guess, and the rule is a warning.

use super::controls::{button_look, word_list};
use crate::cx::{Cx, acted_on, node_words};

/// Whether the words of `label` come in `name` in order, others between
/// them allowed: "More options" is in "More file options".
fn contains_in_order(name: &[String], label: &[String]) -> bool {
    let mut rest = name.iter();
    label.iter().all(|w| rest.any(|n| n == w))
}

pub(crate) fn check(cx: &mut Cx) {
    for (key, _, data) in cx.nodes().iter() {
        if !acted_on(data) || data.name.trim().is_empty() {
            continue;
        }
        let Some(item) = cx.item(*key) else { continue };
        let Some(look) = button_look(cx.engine, item).filter(|l| l.text_shown) else {
            continue;
        };
        let label = word_list(&look.text);
        let name = word_list(&data.name);
        if label.is_empty() || contains_in_order(&name, &label) {
            continue;
        }
        let shown = look.text.trim().to_string();
        cx.report(
            "a11y-name-mismatch",
            Some(item),
            format!(
                "{} shows {shown:?}, which its name does not hold, so someone who says what \
                 they see to speech control does not reach it; begin `Accessible.name` with \
                 the text it shows",
                node_words(data)
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<String> {
        word_list(s)
    }

    #[test]
    fn the_label_may_have_words_between() {
        assert!(contains_in_order(
            &w("More file options"),
            &w("More options")
        ));
        assert!(contains_in_order(&w("Save draft"), &w("save")));
        assert!(!contains_in_order(&w("Store"), &w("Save")));
        assert!(!contains_in_order(&w("options More"), &w("More options")));
    }
}
