//! `color-semantic-hue`: what says danger is drawn green, or what says
//! success red.
//!
//! The meaning is read from words: a button or a label that says Delete,
//! Remove or Discard, Error or Failed, and one that says Saved, Done or
//! Success; or an item whose `state` is `error` or `critical`. Its colour
//! is the one most of its coloured pixels have, drawn as the scene shows
//! them, so a red text on a grey button and a red button with white text
//! are both red. Only red and green are held against each other, the two
//! whose meanings every guide agrees on (ISO 3864-1, and the platforms'
//! destructive and success colours); a warning in some other hue is a
//! choice of the design, and so is anything grey. A hint: the words are
//! not always the meaning.

use ljos_controls::AbstractButton;
use ljos_design::color::{Oklch, Srgb};
use ljos_design::limits::audit::VISIBLE_CHANGE_MIN_PX;
use ljos_design::limits::hue::{GREEN, MIN_CHROMA, RED};
use ljos_engine::ObjectId;
use ljos_quick::Text;

use crate::cx::{Cx, ancestors, is_a, scene_rect, shown_items, words};
use crate::pixels::{Painter, backdrop};
use crate::surfaces::dominant;

#[derive(Clone, Copy, PartialEq)]
enum Meaning {
    Danger,
    Success,
}

const DANGER: &[&str] = &[
    "delete", "remove", "discard", "erase", "destroy", "error", "failed", "failure", "danger",
    "critical",
];
const SUCCESS: &[&str] = &[
    "saved",
    "done",
    "success",
    "successful",
    "succeeded",
    "completed",
];

fn meaning_of(words: &str) -> Option<Meaning> {
    let words: Vec<String> = words
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    // A sentence is more than a label; its words are not its meaning.
    if words.is_empty() || words.len() > 3 {
        return None;
    }
    if words.iter().any(|w| DANGER.contains(&w.as_str())) {
        Some(Meaning::Danger)
    } else if words.iter().any(|w| SUCCESS.contains(&w.as_str())) {
        Some(Meaning::Success)
    } else {
        None
    }
}

fn read(cx: &Cx, item: ObjectId, name: &str) -> String {
    let e = cx.engine;
    e.untrack(|| e.read_by_name(item, name))
        .to_js_string()
        .as_str()
        .to_string()
}

fn in_band(h: f32, (from, to): (f32, f32)) -> bool {
    if from <= to {
        (from..=to).contains(&h)
    } else {
        h >= from || h <= to
    }
}

/// What the item means, read from its words or its state.
fn meaning(cx: &Cx, item: ObjectId) -> Option<Meaning> {
    let e = cx.engine;
    let state = read(cx, item, "state");
    if matches!(state.as_str(), "error" | "critical") {
        return Some(Meaning::Danger);
    }
    if is_a::<AbstractButton>(e, item) {
        return meaning_of(&read(cx, item, "text"));
    }
    // A button's label speaks for the button.
    if is_a::<Text>(e, item) && !ancestors(e, item).any(|a| is_a::<AbstractButton>(e, a)) {
        return meaning_of(&read(cx, item, "text"));
    }
    None
}

pub(crate) fn check(cx: &mut Cx) {
    let scene = cx.scene;
    let Some(snapshot) = scene.snapshot() else {
        return;
    };
    let e = cx.engine;
    let mut painter = Painter::new(backdrop(e, cx.options));
    let mut found = Vec::new();
    for item in shown_items(cx) {
        let Some(meant) = meaning(cx, item) else {
            continue;
        };
        let Some(region) = scene_rect(e, item) else {
            continue;
        };
        let Some(picture) = painter.draw(snapshot, region, |_| true) else {
            continue;
        };
        let coloured: Vec<Srgb> = picture
            .pixels()
            .filter(|c| Oklch::from_srgb(*c).c >= MIN_CHROMA)
            .collect();
        if coloured.len() < VISIBLE_CHANGE_MIN_PX {
            continue;
        }
        let Some((colour, _)) = dominant(coloured.into_iter()) else {
            continue;
        };
        let h = Oklch::from_srgb(colour).h;
        let wrong = match meant {
            Meaning::Danger => in_band(h, GREEN),
            Meaning::Success => in_band(h, RED),
        };
        if wrong {
            found.push((item, meant, colour, h));
        }
    }
    for (item, meant, colour, h) in found {
        let (is, reads) = match meant {
            Meaning::Danger => ("green", "danger or an error, which reads as red"),
            Meaning::Success => ("red", "success, which reads as green"),
        };
        let message = format!(
            "{} is drawn in {is} ({}, hue {h:.0}°), and says {reads}: the colour tells the \
             opposite of the words",
            words(cx, item),
            colour.to_hex(),
        );
        let f = cx.report("color-semantic-hue", Some(item), message);
        f.measured = Some(format!("hue {h:.0}°"));
    }
}
