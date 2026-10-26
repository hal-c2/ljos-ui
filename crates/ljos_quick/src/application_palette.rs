//! The application's palette: the colours of what no item's own palette
//! reaches. In Qt that is `QGuiApplication::palette()`, which a host sets
//! with `setPalette` and QML cannot write; an item's `palette` does not
//! change it (measured on Qt 6.11: `palette.link` on a `Text` or on the
//! item it is in leaves its rich-text links as they were).
//!
//! Of its roles Ljós reads the link colours: a link of rich text or of
//! Markdown that its document gives no colour is drawn in
//! [`ApplicationPalette::link`] (features/text/rich-text-link-colour.feature).
//! `Text.linkColor` is for `StyledText` only, as in Qt.
//!
//! Nothing polls: a host sets the palette, and what read it runs again.

use std::cell::Cell;

use ljos_engine::{Color, Engine, NodeId, Value};

/// The colours a host gives the application. What it does not set is
/// Qt's own default palette, the one qml6 has on a platform without a
/// theme (offscreen).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ApplicationPalette {
    /// A link the document gives no colour (`QPalette::Link`).
    pub link: Color,
    /// A link that was visited (`QPalette::LinkVisited`).
    pub link_visited: Color,
}

impl Default for ApplicationPalette {
    fn default() -> Self {
        ApplicationPalette {
            link: Color::from_rgba8(0, 0, 0xff, 0xff),
            link_visited: Color::from_rgba8(0xff, 0, 0xff, 0xff),
        }
    }
}

impl ApplicationPalette {
    /// The default palette with another link colour.
    pub fn with_link(self, link: Color) -> Self {
        ApplicationPalette { link, ..self }
    }

    /// The default palette with another colour for visited links.
    pub fn with_link_visited(self, link_visited: Color) -> Self {
        ApplicationPalette {
            link_visited,
            ..self
        }
    }
}

/// Per engine: the palette, and a node a tracked read depends on.
#[derive(Default)]
struct State {
    palette: Cell<ApplicationPalette>,
    node: Cell<Option<NodeId>>,
}

fn node(engine: &Engine, s: &State) -> Option<NodeId> {
    if let Some(n) = s.node.get() {
        return Some(n);
    }
    let g = engine.graph();
    let n = g.property(g.root(), Value::Int(0)).ok()?;
    s.node.set(Some(n));
    Some(n)
}

/// The application's palette. Tracked: what reads it runs again when a
/// host sets another.
pub fn application_palette(engine: &Engine) -> ApplicationPalette {
    let s = engine.extension::<State>();
    if let Some(n) = node(engine, &s) {
        let _ = engine.graph().get(n);
    }
    s.palette.get()
}

/// Sets the application's palette, as a host does for its theme. Setting
/// the same again costs nothing.
pub fn set_application_palette(engine: &Engine, palette: ApplicationPalette) {
    let s = engine.extension::<State>();
    if s.palette.replace(palette) == palette {
        return;
    }
    if let Some(n) = node(engine, &s) {
        let g = engine.graph();
        let next = (g.get_untracked(n).to_number() as i32).wrapping_add(1);
        g.set(n, Value::Int(next));
    }
}
