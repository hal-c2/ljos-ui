//! What the application shows at one moment, copied out of the scene so
//! that an answer can be written without the scene (and over several
//! frames).
//!
//! The outline is the accessibility tree: what an assistive technology is
//! told is what an agent is told. The scene builds that tree only while
//! something asks for it, so the inspector switches it on for one request
//! and gives it back afterwards ([`Lease`]).

use std::collections::{HashMap, HashSet};

use ljos_a11y::{NodeKey, accesskit::Toggled, role_word};
use ljos_engine::{Engine, ObjectId, Value};
use ljos_mcp_core::{NodeValue, SnapNode, SnapTree};
use ljos_quick::QuickScene;
use ljos_runtime::Clock;

/// What a masked value is written as outside the outline.
pub(crate) const REDACTED: &str = "<redacted>";

/// The accessibility tree, held for the length of one request.
#[derive(Debug)]
pub(crate) struct Lease {
    /// Whether somebody else (a screen reader, a test) keeps the tree.
    held: bool,
    screen_reader: bool,
}

impl Lease {
    /// Makes sure the scene has an accessibility tree that is up to date.
    pub(crate) fn take(engine: &Engine, scene: &mut QuickScene, clock: &dyn Clock) -> Lease {
        let held = scene.accessibility_active();
        let screen_reader = ljos_quick::system_prefs(engine).screen_reader_active;
        if !held {
            scene.set_accessibility_active(engine, true);
            // Switching the tree on tells the application that a screen
            // reader is listening. An inspector is not one, and an
            // application that changes what it shows for a screen reader
            // must not change it because somebody looked.
            ljos_quick::update_system_prefs(engine, |p| p.screen_reader_active = screen_reader);
        }
        if !held || scene.needs_frame() || scene.snapshot().is_none() {
            scene.frame(engine, clock.now());
            // The host has not painted this frame: it must not think the
            // scene is as it last painted it.
            scene.request_frame();
        }
        Lease {
            held,
            screen_reader,
        }
    }

    /// Gives the tree back, so that a scene nobody else listens to does no
    /// accessibility work again.
    pub(crate) fn release(self, engine: &Engine, scene: &mut QuickScene) {
        if !self.held {
            scene.set_accessibility_active(engine, false);
            let was = self.screen_reader;
            ljos_quick::update_system_prefs(engine, |p| p.screen_reader_active = was);
        }
    }
}

#[derive(Debug)]
pub(crate) struct Capture {
    pub(crate) tree: SnapTree,
    /// The item behind each node of `tree`.
    pub(crate) items: Vec<Option<ObjectId>>,
    /// The node the keyboard is at.
    pub(crate) focus: Option<usize>,
    /// What the fields that must not be shown hold. A value that is one of
    /// these is masked wherever it turns up (a label bound to a password's
    /// text, an alias of it).
    pub(crate) secrets: HashSet<String>,
    /// The fields whose own text is masked.
    pub(crate) secret_items: HashSet<ObjectId>,
}

impl Capture {
    pub(crate) fn node(&self, index: usize) -> &SnapNode {
        &self.tree.nodes()[index]
    }

    /// `text`, or the mask when it is something that must not be shown.
    pub(crate) fn shown<'a>(&self, text: &'a str) -> &'a str {
        if !text.is_empty() && self.secrets.contains(text) {
            REDACTED
        } else {
            text
        }
    }
}

fn truthy(v: &Value) -> bool {
    !matches!(v, Value::Undefined | Value::Null) && v.to_boolean()
}

/// Whether `obj` is a field that hides what is typed into it: any
/// `echoMode` but the normal one (0).
pub(crate) fn hides_its_text(engine: &Engine, obj: ObjectId) -> bool {
    match engine.read_by_name(obj, "echoMode") {
        Value::Undefined | Value::Null => false,
        v => v.to_number() != 0.0,
    }
}

/// Whether `obj` holds text a person types (`TextInput`, `TextEdit` and
/// what is made from them).
pub(crate) fn is_editor(engine: &Engine, obj: ObjectId) -> bool {
    engine.property_index(obj, "cursorPosition").is_ok()
        && engine.property_index(obj, "text").is_ok()
}

fn states(d: &ljos_a11y::NodeData, focused: bool) -> Vec<&'static str> {
    let mut out = Vec::new();
    if focused {
        out.push("focused");
    }
    if d.disabled {
        out.push("disabled");
    }
    match d.toggled {
        Some(Toggled::True) => out.push("checked"),
        Some(Toggled::Mixed) => out.push("mixed"),
        Some(Toggled::False) | None => {}
    }
    match d.expanded {
        Some(true) => out.push("expanded"),
        Some(false) => out.push("collapsed"),
        None => {}
    }
    if d.selected == Some(true) {
        out.push("selected");
    }
    if d.pressed {
        out.push("pressed");
    }
    if d.modal {
        out.push("modal");
    }
    if d.read_only || d.text.as_ref().is_some_and(|t| t.read_only) {
        out.push("readonly");
    }
    out
}

/// Whether the text of `obj` must not be shown: it hides what is typed, or
/// it is an editor and every editor is masked.
pub(crate) fn is_secret(engine: &Engine, obj: ObjectId, redact_all: bool) -> bool {
    engine.untrack(|| hides_its_text(engine, obj) || (redact_all && is_editor(engine, obj)))
}

/// Copies what `scene` shows. The scene's accessibility tree must be up to
/// date ([`Lease::take`]). Nothing read here is tracked.
pub(crate) fn capture(engine: &Engine, scene: &QuickScene, redact_all: bool) -> Capture {
    engine.untrack(|| capture_untracked(engine, scene, redact_all))
}

fn capture_untracked(engine: &Engine, scene: &QuickScene, redact_all: bool) -> Capture {
    let root = scene.root();
    let mut secrets = HashSet::new();
    let mut secret_items = HashSet::new();
    // Every item, shown or not: which fields are secret, and how many
    // items are not shown below each exposed one.
    let mut hidden: HashMap<u64, usize> = HashMap::new();
    if let Some(root) = root {
        // (item, nearest exposed ancestor, inside something not shown)
        let mut stack = vec![(root, NodeKey::ROOT, false)];
        let mut seen = 0usize;
        while let Some((obj, mut above, mut unseen)) = stack.pop() {
            seen += 1;
            if seen > 1_000_000 {
                // A corrupt parent cycle must not hang the inspector.
                break;
            }
            if hides_its_text(engine, obj) || (redact_all && is_editor(engine, obj)) {
                secret_items.insert(obj);
                for prop in ["text", "displayText"] {
                    if let Value::String(s) = engine.read_by_name(obj, prop)
                        && !s.is_empty()
                    {
                        secrets.insert(s.to_string());
                    }
                }
            }
            if !unseen && obj != root && !truthy(&engine.read_by_name(obj, "visible")) {
                unseen = true;
            }
            if unseen {
                *hidden.entry(above.0).or_default() += 1;
            } else if let Some(key) = scene.accessible_node(obj) {
                above = key;
            }
            for child in engine.visual_children(obj).into_iter().rev() {
                stack.push((child, above, unseen));
            }
        }
    }

    let a11y = scene.accessibility_tree();
    let focus_key = scene.accessible_focus_node();
    let viewport = scene.viewport();
    let mut tree = SnapTree::new();
    let mut items = Vec::with_capacity(a11y.len() + 1);
    let mut index_of: HashMap<u64, usize> = HashMap::with_capacity(a11y.len() + 1);
    let mut focus = None;

    let mut window = SnapNode::new(NodeKey::ROOT.0, "window");
    window.bounds = [0.0, 0.0, viewport.width(), viewport.height()];
    window.size = Some((
        viewport.width().round().max(0.0) as u32,
        viewport.height().round().max(0.0) as u32,
    ));
    if let Some(root) = root
        && let Value::String(title) = engine.read_by_name(root, "title")
    {
        window.name = title.to_string();
    }
    window.hidden_below = hidden.get(&NodeKey::ROOT.0).copied().unwrap_or(0);
    index_of.insert(NodeKey::ROOT.0, tree.push(None, window));
    items.push(root);

    let mask = |text: &str| -> String {
        if !text.is_empty() && secrets.contains(text) {
            REDACTED.to_string()
        } else {
            text.to_string()
        }
    };
    a11y.walk(|key, parent, d| {
        // An announcement is something said, not something shown.
        if key.is_announcement() {
            return;
        }
        let Some(&parent) = index_of.get(&parent.0) else {
            return;
        };
        let obj = scene.accessible_item(key);
        let is_focus = focus_key == Some(key);
        let mut node = SnapNode::new(key.0, role_word(d.role));
        node.name = mask(&d.name);
        node.description = mask(&d.description);
        if !d.author_id.is_empty() {
            node.object_name = Some(d.author_id.clone());
        }
        node.type_name = obj
            .and_then(|o| engine.meta_of(o))
            .map(|m| m.name.to_string());
        node.bounds = [
            d.bounds.x0,
            d.bounds.y0,
            d.bounds.x1 - d.bounds.x0,
            d.bounds.y1 - d.bounds.y0,
        ];
        node.states = states(d, is_focus);
        if let (Some(pos), Some(of)) = (d.position_in_set, d.size_of_set) {
            node.attrs.push(("pos", format!("{pos}/{of}")));
        }
        if let Some(level) = d.level {
            node.attrs.push(("level", level.to_string()));
        }
        node.actionable = d.actionable();
        let secret = obj.is_some_and(|o| secret_items.contains(&o))
            || d.role == ljos_a11y::accesskit::Role::PasswordInput;
        node.value = if secret {
            Some(NodeValue::Redacted)
        } else if let Some(text) = &d.text {
            Some(if secrets.contains(&text.value) && !text.value.is_empty() {
                NodeValue::Redacted
            } else {
                NodeValue::Text(text.value.clone())
            })
        } else {
            d.range.map(|r| NodeValue::Number(r.value))
        };
        node.hidden_below = hidden.get(&key.0).copied().unwrap_or(0);
        let index = tree.push(Some(parent), node);
        index_of.insert(key.0, index);
        items.push(obj);
        if is_focus {
            focus = Some(index);
        }
    });

    // One pass over the loaded files for every `id`, instead of one per node.
    let objs: Vec<ObjectId> = items.iter().skip(1).flatten().copied().collect();
    let mut ids = engine.inspect_ids(&objs).into_iter();
    for (index, item) in items.iter().enumerate().skip(1) {
        if item.is_some()
            && let Some(Some(id)) = ids.next()
            && let Some(node) = tree.get_mut(index)
        {
            node.qml_id = Some(id);
        }
    }

    Capture {
        tree,
        items,
        focus,
        secrets,
        secret_items,
    }
}

/// Every object below the scene's root, items or not, each once: where an
/// `#objectName` or an `id=` is looked for when the outline has no such
/// node (an item with no role, or one that is not shown).
pub(crate) fn all_objects(engine: &Engine, scene: &QuickScene) -> Vec<ObjectId> {
    let Some(root) = scene.root() else {
        return Vec::new();
    };
    engine.untrack(|| {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(obj) = stack.pop() {
            if !seen.insert(obj) || !engine.is_alive(obj) {
                continue;
            }
            out.push(obj);
            let mut below = engine.visual_children(obj);
            below.extend(engine.children(obj));
            stack.extend(below.into_iter().rev());
        }
        out
    })
}
