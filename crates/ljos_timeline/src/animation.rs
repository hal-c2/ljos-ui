//! `TimelineAnimation`: a `NumberAnimation` of its timeline's `currentFrame`.

use ljos::item::{
    Animation, NumberAnimation, PropertyAnimation, WriteKind, start_animation, stop_animation,
};
use ljos::prelude::*;

use crate::timeline::Timeline;

/// `TimelineAnimation`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Timeline", name = "TimelineAnimation", extends = NumberAnimation)]
pub struct TimelineAnimation {
    #[qml(base)]
    pub base: NumberAnimation,
    /// There and back again is one loop.
    #[qml(notify)]
    pub ping_pong: bool,
    /// On the way back, `from` and `to` read each other's.
    #[qml(skip)]
    reversed: bool,
    /// The ways there and back done.
    #[qml(skip)]
    round: i32,
}

impl Default for TimelineAnimation {
    fn default() -> Self {
        let mut base = NumberAnimation::default();
        base.base.property = Atom::from("currentFrame");
        let way = &mut base.base.animation.way;
        way.every_stop_finishes = true;
        way.default_on = Some(the_frame);
        Self {
            base,
            ping_pong: false,
            reversed: false,
            round: 0,
        }
    }
}

/// What it animates unless told otherwise: the frame of the timeline it is
/// in.
fn the_frame(e: &Engine, me: ObjectId) -> Option<(ObjectId, PropIndex)> {
    let timeline = e.owner(me).filter(|t| e.is::<Timeline>(*t))?;
    Some((timeline, Timeline::CURRENT_FRAME.index))
}

fn turn(e: &Engine, me: ObjectId, reversed: bool) {
    let was = e
        .with_mut(me, |a: &mut TimelineAnimation| {
            std::mem::replace(&mut a.reversed, reversed)
        })
        .unwrap_or(reversed);
    if was == reversed {
        return;
    }
    let (from, to) = (PropertyAnimation::FROM.index, PropertyAnimation::TO.index);
    let (a, b) = e.untrack(|| (e.read(me, from), e.read(me, to)));
    e.batch(|| {
        // As an animation writes: a binding on either stays.
        let _ = e.write_internal(me, from, b, WriteKind::Animation);
        let _ = e.write_internal(me, to, a, WriteKind::Animation);
    });
}

/// Each way is a run of its own, so `stopped` and `started` come between
/// them and `finished` only after the last.
fn stopped(e: &Engine, me: ObjectId) {
    let ping_pong = e.untrack(|| e.read(me, TimelineAnimation::PING_PONG.index).to_boolean());
    if !ping_pong {
        return;
    }
    let by_itself = e
        .with(me, |a: &Animation| a.way.ended_by_itself)
        .unwrap_or(false);
    let (reversed, round) = e
        .with_mut(me, |a: &mut TimelineAnimation| {
            if a.reversed {
                a.round += 1;
            }
            (a.reversed, a.round)
        })
        .unwrap_or((false, 0));
    let loops = e.untrack(|| e.read(me, Animation::LOOPS.index).to_number()) as i32;
    if by_itself && (loops < 0 || round < loops) {
        let _ = e.with_mut(me, |a: &mut Animation| a.way.hold_finished = true);
        turn(e, me, !reversed);
        start_animation(e, me);
        return;
    }
    turn(e, me, false);
    let _ = e.with_mut(me, |a: &mut TimelineAnimation| a.round = 0);
}

#[qml_impl]
impl TimelineAnimation {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        // Starting it stops the timeline's others.
        e.connect_by_name(me, "started", move |e, _| {
            let Some((timeline, _)) = the_frame(e, me) else {
                return;
            };
            let list = e.untrack(|| e.read(timeline, Timeline::ANIMATIONS.index));
            if let Value::List(list) = list {
                for other in list.iter().filter_map(Value::as_object) {
                    if other != me {
                        stop_animation(e, other);
                    }
                }
            }
        })?;
        e.connect_by_name(me, "stopped", move |e, _| stopped(e, me))?;
        // One pass a way: the ways are counted above.
        e.effect_in(me, move |e| {
            let ping_pong = e.read(me, TimelineAnimation::PING_PONG.index).to_boolean();
            let _ = e.with_mut(me, |a: &mut Animation| a.way.single_pass = ping_pong);
        })?;
        if let Some((timeline, _)) = the_frame(e, me)
            && e.untrack(|| e.read(me, PropertyAnimation::TARGET.index))
                .as_object()
                .is_none()
        {
            e.write_internal(
                me,
                PropertyAnimation::TARGET.index,
                Value::Object(timeline),
                WriteKind::Animation,
            )?;
        }
        Ok(())
    }
}
