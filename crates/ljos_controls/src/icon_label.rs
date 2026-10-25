//! `IconLabel` and `IconImage` (QtQuick.Controls.impl): the picture and
//! the text of a button, laid out as its `display` says.
//!
//! Qt names both types in its type table and documents neither; what
//! they do here is what a button with an icon shows on `qml6` (Qt
//! 6.11.2), pinned by `features/controls/button-icon.feature` and
//! `button-icon-svg.feature`:
//!
//! - the picture is made at `icon.width` by `icon.height` (its
//!   `sourceSize`): a picture is as large as its file and no larger,
//!   keeping its shape, and an SVG drawing is stretched to exactly that;
//! - a picture with less room in the label than that is drawn smaller to
//!   fit, keeping its shape, though the label still asks for its size;
//! - it is drawn in one colour: `icon.color` when the button was given
//!   one (`"transparent"` shows the picture as it is), the colour of the
//!   text otherwise;
//! - picture and text sit side by side or one over the other with
//!   `spacing` between them, and together they are placed by `alignment`;
//! - `icon.name` asks the icon theme, and there is none here: a name
//!   shows nothing, and `icon.source` is shown instead when given.
//!
//! A label shows the icon of the button it belongs to in whatever its own
//! `icon` was not told. Qt's files write `icon: control.icon`; the engine
//! copies the members the source set, so the label sees them either way.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{
    Atom, Color, Cx, Engine, EngineError, Font, MetaRef, ObjectId, PropIndex, Provenance,
    QmlObject, QmlType, Value, qml_impl,
};
use ljos_quick::enums::{align, elide};
use ljos_quick::text_input::text_format;
use ljos_quick::{Image, ImageBase, Item, Text, register_image_tint, set_parent_item};

use crate::button::{AbstractButton, Icon, display, icon_giver};
use crate::util::{check, flag, is_a, mnemonic_underlined, num, set_computed, without_mnemonic};

/// `IconImage`: an `Image` drawn in one colour.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Controls.impl", name = "IconImage", extends = Image)]
pub struct IconImage {
    #[qml(base)]
    pub image: Image,
    /// The name of the icon in the theme. Stored: there is no theme.
    #[qml(notify)]
    pub name: Atom,
    /// Transparent shows the picture as it is.
    #[qml(notify)]
    pub color: Color,
}

impl Default for IconImage {
    fn default() -> Self {
        Self {
            image: Image::default(),
            name: Atom::EMPTY,
            color: Color::TRANSPARENT,
        }
    }
}

fn label_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Display",
        vec![
            ("IconOnly", i64::from(display::ICON_ONLY)),
            ("TextOnly", i64::from(display::TEXT_ONLY)),
            ("TextBesideIcon", i64::from(display::TEXT_BESIDE_ICON)),
            ("TextUnderIcon", i64::from(display::TEXT_UNDER_ICON)),
        ],
    )]
}

/// `IconLabel`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Controls.impl",
    name = "IconLabel",
    extends = Item,
    enums = label_enums
)]
pub struct IconLabel {
    #[qml(base)]
    pub item: Item,
    #[qml(group)]
    pub icon: Icon,
    #[qml(notify)]
    pub text: Atom,
    #[qml(notify)]
    pub font: Font,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify, enumeration = "Display")]
    pub display: i32,
    #[qml(notify)]
    pub spacing: f64,
    #[qml(notify)]
    pub mirrored: bool,
    /// Where the picture and the text are, together, in a label that is
    /// larger than they need.
    #[qml(notify, flags = "Qt.Alignment")]
    pub alignment: i32,
    #[qml(notify, reset)]
    pub top_padding: f64,
    #[qml(notify, reset)]
    pub left_padding: f64,
    #[qml(notify, reset)]
    pub right_padding: f64,
    #[qml(notify, reset)]
    pub bottom_padding: f64,
    /// The colour of an icon that was given none. Until it is set, that
    /// is the colour of the text.
    #[qml(notify)]
    pub default_icon_color: Color,
    #[qml(skip)]
    made: Rc<Made>,
}

impl Default for IconLabel {
    fn default() -> Self {
        Self {
            item: Item::default(),
            icon: Icon::default(),
            text: Atom::EMPTY,
            font: Font::default(),
            color: Color::BLACK,
            display: display::TEXT_BESIDE_ICON,
            spacing: 0.0,
            mirrored: false,
            alignment: align::H_CENTER | align::V_CENTER,
            top_padding: 0.0,
            left_padding: 0.0,
            right_padding: 0.0,
            bottom_padding: 0.0,
            default_icon_color: Color::TRANSPARENT,
            made: Rc::default(),
        }
    }
}

/// The items a label made for what it shows.
#[derive(Debug, Default)]
struct Made {
    image: Cell<Option<ObjectId>>,
    text: Cell<Option<ObjectId>>,
    /// The `sourceSize` last given to `image`, so that it is given only
    /// when it changes: giving it loads the picture again.
    asked: Cell<Option<(ObjectId, (f64, f64))>>,
}

#[qml_impl]
impl IconLabel {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        let made = e.with::<IconLabel, _>(obj, |l| l.made.clone())?;
        e.effect_in(obj, move |e| {
            if e.is_alive(obj) {
                check(e, "IconLabel", show(e, obj, &made));
            }
        })?;
        Ok(())
    }
}

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick.Controls.impl" 2.15 => [
        IconImage, IconLabel,
    ]));
    register_image_tint(
        engine,
        MetaRef::Static(IconImage::static_meta()),
        |e, o| match e.read(o, IconImage::COLOR.index) {
            Value::Color(c) if c.a > 0 => Some(c),
            _ => None,
        },
    );
}

/// The leaves of an `icon` group, in the order of [`Icon`]'s fields that
/// matter here.
const LEAVES: [&str; 5] = [
    "icon.name",
    "icon.source",
    "icon.width",
    "icon.height",
    "icon.color",
];

/// The button a label belongs to: the nearest object above it that is
/// one.
fn button_of(engine: &Engine, label: ObjectId) -> Option<ObjectId> {
    let mut cur = engine.owner(label);
    // An ownership chain deeper than this is a cycle someone built.
    for _ in 0..4096 {
        let o = cur.filter(|o| engine.is_alive(*o))?;
        if is_a::<AbstractButton>(engine, o) {
            return Some(o);
        }
        cur = engine.owner(o);
    }
    None
}

/// The icon a label shows.
#[derive(Debug)]
struct Shown {
    name: Atom,
    /// As the file that named it means it; empty for none.
    source: String,
    /// The most the picture may take; a side that is not positive is not
    /// limited.
    limit: (f64, f64),
    /// The colour the picture is drawn in; none shows it as it is.
    colour: Option<Color>,
}

/// What each leaf of the label's icon is and who gave it: the label, the
/// button it belongs to or the action that button follows (tracked).
fn shown(engine: &Engine, label: ObjectId) -> Shown {
    let button = engine.untrack(|| button_of(engine, label));
    let giver = |leaf: &str| -> Option<(ObjectId, PropIndex, bool)> {
        let own = engine.property_index(label, leaf).ok()?;
        if engine
            .provenance_tracked(label, own)
            .is_some_and(|p| !unset(p))
        {
            return Some((label, own, true));
        }
        button
            .and_then(|b| icon_giver(engine, b, leaf))
            .or(Some((label, own, false)))
    };
    let [name, source, width, height, colour] = LEAVES.map(giver);
    let read = |g: Option<(ObjectId, PropIndex, bool)>| match g {
        Some((o, p, _)) => engine.read(o, p),
        None => Value::Undefined,
    };
    let text_colour = || match engine.read(label, IconLabel::COLOR.index) {
        Value::Color(c) => c,
        _ => Color::BLACK,
    };
    let colour = match (colour, read(colour)) {
        (Some((_, _, true)), Value::Color(c)) => (c.a > 0).then_some(c),
        (_, Value::Color(c)) if c.a > 0 => Some(c),
        _ => Some(
            match engine.read(label, IconLabel::DEFAULT_ICON_COLOR.index) {
                Value::Color(c) if c.a > 0 => c,
                _ => text_colour(),
            },
        ),
    };
    let relative = read(source).to_js_string();
    Shown {
        name: Atom::from(read(name).to_js_string().as_str()),
        source: match source {
            Some((from, ..)) if !relative.is_empty() => engine.resolve_url(from, &relative),
            _ => String::new(),
        },
        limit: (read(width).to_number(), read(height).to_number()),
        colour,
    }
}

fn unset(p: Provenance) -> bool {
    matches!(
        p,
        Provenance::Default | Provenance::Animated | Provenance::Host
    )
}

/// Makes or drops the item in `slot` so that there is one exactly when
/// `wanted`.
fn keep<T: QmlType + Default>(
    engine: &Engine,
    label: ObjectId,
    slot: &Cell<Option<ObjectId>>,
    wanted: bool,
) -> Result<Option<ObjectId>, EngineError> {
    let have = slot.get().filter(|o| engine.is_alive(*o));
    match (have, wanted) {
        (Some(o), true) => Ok(Some(o)),
        (None, false) => Ok(None),
        (Some(o), false) => {
            slot.set(None);
            // Out of the label now; the object goes at the next pass.
            set_parent_item(engine, o, None)?;
            engine.destroy(o);
            Ok(None)
        }
        (None, true) => {
            let o = engine.create_native_incomplete(T::default(), Some(label))?;
            set_parent_item(engine, o, Some(label))?;
            engine.complete(o)?;
            slot.set(Some(o));
            Ok(Some(o))
        }
    }
}

/// Brings the label's picture and text in step with its properties, and
/// places them (the body of the label's effect: what it reads is
/// followed).
fn show(engine: &Engine, label: ObjectId, made: &Made) -> Result<(), EngineError> {
    let icon = shown(engine, label);
    let how = num(engine, label, IconLabel::DISPLAY.index) as i32;
    // What is drawn has no mnemonic markers; `text` keeps them.
    let text = Value::String(Atom::from(
        without_mnemonic(&engine.read(label, IconLabel::TEXT.index).to_js_string()).as_str(),
    ));
    let has_text = !text.to_js_string().is_empty();
    let has_icon = !icon.name.is_empty() || !icon.source.is_empty();
    // A label with only one of the two shows it whatever the display.
    let wants_icon = has_icon && (how != display::TEXT_ONLY || !has_text);
    let wants_text = has_text && (how != display::ICON_ONLY || !has_icon);

    let image = engine.untrack(|| keep::<IconImage>(engine, label, &made.image, wants_icon))?;
    let words = engine.untrack(|| keep::<Text>(engine, label, &made.text, wants_text))?;

    let mut icon_size = (0.0, 0.0);
    if let Some(image) = image {
        let tint = icon.colour.unwrap_or(Color::TRANSPARENT);
        set_computed(engine, image, IconImage::COLOR.index, Value::Color(tint))?;
        set_computed(
            engine,
            image,
            IconImage::NAME.index,
            Value::String(icon.name.clone()),
        )?;
        // The picture is made at the icon's size, before it is loaded so
        // that it loads once: a drawing exactly at it, a picture no larger.
        let ask = (icon.limit.0.max(0.0), icon.limit.1.max(0.0));
        if made.asked.get() != Some((image, ask)) {
            made.asked.set(Some((image, ask)));
            set_computed(
                engine,
                image,
                ImageBase::SOURCE_SIZE.index,
                Value::size(ask.0, ask.1),
            )?;
        }
        let source = Value::Url(Atom::from(icon.source.as_str()));
        set_computed(engine, image, ImageBase::SOURCE.index, source)?;
        let natural = (
            num(engine, image, Item::IMPLICIT_WIDTH.index),
            num(engine, image, Item::IMPLICIT_HEIGHT.index),
        );
        icon_size = fitted(natural, icon.limit);
    }
    let mut text_size = (0.0, 0.0);
    if let Some(words) = words {
        let written = engine.read(label, IconLabel::TEXT.index).to_js_string();
        let (text, format) = match mnemonic_underlined(&written) {
            Some(styled) => (
                Value::String(Atom::from(styled.as_str())),
                text_format::STYLED_TEXT,
            ),
            None => (text, text_format::AUTO_TEXT),
        };
        set_computed(engine, words, Text::TEXT_FORMAT.index, Value::Int(format))?;
        set_computed(engine, words, Text::TEXT.index, text)?;
        for (to, from) in [
            (Text::FONT.index, IconLabel::FONT.index),
            (Text::COLOR.index, IconLabel::COLOR.index),
        ] {
            set_computed(engine, words, to, engine.read(label, from))?;
        }
        // A text with less room than it needs ends in an ellipsis.
        set_computed(engine, words, Text::ELIDE.index, Value::Int(elide::RIGHT))?;
        text_size = (
            num(engine, words, Item::IMPLICIT_WIDTH.index),
            num(engine, words, Item::IMPLICIT_HEIGHT.index),
        );
    }

    let layout = Layout {
        icon: image.map(|_| icon_size),
        text: words.map(|_| text_size),
        under: how == display::TEXT_UNDER_ICON,
        spacing: num(engine, label, IconLabel::SPACING.index),
        mirrored: flag(engine, label, IconLabel::MIRRORED.index),
        alignment: num(engine, label, IconLabel::ALIGNMENT.index) as i32,
        padding: [
            IconLabel::LEFT_PADDING.index,
            IconLabel::TOP_PADDING.index,
            IconLabel::RIGHT_PADDING.index,
            IconLabel::BOTTOM_PADDING.index,
        ]
        .map(|p| num(engine, label, p)),
    };
    let (w, h) = layout.implicit();
    engine.batch(|| -> Result<(), EngineError> {
        set_computed(engine, label, Item::IMPLICIT_WIDTH.index, Value::Real(w))?;
        set_computed(engine, label, Item::IMPLICIT_HEIGHT.index, Value::Real(h))?;
        // After the implicit size: the label's own size may follow it.
        let size = (
            num(engine, label, Item::WIDTH.index),
            num(engine, label, Item::HEIGHT.index),
        );
        let (icon_at, text_at) = layout.place(size);
        for (item, at) in [(image, icon_at), (words, text_at)] {
            let (Some(item), Some(at)) = (item, at) else {
                continue;
            };
            for (p, v) in [
                (Item::X.index, at[0]),
                (Item::Y.index, at[1]),
                (Item::WIDTH.index, at[2]),
                (Item::HEIGHT.index, at[3]),
            ] {
                set_computed(engine, item, p, Value::Real(v))?;
            }
        }
        Ok(())
    })
}

/// A picture of the size `natural` made no larger than `limit`, keeping
/// its shape. It is never made larger than it is.
fn fitted(natural: (f64, f64), limit: (f64, f64)) -> (f64, f64) {
    if natural.0 <= 0.0 || natural.1 <= 0.0 {
        return (0.0, 0.0);
    }
    let mut scale: f64 = 1.0;
    if limit.0 > 0.0 {
        scale = scale.min(limit.0 / natural.0);
    }
    if limit.1 > 0.0 {
        scale = scale.min(limit.1 / natural.1);
    }
    ((natural.0 * scale).round(), (natural.1 * scale).round())
}

/// What a label lays out, and how. Sizes are (width, height); a rectangle
/// is `[x, y, width, height]`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Layout {
    icon: Option<(f64, f64)>,
    text: Option<(f64, f64)>,
    /// The text is under the picture, not beside it.
    under: bool,
    spacing: f64,
    mirrored: bool,
    alignment: i32,
    /// Left, top, right, bottom.
    padding: [f64; 4],
}

impl Layout {
    /// How much room the picture and the text need together.
    fn content(&self) -> (f64, f64) {
        match (self.icon, self.text) {
            (Some(i), Some(t)) if self.under => (i.0.max(t.0), i.1 + self.spacing + t.1),
            (Some(i), Some(t)) => (i.0 + self.spacing + t.0, i.1.max(t.1)),
            (Some(one), None) | (None, Some(one)) => one,
            (None, None) => (0.0, 0.0),
        }
    }

    fn implicit(&self) -> (f64, f64) {
        let (w, h) = self.content();
        let [l, t, r, b] = self.padding;
        (w + l + r, h + t + b)
    }

    /// Where the picture and the text go in a label of `size`. The picture
    /// is on whole pixels, as Qt has it; the text is where the sum puts
    /// it.
    fn place(&self, size: (f64, f64)) -> (Option<[f64; 4]>, Option<[f64; 4]>) {
        let [l, t, r, b] = self.padding;
        let room = ((size.0 - l - r).max(0.0), (size.1 - t - b).max(0.0));
        // A picture with less room than its size is made smaller to fit,
        // keeping its shape; the label still asks for its whole size.
        let this = Layout {
            icon: self.icon.map(|i| fitted(i, room)),
            ..*self
        };
        this.arrange(room)
    }

    fn arrange(&self, room: (f64, f64)) -> (Option<[f64; 4]>, Option<[f64; 4]>) {
        let [l, t, ..] = self.padding;
        let content = self.content();
        let (cw, ch) = (content.0.min(room.0), content.1.min(room.1));
        let (left, right) = if self.mirrored {
            (align::RIGHT, align::LEFT)
        } else {
            (align::LEFT, align::RIGHT)
        };
        let cx = if self.alignment & left != 0 {
            l
        } else if self.alignment & right != 0 {
            l + room.0 - cw
        } else {
            l + (room.0 - cw) / 2.0
        };
        let cy = if self.alignment & align::TOP != 0 {
            t
        } else if self.alignment & align::BOTTOM != 0 {
            t + room.1 - ch
        } else {
            t + (room.1 - ch) / 2.0
        };
        let whole = |at: [f64; 4]| [at[0].round(), at[1].round(), at[2], at[3]];
        match (self.icon, self.text) {
            (Some(i), Some(tx)) if self.under => {
                let tw = tx.0.min(cw);
                (
                    Some(whole([cx + (cw - i.0) / 2.0, cy, i.0, i.1])),
                    Some([cx + (cw - tw) / 2.0, cy + i.1 + self.spacing, tw, tx.1]),
                )
            }
            (Some(i), Some(tx)) => {
                let tw = tx.0.min((cw - i.0 - self.spacing).max(0.0));
                let (ix, tx_x) = if self.mirrored {
                    (cx + cw - i.0, cx)
                } else {
                    (cx, cx + i.0 + self.spacing)
                };
                (
                    Some(whole([ix, cy + (ch - i.1) / 2.0, i.0, i.1])),
                    Some([tx_x, cy + (ch - tx.1) / 2.0, tw, tx.1]),
                )
            }
            (Some(i), None) => (
                Some(whole([
                    cx + (cw - i.0) / 2.0,
                    cy + (ch - i.1) / 2.0,
                    i.0,
                    i.1,
                ])),
                None,
            ),
            (None, Some(tx)) => (None, Some([cx, cy, tx.0.min(cw), tx.1.min(ch.max(tx.1))])),
            (None, None) => (None, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beside(icon: (f64, f64), text: (f64, f64)) -> Layout {
        Layout {
            icon: Some(icon),
            text: Some(text),
            under: false,
            spacing: 6.0,
            mirrored: false,
            alignment: align::H_CENTER | align::V_CENTER,
            padding: [0.0; 4],
        }
    }

    // The numbers are what qml6 gives for a Button with a 16 by 16 icon
    // and the text "OK": a content item of 84 by 28.
    const TEXT: (f64, f64) = (21.546875, 19.0);

    #[test]
    fn a_picture_is_made_smaller_and_never_larger() {
        assert_eq!(fitted((16.0, 16.0), (24.0, 24.0)), (16.0, 16.0));
        assert_eq!(fitted((48.0, 48.0), (24.0, 24.0)), (24.0, 24.0));
        assert_eq!(fitted((64.0, 32.0), (24.0, 24.0)), (24.0, 12.0));
        assert_eq!(fitted((48.0, 48.0), (0.0, 0.0)), (48.0, 48.0));
        assert_eq!(fitted((48.0, 48.0), (12.0, 24.0)), (12.0, 12.0));
        assert_eq!(fitted((0.0, 0.0), (24.0, 24.0)), (0.0, 0.0));
    }

    #[test]
    fn a_picture_with_too_little_room_is_made_smaller_to_fit() {
        let l = Layout {
            icon: Some((38.0, 38.0)),
            text: None,
            padding: [8.0, 6.0, 8.0, 6.0],
            ..beside((0.0, 0.0), TEXT)
        };
        assert_eq!(l.implicit(), (54.0, 50.0));
        let (icon, _) = l.place((48.0, 38.0));
        assert_eq!(icon, Some([11.0, 6.0, 26.0, 26.0]));
    }

    #[test]
    fn the_text_is_beside_the_picture() {
        let l = beside((16.0, 16.0), TEXT);
        assert_eq!(l.implicit(), (43.546875, 19.0));
        let (icon, text) = l.place((84.0, 28.0));
        assert_eq!(icon, Some([20.0, 6.0, 16.0, 16.0]));
        assert_eq!(text, Some([42.2265625, 4.5, 21.546875, 19.0]));
    }

    #[test]
    fn a_mirrored_label_has_the_picture_after_the_text() {
        let l = Layout {
            mirrored: true,
            ..beside((16.0, 16.0), TEXT)
        };
        let (icon, text) = l.place((84.0, 28.0));
        assert_eq!(icon, Some([48.0, 6.0, 16.0, 16.0]));
        assert_eq!(text, Some([20.2265625, 4.5, 21.546875, 19.0]));
    }

    #[test]
    fn the_text_is_under_the_picture() {
        let l = Layout {
            under: true,
            ..beside((16.0, 16.0), TEXT)
        };
        assert_eq!(l.implicit(), (21.546875, 41.0));
        let (icon, text) = l.place((84.0, 41.0));
        assert_eq!(icon, Some([34.0, 0.0, 16.0, 16.0]));
        assert_eq!(text, Some([31.2265625, 22.0, 21.546875, 19.0]));
    }

    #[test]
    fn a_label_at_the_start_keeps_room_for_its_padding() {
        // A menu item that can be checked: 34 for the mark.
        let l = Layout {
            alignment: align::LEFT,
            padding: [34.0, 0.0, 0.0, 0.0],
            ..beside((16.0, 16.0), TEXT)
        };
        assert_eq!(l.implicit(), (77.546875, 19.0));
        let (icon, text) = l.place((188.0, 28.0));
        assert_eq!(icon, Some([34.0, 6.0, 16.0, 16.0]));
        assert_eq!(text, Some([56.0, 4.5, 21.546875, 19.0]));
    }

    #[test]
    fn a_text_with_too_little_room_is_narrower() {
        let l = beside((16.0, 16.0), TEXT);
        let (icon, text) = l.place((30.0, 28.0));
        assert_eq!(icon, Some([0.0, 6.0, 16.0, 16.0]));
        assert_eq!(text, Some([22.0, 4.5, 8.0, 19.0]));
    }
}
