//! `content-repeated-text`: two texts side by side in one parent say the
//! same words, so the second tells a user nothing (an error that repeats
//! its own title).
//!
//! Only texts the application declares, of two words or more: the labels
//! a style draws for its controls, and a single word repeated in a table
//! of numbers or tags, are left out.

use std::collections::HashMap;

use ljos_engine::ObjectId;

use super::controls::{shown_texts, word_list};
use crate::cx::{Cx, place_of};

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let mut first: HashMap<(ObjectId, String), ObjectId> = HashMap::new();
    for (item, text) in shown_texts(cx) {
        if word_list(&text).len() < 2
            || place_of(e, item).is_none_or(|p| ljos_controls::is_style_url(&p))
        {
            continue;
        }
        let Some(parent) = e.visual_parent(item) else {
            continue;
        };
        let key = (parent, text.trim().to_lowercase());
        match first.get(&key) {
            None => {
                first.insert(key, item);
            }
            Some(_) => {
                cx.report(
                    "content-repeated-text",
                    Some(item),
                    format!(
                        "the text {:?} says what the text before it says; say the next thing \
                         instead (the cause, what to do), or leave it out",
                        text.trim()
                    ),
                );
            }
        }
    }
}
