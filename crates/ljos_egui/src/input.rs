//! egui pointer vocabulary to `ljos_quick`'s egui-free one.

use egui::os::OperatingSystem;
use egui::{MouseWheelSource, MouseWheelUnit, PointerButton, TouchPhase, Vec2};
use ljos_quick::{Modifiers, MouseButtons, ScrollPhase, WheelEvent};
use ljos_render::kurbo;

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

/// Whether the device that scrolls is inside a gesture it announced. Only
/// then do its events have phases; a wheel that turns in notches sends
/// every event as a move with nothing around it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WheelTrack {
    in_gesture: bool,
}

/// What a scroll event of egui says, in the terms of [`WheelEvent`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scroll {
    pub angle_delta: kurbo::Vec2,
    pub pixel_delta: kurbo::Vec2,
    pub phase: ScrollPhase,
}

/// egui and Qt agree on the signs: a positive delta shows what is further
/// up or further left. They differ in the units. A line of egui is a notch
/// of the wheel (winit reports one line a notch), a point is a logical
/// pixel, and a page is as much as the view shows (`page`).
pub fn map_wheel(
    track: &mut WheelTrack,
    unit: MouseWheelUnit,
    delta: Vec2,
    phase: TouchPhase,
    source: MouseWheelSource,
    page: Vec2,
) -> Scroll {
    let v = |x: f32, y: f32| kurbo::Vec2::new(f64::from(x), f64::from(y));
    let (angle_delta, pixel_delta) = match unit {
        MouseWheelUnit::Line => (v(delta.x, delta.y) * WheelEvent::NOTCH, kurbo::Vec2::ZERO),
        MouseWheelUnit::Point => (kurbo::Vec2::ZERO, v(delta.x, delta.y)),
        MouseWheelUnit::Page => (kurbo::Vec2::ZERO, v(delta.x * page.x, delta.y * page.y)),
    };
    let phase = match phase {
        // The platform keeps the content going after the fingers left;
        // Qt calls that its own phase. egui knows it only on macOS, and
        // elsewhere it looks like a wheel.
        TouchPhase::End | TouchPhase::Cancel if source == MouseWheelSource::Momentum => {
            ScrollPhase::End
        }
        _ if source == MouseWheelSource::Momentum => ScrollPhase::Momentum,
        TouchPhase::Start => {
            track.in_gesture = true;
            ScrollPhase::Begin
        }
        TouchPhase::Move if track.in_gesture => ScrollPhase::Update,
        TouchPhase::Move => ScrollPhase::None,
        TouchPhase::End | TouchPhase::Cancel => {
            track.in_gesture = false;
            ScrollPhase::End
        }
    };
    Scroll {
        angle_delta,
        pixel_delta,
        phase,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: Vec2 = Vec2::new(300.0, 200.0);

    fn wheel(track: &mut WheelTrack, unit: MouseWheelUnit, x: f32, y: f32) -> Scroll {
        map_wheel(
            track,
            unit,
            Vec2::new(x, y),
            TouchPhase::Move,
            MouseWheelSource::Unknown,
            PAGE,
        )
    }

    #[test]
    fn a_line_is_a_notch() {
        let mut t = WheelTrack::default();
        let s = wheel(&mut t, MouseWheelUnit::Line, 0.0, -1.0);
        assert_eq!(s.angle_delta, kurbo::Vec2::new(0.0, -120.0));
        assert_eq!(s.pixel_delta, kurbo::Vec2::ZERO);
        assert_eq!(s.phase, ScrollPhase::None);
        let s = wheel(&mut t, MouseWheelUnit::Line, 2.0, 0.5);
        assert_eq!(s.angle_delta, kurbo::Vec2::new(240.0, 60.0));
    }

    #[test]
    fn a_point_is_a_pixel_and_a_page_is_the_view() {
        let mut t = WheelTrack::default();
        let s = wheel(&mut t, MouseWheelUnit::Point, 3.0, -7.5);
        assert_eq!(s.angle_delta, kurbo::Vec2::ZERO);
        assert_eq!(s.pixel_delta, kurbo::Vec2::new(3.0, -7.5));
        let s = wheel(&mut t, MouseWheelUnit::Page, 1.0, -0.5);
        assert_eq!(s.pixel_delta, kurbo::Vec2::new(300.0, -100.0));
    }

    #[test]
    fn only_an_announced_gesture_has_phases() {
        let mut t = WheelTrack::default();
        let unit = MouseWheelUnit::Point;
        let mut phase = |p: TouchPhase| {
            map_wheel(
                &mut t,
                unit,
                Vec2::new(0.0, -4.0),
                p,
                MouseWheelSource::Unknown,
                PAGE,
            )
            .phase
        };
        assert_eq!(phase(TouchPhase::Move), ScrollPhase::None);
        assert_eq!(phase(TouchPhase::Start), ScrollPhase::Begin);
        assert_eq!(phase(TouchPhase::Move), ScrollPhase::Update);
        assert_eq!(phase(TouchPhase::Move), ScrollPhase::Update);
        assert_eq!(phase(TouchPhase::End), ScrollPhase::End);
        assert_eq!(phase(TouchPhase::Move), ScrollPhase::None);
        assert_eq!(phase(TouchPhase::Start), ScrollPhase::Begin);
        assert_eq!(phase(TouchPhase::Cancel), ScrollPhase::End);
        assert_eq!(phase(TouchPhase::Move), ScrollPhase::None);
    }

    /// macOS goes on scrolling after the fingers left, in a gesture of
    /// its own; Qt shows it as `Qt.ScrollMomentum` until it ends.
    #[test]
    fn momentum_after_the_fingers_left_is_its_own_phase() {
        let mut t = WheelTrack::default();
        let unit = MouseWheelUnit::Point;
        let mut phase = |p: TouchPhase, source| {
            map_wheel(&mut t, unit, Vec2::new(0.0, -4.0), p, source, PAGE).phase
        };
        let fingers = MouseWheelSource::Trackpad;
        let momentum = MouseWheelSource::Momentum;
        assert_eq!(phase(TouchPhase::Start, fingers), ScrollPhase::Begin);
        assert_eq!(phase(TouchPhase::Move, fingers), ScrollPhase::Update);
        assert_eq!(phase(TouchPhase::End, fingers), ScrollPhase::End);
        assert_eq!(phase(TouchPhase::Start, momentum), ScrollPhase::Momentum);
        assert_eq!(phase(TouchPhase::Move, momentum), ScrollPhase::Momentum);
        assert_eq!(phase(TouchPhase::End, momentum), ScrollPhase::End);
        assert_eq!(
            phase(TouchPhase::Move, MouseWheelSource::Wheel),
            ScrollPhase::None
        );
    }

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
