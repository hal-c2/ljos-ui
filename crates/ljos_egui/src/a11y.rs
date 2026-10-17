//! The scene's accessibility tree, written into egui's AccessKit tree.
//!
//! egui owns the window's tree and rebuilds it every pass, so while an
//! assistive technology listens the view repeats its nodes every pass. The
//! scene's tree is retained and incremental; only this copy is per pass,
//! and it does not happen at all while nothing listens.

use std::collections::HashMap;

use egui::accesskit::{self, Role};
use egui::{Id, Pos2, Rect, Ui, UiBuilder};
use ljos_a11y::{ColorScheme, HostPrefs, NodeKey, TextValue, Tree};
use ljos_engine::Engine;
use ljos_quick::{AccessibleAction, AccessibleTextAction};

/// Hands the application what egui knows of the system's preferences:
/// the colour scheme, when the platform reports one. (That an assistive
/// technology listens is told where the tree is switched on.) Contrast,
/// reduced motion and the text size egui does not hear of; the shell reads
/// them from the platform and reports them itself.
///
/// It reports only when egui's answer changed, so a pass in which nothing
/// changed costs one comparison. A scheme an application or a test set
/// stays over it (`ljos_quick::report_system_prefs`).
#[derive(Debug, Default)]
pub(crate) struct SystemFeed {
    told: Option<egui::Theme>,
}

impl SystemFeed {
    pub(crate) fn feed(&mut self, ctx: &egui::Context, engine: &Engine) {
        let system = ctx.system_theme();
        if system == self.told {
            return;
        }
        self.told = system;
        let scheme = match system {
            Some(egui::Theme::Dark) => ColorScheme::Dark,
            Some(egui::Theme::Light) => ColorScheme::Light,
            // The platform stopped saying: what was last said stays.
            None => return,
        };
        ljos_quick::report_system_prefs(
            engine,
            HostPrefs {
                color_scheme: Some(scheme),
                ..HostPrefs::default()
            },
        );
    }
}

/// AccessKit ids of the nodes of the last pass, for routing actions back.
pub(crate) type Emitted = HashMap<accesskit::NodeId, NodeKey>;

/// What a request of an assistive technology asks of an item's type, for
/// the requests that are neither a press nor the focus. A scroll with no
/// unit named goes one step, not a page.
pub(crate) fn item_action(req: &accesskit::ActionRequest) -> Option<AccessibleAction> {
    use accesskit::{Action, ActionData, ScrollUnit};
    let page = matches!(req.data, Some(ActionData::ScrollUnit(ScrollUnit::Page)));
    match req.action {
        Action::Increment => Some(AccessibleAction::Increase),
        Action::Decrement => Some(AccessibleAction::Decrease),
        Action::SetValue => match &req.data {
            Some(ActionData::NumericValue(to)) => Some(AccessibleAction::SetValue(*to)),
            _ => None,
        },
        Action::ScrollUp => Some(AccessibleAction::ScrollUp { page }),
        Action::ScrollDown => Some(AccessibleAction::ScrollDown { page }),
        Action::ScrollLeft => Some(AccessibleAction::ScrollLeft { page }),
        Action::ScrollRight => Some(AccessibleAction::ScrollRight { page }),
        Action::ScrollIntoView => Some(AccessibleAction::ShowOnScreen),
        Action::Expand => Some(AccessibleAction::Expand),
        Action::Collapse => Some(AccessibleAction::Collapse),
        _ => None,
    }
}

/// AccessKit ids of the text runs below the editors' nodes of the last
/// pass: an assistive technology names a place in a text by a run and a
/// character in it.
pub(crate) type TextRuns = HashMap<accesskit::NodeId, (NodeKey, usize)>;

/// What a request of an assistive technology asks of the text of the
/// node `key`: a selection, or other text in place of the selection or of
/// all of it. `owner` says whose run an id is, and which.
pub(crate) fn text_action(
    req: &accesskit::ActionRequest,
    key: NodeKey,
    tree: &Tree,
    owner: impl Fn(accesskit::NodeId) -> Option<(NodeKey, usize)>,
) -> Option<AccessibleTextAction> {
    use accesskit::{Action, ActionData};
    let text = tree.get(key)?.text.as_ref()?;
    match (&req.action, &req.data) {
        (Action::SetTextSelection, Some(ActionData::SetTextSelection(selection))) => {
            let cut = text.runs();
            // A run of another node names no place in this one.
            let run_of = |id| owner(id).filter(|(k, _)| *k == key).map(|(_, i)| i);
            let (anchor, cursor) = TextValue::bytes_of(&cut, selection, run_of)?;
            Some(AccessibleTextAction::Select { anchor, cursor })
        }
        (Action::ReplaceSelectedText, Some(ActionData::Value(to))) => {
            Some(AccessibleTextAction::ReplaceSelection(to.to_string()))
        }
        (Action::SetValue, Some(ActionData::Value(to))) => {
            Some(AccessibleTextAction::SetValue(to.to_string()))
        }
        _ => None,
    }
}

/// The egui id of run `index` of the text of a node.
fn run_id(view: Id, key: NodeKey, index: usize) -> Id {
    Id::new((view, key.0, "run", index))
}

/// Whether egui is building an AccessKit tree this pass.
pub(crate) fn listening(ui: &Ui) -> bool {
    // A `Ui` always has a node of its own. Asking for an id egui has not
    // seen would make a stray node under the window, so ours is not asked.
    ui.ctx()
        .accesskit_node_builder(ui.unique_id(), |_| ())
        .is_some()
}

/// The egui id of a node: the same item in the same view keeps it for as
/// long as it lives, wherever it moves in the tree.
pub(crate) fn node_id(view: Id, key: NodeKey) -> Id {
    Id::new((view, key.0))
}

/// Writes every node of `tree` under a node for the view. `rect` is where
/// the scene is painted. `holder` is the widget that has egui's focus for
/// the scene; the nodes go below it, so that it can point at the one with
/// the scene's focus.
pub(crate) fn emit(
    ui: &mut Ui,
    view: Id,
    holder: Id,
    rect: Rect,
    tree: &Tree,
    emitted: &mut Emitted,
    runs: &mut TextRuns,
) {
    emitted.clear();
    runs.clear();
    let ctx = ui.ctx().clone();
    // egui learns a node's parent from the `Ui` that carries its id, so
    // every node gets one. They paint nothing and take no space.
    let mut host = ui.new_child(
        UiBuilder::new()
            .id(view)
            .max_rect(rect)
            .accessibility_parent(holder),
    );
    // Parents come before children and siblings in order, which is the
    // order egui lists children in.
    tree.walk(|key, parent, data| {
        let id = node_id(view, key);
        let parent_id = if parent == NodeKey::ROOT {
            view
        } else {
            node_id(view, parent)
        };
        let b = data.bounds;
        let min = Pos2::new(rect.min.x + b.x0 as f32, rect.min.y + b.y0 as f32);
        let max = Pos2::new(rect.min.x + b.x1 as f32, rect.min.y + b.y1 as f32);
        let _node_ui = host.new_child(
            UiBuilder::new()
                .id(id)
                .max_rect(Rect::from_min_max(min, max))
                .accessibility_parent(parent_id),
        );
        // The closure runs under the context lock: it only copies.
        ctx.accesskit_node_builder(id, |node| {
            data.apply(node);
            // A label that is not in the tree (hidden, ignored) names
            // nothing an assistive technology could go to.
            data.apply_relations(node, |k| {
                tree.contains(k).then(|| node_id(view, k).accesskit_id())
            });
            // Logical points; egui's root node scales to pixels.
            node.set_bounds(accesskit::Rect {
                x0: f64::from(min.x),
                y0: f64::from(min.y),
                x1: f64::from(max.x),
                y1: f64::from(max.y),
            });
        });
        emitted.insert(id.accesskit_id(), key);
        // AccessKit reads a text that can be edited from runs below its
        // node, and the selection names places in them.
        let Some(text) = &data.text else {
            return;
        };
        let cut = text.runs();
        for (index, run) in cut.iter().enumerate() {
            let rid = run_id(view, key, index);
            let _run_ui = host.new_child(
                UiBuilder::new()
                    .id(rid)
                    .max_rect(Rect::from_min_max(min, max))
                    .accessibility_parent(id),
            );
            ctx.accesskit_node_builder(rid, |node| run.apply(&text.value, node));
            runs.insert(rid.accesskit_id(), (key, index));
        }
        let selection = text.selection(&cut, |index| run_id(view, key, index).accesskit_id());
        ctx.accesskit_node_builder(id, |node| node.set_text_selection(selection));
    });
    // egui's focus stays on the holder whichever item has the scene's; an
    // assistive technology follows the holder's active descendant to it.
    let focus = tree
        .focus()
        .map(|key| node_id(view, key).accesskit_id())
        .filter(|id| emitted.contains_key(id));
    ctx.accesskit_node_builder(holder, |node| {
        node.set_role(Role::Group);
        if let Some(focus) = focus {
            node.set_active_descendant(focus);
        }
    });
}

/// The node of a grafted tree a request names: the scene's own keys are
/// its ids there. A run of a text stands for the node whose text it is.
pub(crate) fn grafted_key(tree: &Tree, id: accesskit::NodeId) -> Option<NodeKey> {
    let key = NodeKey(id.0);
    if key != NodeKey::ROOT && tree.contains(key) {
        return Some(key);
    }
    tree.text_run(id).map(|(owner, _)| owner)
}

/// Makes the node of `holder` the place where the scene's own AccessKit
/// tree `tree` hangs in egui's, instead of copying the scene's nodes into
/// egui's tree every pass. `rect` is where the scene is painted: the
/// nodes of the grafted tree keep the scene's coordinates and the graft
/// moves them there.
///
/// `holder` is the widget that has egui's focus for the scene: the focus
/// of the grafted tree counts while the graft node has the focus of the
/// tree around it.
pub(crate) fn graft(ctx: &egui::Context, holder: Id, rect: Rect, tree: accesskit::TreeId) {
    ctx.accesskit_node_builder(holder, |node| {
        node.set_role(Role::Group);
        // In the node's own space, which the transform puts in the window
        // (logical points; egui's root node scales to pixels).
        node.set_bounds(accesskit::Rect {
            x0: 0.0,
            y0: 0.0,
            x1: f64::from(rect.width()),
            y1: f64::from(rect.height()),
        });
        node.set_transform(accesskit::Affine::translate((
            f64::from(rect.min.x),
            f64::from(rect.min.y),
        )));
        // A graft node has the root of its tree for a child and no other.
        node.clear_children();
        node.set_tree_id(tree);
    });
}
