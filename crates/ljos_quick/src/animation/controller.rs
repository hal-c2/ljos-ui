//! `AnimationController`: an animation moved by hand.
//!
//! The controller never starts its animation. It keeps the animation's
//! timeline and shows the moment `progress` names, so the animation's
//! `running` stays false and none of its signals or scripts happen.

use std::cell::Cell;

use ljos_engine::{Cx, Engine, EngineError, ObjectId, QmlObject, QtObject, Value, qml_impl};
use ljos_reactive::WriteKind;

use super::timeline::{Ctx, Timed};

/// `AnimationController` (QtQuick).
///
/// TODO(controller): `completeToBeginning()` and `completeToEnd()`, which
/// let the animation run on by itself from where it was put, are missing.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "AnimationController",
    extends = QtObject,
    default = "animation"
)]
pub struct AnimationController {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub animation: Option<ObjectId>,
    /// 0 is the animation's start, 1 its end.
    #[qml(notify)]
    pub progress: f64,
    /// The animation as it was when last read (`reload()` reads again).
    #[qml(skip)]
    timeline: Option<Timed>,
}

#[qml_impl]
impl AnimationController {
    /// Reads the animation's properties again.
    #[qml(invokable)]
    fn reload(cx: &Cx) -> Result<(), EngineError> {
        rebuild(cx.engine, cx.this);
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        cx.engine
            .watch(me, AnimationController::ANIMATION.index, move |e, _| {
                rebuild(e, me);
            })?;
        cx.engine
            .watch(me, AnimationController::PROGRESS.index, move |e, v| {
                let p = v.to_number();
                let kept = if p.is_nan() { 0.0 } else { p.clamp(0.0, 1.0) };
                if kept == p {
                    seek(e, me);
                } else {
                    // Comes back here with the value in range.
                    let _ = e.write_internal(
                        me,
                        AnimationController::PROGRESS.index,
                        Value::Real(kept),
                        WriteKind::Host,
                    );
                }
            })?;
        rebuild(cx.engine, me);
        Ok(())
    }
}

fn rebuild(engine: &Engine, me: ObjectId) {
    let anim = engine
        .untrack(|| engine.read(me, AnimationController::ANIMATION.index))
        .as_object()
        .filter(|a| engine.is_alive(*a));
    let timeline = anim.map(|a| engine.untrack(|| Timed::build(engine, a, None)));
    let _ = engine.with_mut::<AnimationController, _>(me, |c| c.timeline = timeline);
    seek(engine, me);
}

/// Shows the moment `progress` names.
fn seek(engine: &Engine, me: ObjectId) {
    let progress = engine
        .untrack(|| engine.read(me, AnimationController::PROGRESS.index))
        .to_number();
    // Taken out: the values it writes may bring this controller back here.
    let Ok(Some(mut timeline)) =
        engine.with_mut::<AnimationController, _>(me, |c| c.timeline.take())
    else {
        return;
    };
    let total = if timeline.is_endless() {
        timeline.end_of_pass(0)
    } else {
        timeline.total()
    };
    // Qt places the animation by whole milliseconds.
    let ms = (progress.clamp(0.0, 1.0) * (total as f64 / 1000.0)).floor() as i64;
    let cancelled = Cell::new(false);
    let cx = Ctx {
        engine,
        cancelled: &cancelled,
        scripts: false,
    };
    engine.batch(|| timeline.seek(ms.saturating_mul(1000), &cx));
    let _ = engine.with_mut::<AnimationController, _>(me, |c| {
        // Unless it was reloaded meanwhile.
        if c.timeline.is_none() {
            c.timeline = Some(timeline);
        }
    });
}
