//! A plain `Text`, `TextInput` or `TextEdit` follows the application font
//! where it names nothing of its own.
//!
//! Such an item starts from the application font when it is made
//! ([`Font::default`]). Each one is kept in a list when it completes, so
//! that a change of the application font reaches the text items and no
//! other: nothing walks the tree, and nothing is done for a text item
//! until the font changes. Dead items leave the list as it grows.
//!
//! What follows is an attribute an item has as the old application font
//! had it; any other was named by the item's author and stays. Pinned by
//! `features/controls/application-font.feature`.

use std::cell::RefCell;
use std::sync::Arc;

use ljos_engine::{Engine, Font, ObjectId, PropIndex, Value};

#[derive(Default)]
struct TextItems {
    items: RefCell<Vec<(ObjectId, PropIndex)>>,
    /// How many were alive at the last sweep: the list is swept again
    /// when it has grown to twice that.
    swept: std::cell::Cell<usize>,
}

/// Keeps `obj`'s font `prop` following the application font.
pub(crate) fn follow(engine: &Engine, obj: ObjectId, prop: PropIndex) {
    let all = engine.extension::<TextItems>();
    let mut items = all.items.borrow_mut();
    if items.len() >= 2 * all.swept.get().max(32) {
        items.retain(|(o, _)| engine.is_alive(*o));
        all.swept.set(items.len());
    }
    items.push((obj, prop));
}

/// The application font changed from `old` to `new`: every plain text item
/// takes `new`'s attributes where it has `old`'s. `skip` says which items
/// follow it some other way (a control's label resolves its own font).
pub fn application_font_changed(
    engine: &Engine,
    old: &Font,
    new: &Font,
    skip: impl Fn(ObjectId) -> bool,
) {
    let all = engine.extension::<TextItems>();
    let items = {
        let mut items = all.items.borrow_mut();
        items.retain(|(o, _)| engine.is_alive(*o));
        all.swept.set(items.len());
        items.clone()
    };
    for (obj, prop) in items {
        // A binding gives the whole font each time it runs.
        if skip(obj) || engine.is_bound(obj, prop) {
            continue;
        }
        let Value::Font(now) = engine.untrack(|| engine.read(obj, prop)) else {
            continue;
        };
        let font = followed(&now, old, new);
        if font != *now {
            let _ = engine.write_derived(obj, prop, Value::Font(Arc::new(font)));
        }
    }
}

/// `font` with what it has of `old` replaced by `new`'s. The two sizes go
/// together, since one of them is unset (`-1`) when the other is given.
fn followed(font: &Font, old: &Font, new: &Font) -> Font {
    let mut f = font.clone();
    if f.family == old.family {
        f.family = new.family.clone();
    }
    if f.point_size == old.point_size && f.pixel_size == old.pixel_size {
        f.point_size = new.point_size;
        f.pixel_size = new.pixel_size;
    }
    if f.weight == old.weight {
        f.weight = new.weight;
    }
    if f.italic == old.italic {
        f.italic = new.italic;
    }
    if f.style_name == old.style_name {
        f.style_name = new.style_name.clone();
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_an_item_named_stays_and_the_rest_follows() {
        let old = Font::process_default();
        let new = Font {
            family: "Literata".into(),
            weight: 600,
            ..old.clone()
        };
        let own = Font {
            weight: 300,
            ..old.clone()
        };
        let f = followed(&own, &old, &new);
        assert_eq!(f.family, new.family);
        assert_eq!(f.weight, 300);
        let sized = Font {
            pixel_size: 20,
            point_size: -1.0,
            ..old.clone()
        };
        let f = followed(&sized, &old, &new);
        assert_eq!((f.pixel_size, f.point_size), (20, -1.0));
    }
}
