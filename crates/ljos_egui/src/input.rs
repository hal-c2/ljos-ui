//! egui pointer vocabulary to `ljos_quick`'s egui-free one.

use egui::PointerButton;
use egui::os::OperatingSystem;
use ljos_quick::{Modifiers, MouseButtons};

/// egui's five buttons map one-to-one onto Qt's.
pub fn map_button(b: PointerButton) -> MouseButtons {
    match b {
        PointerButton::Primary => MouseButtons::LEFT,
        PointerButton::Secondary => MouseButtons::RIGHT,
        PointerButton::Middle => MouseButtons::MIDDLE,
        PointerButton::Extra1 => MouseButtons::BACK,
        PointerButton::Extra2 => MouseButtons::FORWARD,
    }
}

/// Qt on macOS reports Command as `ControlModifier` and Control as
/// `MetaModifier`, so QML written for "Ctrl+click" works with Cmd there.
pub fn map_modifiers(m: egui::Modifiers, os: OperatingSystem) -> Modifiers {
    let mut out = Modifiers::empty();
    if m.shift {
        out |= Modifiers::SHIFT;
    }
    if m.alt {
        out |= Modifiers::ALT;
    }
    if os == OperatingSystem::Mac {
        if m.mac_cmd {
            out |= Modifiers::CONTROL;
        }
        if m.ctrl {
            out |= Modifiers::META;
        }
    } else if m.ctrl {
        out |= Modifiers::CONTROL;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_map_one_to_one() {
        assert_eq!(map_button(PointerButton::Primary), MouseButtons::LEFT);
        assert_eq!(map_button(PointerButton::Secondary), MouseButtons::RIGHT);
        assert_eq!(map_button(PointerButton::Middle), MouseButtons::MIDDLE);
        assert_eq!(map_button(PointerButton::Extra1), MouseButtons::BACK);
        assert_eq!(map_button(PointerButton::Extra2), MouseButtons::FORWARD);
    }

    #[test]
    fn mac_swaps_command_and_control() {
        let cmd = egui::Modifiers::MAC_CMD;
        let ctrl = egui::Modifiers::CTRL;
        assert_eq!(map_modifiers(cmd, OperatingSystem::Mac), Modifiers::CONTROL);
        assert_eq!(map_modifiers(ctrl, OperatingSystem::Mac), Modifiers::META);
        assert_eq!(
            map_modifiers(ctrl, OperatingSystem::Nix),
            Modifiers::CONTROL
        );
        assert_eq!(
            map_modifiers(
                egui::Modifiers::SHIFT | egui::Modifiers::ALT,
                OperatingSystem::Nix
            ),
            Modifiers::SHIFT | Modifiers::ALT
        );
    }
}
