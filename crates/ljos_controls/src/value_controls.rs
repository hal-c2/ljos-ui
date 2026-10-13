//! Registers the value controls and the delegates built on the buttons:
//! the sliders, the dial, the bars and indicators, and the rows of a list
//! that check, choose or switch.

use ljos_a11y::Role;
use ljos_engine::{Engine, MetaRef, ObjectId, QmlType, Version};

use crate::action_group::ActionGroup;
use crate::bars::{BusyIndicator, ProgressBar, ToolSeparator};
use crate::delay_button::{DelayButton, RoundButton};
use crate::delegates::{CheckDelegate, RadioDelegate, SwitchDelegate};
use crate::dial::Dial;
use crate::page_indicator::PageIndicator;
use crate::slider::{RangeSlider, Slider};
use crate::util::is_a;

pub(crate) fn register(engine: &Engine) {
    for meta in [
        Slider::static_meta(),
        RangeSlider::static_meta(),
        ProgressBar::static_meta(),
        BusyIndicator::static_meta(),
        ToolSeparator::static_meta(),
        Dial::static_meta(),
        DelayButton::static_meta(),
        RoundButton::static_meta(),
        PageIndicator::static_meta(),
        CheckDelegate::static_meta(),
        RadioDelegate::static_meta(),
        SwitchDelegate::static_meta(),
    ] {
        engine.register_type(
            "QtQuick.Templates",
            Version::new(2, 15),
            MetaRef::Static(meta),
        );
    }
    // A group of actions has no look of its own: the template is the type.
    for uri in std::iter::once("QtQuick.Templates").chain(crate::CONTROLS) {
        engine.register_type(
            uri,
            Version::new(2, 15),
            MetaRef::Static(ActionGroup::static_meta()),
        );
    }
    crate::action_group::register(engine);
    crate::slider::register(engine);
    crate::bars::register(engine);
    crate::dial::register(engine);
    crate::page_indicator::register(engine);
}

/// What the click that ends a press does to the check of a button that
/// has its own way with it: a delay button, a check delegate. False for
/// any other button, which toggles.
pub(crate) fn click(engine: &Engine, obj: ObjectId) -> bool {
    if is_a::<DelayButton>(engine, obj) {
        crate::delay_button::click(engine, obj);
    } else if is_a::<CheckDelegate>(engine, obj) {
        crate::delegates::advance(engine, obj);
    } else {
        return false;
    }
    true
}

/// The role of a delegate that is a check box, a radio button or a switch
/// in a list: what it works like rather than where it is.
pub(crate) fn button_role(engine: &Engine, obj: ObjectId) -> Option<Role> {
    if is_a::<CheckDelegate>(engine, obj) {
        Some(Role::CheckBox)
    } else if is_a::<RadioDelegate>(engine, obj) {
        Some(Role::RadioButton)
    } else if is_a::<SwitchDelegate>(engine, obj) {
        Some(Role::Switch)
    } else {
        None
    }
}
