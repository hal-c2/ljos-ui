//! The state types: `State`, `StateGroup`, and the changes a state is
//! made of.
//!
//! The objects only hold what QML wrote. What entering and leaving a
//! state does is in [`super::group`].

use ljos_engine::{
    Atom, Cx, EngineError, ForeignMember, ObjectId, PropIndex, QmlList, QmlObject, QtObject, Reset,
    Value, qml_impl,
};

use super::group;
use crate::item::{is_a, is_item};

/// `State` (QtQuick): a name and the changes that make the state.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "State",
    extends = QtObject,
    default = "changes"
)]
pub struct State {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub name: Atom,
    /// While true the item is in this state, if no state before it in the
    /// list has a true `when` as well.
    #[qml(notify)]
    pub when: bool,
    /// The state whose changes this one starts from.
    #[qml(notify)]
    pub extend: Atom,
    #[qml(list)]
    pub changes: QmlList,
}

#[qml_impl]
impl State {
    /// A state is complete before the item it is a state of: the item's
    /// states start from the first of them that is.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        if let Some(owner) = cx.engine.owner(cx.this)
            && (is_item(cx.engine, owner) || is_a::<StateGroup>(cx.engine, owner))
        {
            group::attach(cx.engine, owner);
        }
        Ok(())
    }
}

/// `StateGroup` (QtQuick): states for what is not an item, or a second
/// set of them for one that is.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "StateGroup", extends = QtObject)]
pub struct StateGroup {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub state: Atom,
    #[qml(notify, list)]
    pub states: QmlList,
    #[qml(notify, list)]
    pub transitions: QmlList,
}

#[qml_impl]
impl StateGroup {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        group::attach(cx.engine, cx.this);
        Ok(())
    }
}

/// `PropertyChanges` (QtQuick): values for properties of another object
/// while the state lasts.
///
/// What it is given are not properties of its own. The engine hands them
/// over as written ([`ljos_engine::Engine::register_foreign_members`]):
/// `width: 10` is of `target`, `box.width: 10` of the object with that id.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "PropertyChanges", extends = QtObject)]
pub struct PropertyChanges {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub target: Option<ObjectId>,
    /// An expression is worked out once, when the state is entered,
    /// instead of being followed while the state lasts.
    #[qml(notify)]
    pub explicit: bool,
    /// False: leaving the state leaves the properties as they are.
    #[qml(notify)]
    pub restore_entry_values: bool,
    /// In the order written.
    #[qml(skip)]
    pub(crate) members: Vec<ForeignMember>,
}

impl Default for PropertyChanges {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            target: None,
            explicit: false,
            restore_entry_values: true,
            members: Vec::new(),
        }
    }
}

/// `AnchorChanges` (QtQuick): other anchors for an item while the state
/// lasts.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "AnchorChanges", extends = QtObject)]
pub struct AnchorChanges {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub target: Option<ObjectId>,
    #[qml(group)]
    pub anchors: AnchorChangesAnchors,
    /// The lines that were taken back ([`LINE_RESET`]).
    #[qml(skip)]
    pub(crate) taken: Vec<PropIndex>,
}

impl AnchorChanges {
    /// Whether `line` (`anchors.left` of the change) was given `undefined`
    /// as a value: the state takes that anchor off its target.
    pub(crate) fn takes_off(&self, line: PropIndex) -> bool {
        self.taken.contains(&line)
    }
}

/// `anchors.left = undefined`, from a script or as the value written in
/// the change. Taking a line back does not make it one the change does not
/// name: the state then takes the anchor off its target, also for a line
/// the change never named, and a state that is in force keeps the anchor
/// it has until it is entered again (measured on Qt 6.11). So the line is
/// remembered apart from its value, which a line nobody named has too.
const LINE_RESET: Reset = Reset::Custom {
    apply: |engine, change, line| {
        engine.with_mut::<AnchorChanges, _>(change, |c| {
            if !c.taken.contains(&line) {
                c.taken.push(line);
            }
        })?;
        engine.reset_to(change, line, Value::Undefined)
    },
    value: |_, _, _| Value::Undefined,
};

/// The `anchors` group of [`AnchorChanges`]. A line that was given
/// anything, `undefined` included, is one the state changes: `undefined`
/// takes the anchor off.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "AnchorChangesAnchors", uncreatable)]
pub struct AnchorChangesAnchors {
    #[qml(notify, reset = LINE_RESET)]
    pub left: Value,
    #[qml(notify, reset = LINE_RESET)]
    pub right: Value,
    #[qml(notify, reset = LINE_RESET)]
    pub top: Value,
    #[qml(notify, reset = LINE_RESET)]
    pub bottom: Value,
    #[qml(notify, reset = LINE_RESET)]
    pub horizontal_center: Value,
    #[qml(notify, reset = LINE_RESET)]
    pub vertical_center: Value,
    #[qml(notify, reset = LINE_RESET)]
    pub baseline: Value,
}

/// `ParentChange` (QtQuick): another parent for an item while the state
/// lasts. The item stays where it is on screen unless the change says
/// where it goes.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "ParentChange", extends = QtObject)]
pub struct ParentChange {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub target: Option<ObjectId>,
    #[qml(notify)]
    pub parent: Option<ObjectId>,
    /// `undefined`, as the five below: not given.
    #[qml(notify)]
    pub x: Value,
    #[qml(notify)]
    pub y: Value,
    #[qml(notify)]
    pub width: Value,
    #[qml(notify)]
    pub height: Value,
    #[qml(notify)]
    pub scale: Value,
    #[qml(notify)]
    pub rotation: Value,
}

/// `StateChangeScript` (QtQuick): a statement that runs when the state is
/// entered, at its place among the changes.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "StateChangeScript", extends = QtObject)]
pub struct StateChangeScript {
    #[qml(base)]
    pub object: QtObject,
    /// What a `ScriptAction` of a transition names to run the script in
    /// its own time instead.
    #[qml(notify)]
    pub name: Atom,
    /// Kept by the engine as a statement; the field holds nothing.
    #[qml(notify)]
    pub script: Value,
}
