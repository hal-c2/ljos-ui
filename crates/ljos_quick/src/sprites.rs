//! `Sprite` and `SpriteSequence`: frames cut from an image, played one
//! sprite at a time, going from sprite to sprite as their `to` maps say.
//!
//! What a sequence does was read on qml6 (`features/animation/
//! sprite-sequence.feature`): it shows nothing and has no `currentSprite`
//! until its first frame, a turn after it is made, and then plays the
//! first sprite; a `jumpTo()` before that is lost, and one after it lands
//! on the next frame, running or not. A sprite whose frames are done goes
//! to one of its `to` sprites, chosen by weight, or plays again when it
//! has none; with a `goalSprite` it takes the shortest way there instead,
//! whatever the weights. A frame not given a size is the image's height
//! high and its width shared by the frames; frames go on to the next row
//! when the row runs out.
//!
//! The sequence draws through an ordinary `Image` child that it shows the
//! current frame's part of, so a frame takes the render tree's image path.
//! It keeps one engine deadline, the next frame, while it plays a sprite
//! that has more than one moment to show, and none otherwise.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, LogLevel, ObjectId, PropIndex, QmlList, QmlObject, QtObject,
    Rect, Url, Value, WriteKind, qml_impl,
};
use ljos_render::ImageHandle;
use ljos_runtime::{Time, TimerId};

use crate::image::{Image, ImageBase, ImageKey, ImageStore};
use crate::item::{Item, is_a, write_computed};

const CATEGORY: &str = "sprites";

/// How long "the next frame" is: when a sequence starts, and when a jump
/// lands.
const TURN: Duration = Duration::from_millis(16);

/// `Sprite` (QtQuick): which frames of an image, for how long, and where
/// to go after.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Sprite", extends = QtObject)]
pub struct Sprite {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub name: Atom,
    #[qml(notify)]
    pub source: Url,
    #[qml(notify)]
    pub reverse: bool,
    /// Accepted; a frame lasts as its duration says.
    #[qml(notify)]
    pub frame_sync: bool,
    #[qml(notify)]
    pub frame_count: i32,
    #[qml(notify)]
    pub frame_height: i32,
    #[qml(notify)]
    pub frame_width: i32,
    #[qml(notify)]
    pub frame_x: i32,
    #[qml(notify)]
    pub frame_y: i32,
    /// Frames a second; below 0 is not set, and `frameDuration` is used.
    #[qml(notify, reset)]
    pub frame_rate: f64,
    #[qml(notify)]
    pub frame_rate_variation: f64,
    /// Milliseconds a frame; below 0 is not set, and `duration` is used.
    #[qml(notify, reset)]
    pub frame_duration: i32,
    #[qml(notify)]
    pub frame_duration_variation: i32,
    /// Milliseconds for all the frames; below 0 is for ever.
    #[qml(notify)]
    pub duration: i32,
    #[qml(notify)]
    pub duration_variation: i32,
    /// Starts at a frame chosen by chance instead of the first.
    #[qml(notify)]
    pub random_start: bool,
    /// The sprites this one may go to, each with a weight.
    #[qml(notify)]
    pub to: Value,
}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            name: Atom::default(),
            source: Url::default(),
            reverse: false,
            frame_sync: false,
            frame_count: 1,
            frame_height: 0,
            frame_width: 0,
            frame_x: 0,
            frame_y: 0,
            frame_rate: -2.0,
            frame_rate_variation: 0.0,
            frame_duration: -2,
            frame_duration_variation: 0,
            duration: -1,
            duration_variation: 0,
            random_start: false,
            to: Value::Undefined,
        }
    }
}

#[qml_impl]
impl Sprite {}

/// `SpriteSequence` (QtQuick): plays its sprites, one at a time.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "SpriteSequence", extends = Item, default = "sprites")]
pub struct SpriteSequence {
    #[qml(base)]
    pub item: Item,
    #[qml(list, append = sprite_append)]
    pub sprites: QmlList,
    #[qml(notify)]
    pub running: bool,
    /// Accepted; one frame is shown at a time, not blended into the next.
    #[qml(notify)]
    pub interpolate: bool,
    #[qml(notify)]
    pub goal_sprite: Atom,
    #[qml(notify, readonly)]
    pub current_sprite: Atom,
    #[qml(skip)]
    run: Rc<RefCell<Run>>,
}

impl Default for SpriteSequence {
    fn default() -> Self {
        Self {
            item: Item::default(),
            sprites: QmlList::default(),
            running: true,
            interpolate: true,
            goal_sprite: Atom::default(),
            current_sprite: Atom::default(),
            run: Rc::default(),
        }
    }
}

/// Where a sequence is.
#[derive(Debug)]
struct Run {
    /// Whether the first frame was shown.
    started: bool,
    /// Which of `sprites` plays, and its frame.
    sprite: usize,
    frame: i32,
    /// When the frame shown now was due: the next is due from it, so a
    /// late wake does not push every frame after it.
    due: Time,
    /// The next frame's deadline, when there is one.
    next: Option<TimerId>,
    /// A `jumpTo()` waiting for the next frame.
    jump: Option<(TimerId, Atom)>,
    /// The picture the frames are shown in.
    picture: Option<ObjectId>,
    /// The sheets taken in when the sequence started, kept while it is.
    sheets: Vec<ImageHandle>,
    /// What the weights and variations are chosen with.
    rng: u64,
}

impl Default for Run {
    fn default() -> Self {
        Self {
            started: false,
            sprite: 0,
            frame: 0,
            due: Time::ZERO,
            next: None,
            jump: None,
            picture: None,
            sheets: Vec::new(),
            rng: 0x5eed_5eed_5eed_5eed,
        }
    }
}

impl Run {
    /// splitmix64, from 0 to 1.
    fn unit(&mut self) -> f64 {
        self.rng = self.rng.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[qml_impl]
impl SpriteSequence {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let picture = e.create_native_incomplete(Image::default(), Some(me))?;
        crate::tree::set_parent_item(e, picture, Some(me))?;
        write_computed(e, picture, Item::VISIBLE.index, Value::Bool(false))?;
        e.complete(picture)?;
        with_run(e, me, |r| r.picture = Some(picture));
        e.effect_in(me, move |e| {
            let w = e.read(me, Item::WIDTH.index);
            let h = e.read(me, Item::HEIGHT.index);
            warn(e, write_computed(e, picture, Item::WIDTH.index, w));
            warn(e, write_computed(e, picture, Item::HEIGHT.index, h));
        })?;
        e.watch(me, SpriteSequence::RUNNING.index, move |e, v| {
            if v.to_boolean() {
                schedule(e, me, false);
            } else {
                cancel_next(e, me);
            }
        })?;
        e.watch(me, SpriteSequence::GOAL_SPRITE.index, move |e, _| {
            // A sprite that lasts for ever has no end to look for the way
            // at, so the way starts on the next frame.
            if with_run(e, me, |r| r.started && r.next.is_none()) == Some(true) {
                schedule(e, me, true);
            }
        })?;
        e.call_at(e.now() + TURN, move |e| start(e, me));
        Ok(())
    }

    #[qml(invokable, name = "jumpTo")]
    fn jump_to(cx: &Cx, sprite: Atom) {
        let (e, me) = (cx.engine, cx.this);
        if with_run(e, me, |r| r.started) != Some(true) {
            return;
        }
        let id = e.call_at(e.now() + TURN, move |e| jumped(e, me));
        let old = with_run(e, me, |r| r.jump.replace((id, sprite))).flatten();
        if let Some((old, _)) = old {
            e.cancel_call(old);
        }
    }

    #[qml(invokable, name = "setGoalSprite")]
    fn set_goal_sprite(cx: &Cx, sprite: Atom) -> Result<(), EngineError> {
        cx.engine.write(
            cx.this,
            SpriteSequence::GOAL_SPRITE.index,
            Value::String(sprite),
            WriteKind::Imperative,
        )
    }
}

fn sprite_append(cx: &Cx, sprite: ObjectId) -> Result<(), EngineError> {
    if !is_a::<Sprite>(cx.engine, sprite) {
        return Err(EngineError::WrongType {
            ty: "SpriteSequence.sprites".into(),
            expected: "Sprite".into(),
        });
    }
    cx.engine
        .list_push(cx.this, SpriteSequence::SPRITES.index, sprite)
}

fn with_run<R>(e: &Engine, me: ObjectId, f: impl FnOnce(&mut Run) -> R) -> Option<R> {
    let run = e.with(me, |s: &SpriteSequence| Rc::clone(&s.run)).ok()?;
    let mut run = run.borrow_mut();
    Some(f(&mut run))
}

fn warn(e: &Engine, r: Result<(), EngineError>) {
    if let Err(err) = r {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!("SpriteSequence: {err}"),
        );
    }
}

fn sprites(e: &Engine, me: ObjectId) -> Vec<ObjectId> {
    match e.untrack(|| e.read(me, SpriteSequence::SPRITES.index)) {
        Value::List(l) => l.iter().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    }
}

fn read(e: &Engine, obj: ObjectId, p: PropIndex) -> Value {
    e.untrack(|| e.read(obj, p))
}

fn int(e: &Engine, obj: ObjectId, p: PropIndex) -> i32 {
    read(e, obj, p).to_number() as i32
}

fn name_of(e: &Engine, sprite: ObjectId) -> Atom {
    read(e, sprite, Sprite::NAME.index).to_js_string()
}

fn has_source(e: &Engine, sprite: ObjectId) -> bool {
    !read(e, sprite, Sprite::SOURCE.index)
        .to_js_string()
        .as_str()
        .is_empty()
}

fn find(e: &Engine, all: &[ObjectId], name: &str) -> Option<usize> {
    all.iter().position(|s| name_of(e, *s).as_str() == name)
}

/// The first frame: the first sprite, when it has a picture to show.
fn start(e: &Engine, me: ObjectId) {
    if !e.is_alive(me) {
        return;
    }
    let all = sprites(e, me);
    if !all.first().is_some_and(|s| has_source(e, *s)) {
        return;
    }
    let now = e.now();
    with_run(e, me, |r| {
        r.started = true;
        r.due = now;
    });
    enter(e, me, &all, 0);
    take_in(e, me, &all);
    if let Some(p) = with_run(e, me, |r| r.picture).flatten() {
        warn(
            e,
            write_computed(e, p, Item::VISIBLE.index, Value::Bool(true)),
        );
    }
    schedule(e, me, false);
}

/// As in Qt, a sequence takes in every sprite's sheet when it starts:
/// those not shown yet decode aside, so the frame a jump goes to one in
/// does not decode it.
fn take_in(e: &Engine, me: ObjectId, all: &[ObjectId]) {
    let store = ImageStore::of(e);
    let mut seen: Vec<String> = Vec::new();
    for &sprite in all {
        let raw = read(e, sprite, Sprite::SOURCE.index).to_js_string();
        if raw.is_empty() {
            continue;
        }
        let url = e.resolve_url(sprite, raw.as_str());
        if seen.contains(&url) {
            continue;
        }
        store.load(e, &url, move |e, got| {
            if let Ok(sheet) = got {
                with_run(e, me, |r| r.sheets.push(sheet));
            }
        });
        seen.push(url);
    }
}

fn jumped(e: &Engine, me: ObjectId) {
    if !e.is_alive(me) {
        return;
    }
    let Some(Some((_, name))) = with_run(e, me, |r| r.jump.take()) else {
        return;
    };
    let all = sprites(e, me);
    let Some(i) = find(e, &all, name.as_str()) else {
        return;
    };
    cancel_next(e, me);
    with_run(e, me, |r| r.due = e.now());
    enter(e, me, &all, i);
    schedule(e, me, false);
}

/// Plays sprite `i` from its first frame.
fn enter(e: &Engine, me: ObjectId, all: &[ObjectId], i: usize) {
    let Some(&sprite) = all.get(i) else {
        return;
    };
    let count = int(e, sprite, Sprite::FRAME_COUNT.index).max(1);
    let random = read(e, sprite, Sprite::RANDOM_START.index).to_boolean();
    with_run(e, me, |r| {
        r.sprite = i;
        r.frame = if random {
            ((r.unit() * f64::from(count)) as i32).min(count - 1)
        } else {
            0
        };
    });
    warn(
        e,
        write_computed(
            e,
            me,
            SpriteSequence::CURRENT_SPRITE.index,
            Value::String(name_of(e, sprite)),
        ),
    );
    show(e, me);
}

fn cancel_next(e: &Engine, me: ObjectId) {
    if let Some(Some(id)) = with_run(e, me, |r| r.next.take()) {
        e.cancel_call(id);
    }
}

/// Asks for the next frame of the sprite that plays, when it has one and
/// the sequence runs; `soon` asks for it on the next turn instead.
fn schedule(e: &Engine, me: ObjectId, soon: bool) {
    cancel_next(e, me);
    if !read(e, me, SpriteSequence::RUNNING.index).to_boolean()
        || with_run(e, me, |r| r.started) != Some(true)
    {
        return;
    }
    let all = sprites(e, me);
    let Some(sprite) = with_run(e, me, |r| r.sprite).and_then(|i| all.get(i).copied()) else {
        return;
    };
    let wait = if soon {
        Some(TURN)
    } else {
        let mut rng = || with_run(e, me, Run::unit).unwrap_or(0.5);
        frame_length(e, sprite, &mut rng)
    };
    let Some(wait) = wait else {
        return;
    };
    let now = e.now();
    let at = with_run(e, me, |r| {
        // A frame that is already late is counted from now.
        let at = r.due + wait;
        if at < now { now } else { at }
    })
    .unwrap_or(now);
    let id = e.call_at(at, move |e| tick(e, me, at));
    with_run(e, me, |r| r.next = Some(id));
}

/// How long a frame of `sprite` is shown, or `None` for ever.
fn frame_length(e: &Engine, sprite: ObjectId, unit: &mut dyn FnMut() -> f64) -> Option<Duration> {
    let count = f64::from(int(e, sprite, Sprite::FRAME_COUNT.index).max(1));
    let vary = |unit: &mut dyn FnMut() -> f64, base: f64, by: f64| base + by * (2.0 * unit() - 1.0);
    let rate = read(e, sprite, Sprite::FRAME_RATE.index).to_number();
    let ms = if rate > 0.0 {
        let by = read(e, sprite, Sprite::FRAME_RATE_VARIATION.index).to_number();
        1000.0 / vary(unit, rate, by).max(0.001)
    } else {
        let per = f64::from(int(e, sprite, Sprite::FRAME_DURATION.index));
        if per >= 0.0 {
            let by = f64::from(int(e, sprite, Sprite::FRAME_DURATION_VARIATION.index));
            vary(unit, per, by)
        } else {
            let all = f64::from(int(e, sprite, Sprite::DURATION.index));
            if all < 0.0 {
                return None;
            }
            let by = f64::from(int(e, sprite, Sprite::DURATION_VARIATION.index));
            vary(unit, all, by) / count
        }
    };
    // A frame takes at least one; a length of none would never let go.
    Some(Duration::from_secs_f64(ms.max(1.0) / 1000.0))
}

fn tick(e: &Engine, me: ObjectId, at: Time) {
    if !e.is_alive(me) {
        return;
    }
    with_run(e, me, |r| {
        r.next = None;
        r.due = at;
    });
    let all = sprites(e, me);
    let Some(i) = with_run(e, me, |r| r.sprite) else {
        return;
    };
    let Some(&sprite) = all.get(i) else {
        return;
    };
    let count = int(e, sprite, Sprite::FRAME_COUNT.index).max(1);
    let frame = with_run(e, me, |r| r.frame).unwrap_or(0) + 1;
    let goal = read(e, me, SpriteSequence::GOAL_SPRITE.index).to_js_string();
    if frame < count {
        with_run(e, me, |r| r.frame = frame);
        show(e, me);
    } else {
        match next_sprite(e, me, &all, i, goal.as_str()) {
            Some(n) if n != i => enter(e, me, &all, n),
            _ => {
                with_run(e, me, |r| r.frame = 0);
                show(e, me);
            }
        }
    }
    schedule(e, me, false);
}

/// The sprites `sprite`'s `to` names, with their weights, in its order.
fn targets(e: &Engine, all: &[ObjectId], sprite: ObjectId) -> Vec<(usize, f64)> {
    let to = read(e, sprite, Sprite::TO.index);
    e.js_object_entries(&to)
        .into_iter()
        .filter_map(|(name, w)| find(e, all, &name).map(|i| (i, w.to_number())))
        .collect()
}

/// Where sprite `i` goes when its frames are done: the first step on the
/// shortest way to the goal, else one of its targets by weight.
fn next_sprite(e: &Engine, me: ObjectId, all: &[ObjectId], i: usize, goal: &str) -> Option<usize> {
    if let Some(g) = find(e, all, goal).filter(|g| *g != i) {
        let mut first: Vec<Option<usize>> = vec![None; all.len()];
        let mut seen = vec![false; all.len()];
        seen[i] = true;
        let mut queue = VecDeque::from([i]);
        while let Some(at) = queue.pop_front() {
            for (n, _) in targets(e, all, all[at]) {
                if seen[n] {
                    continue;
                }
                seen[n] = true;
                first[n] = if at == i { Some(n) } else { first[at] };
                if n == g {
                    return first[n];
                }
                queue.push_back(n);
            }
        }
    }
    let weighted: Vec<(usize, f64)> = targets(e, all, all[i])
        .into_iter()
        .filter(|(_, w)| *w > 0.0)
        .collect();
    let total: f64 = weighted.iter().map(|(_, w)| w).sum();
    if total <= 0.0 {
        return None;
    }
    let mut pick = with_run(e, me, Run::unit).unwrap_or(0.0) * total;
    for (n, w) in &weighted {
        if pick < *w {
            return Some(*n);
        }
        pick -= w;
    }
    weighted.last().map(|(n, _)| *n)
}

/// Shows the frame the sequence is at.
fn show(e: &Engine, me: ObjectId) {
    let all = sprites(e, me);
    let Some((i, frame, Some(picture))) = with_run(e, me, |r| (r.sprite, r.frame, r.picture))
    else {
        return;
    };
    let Some(&sprite) = all.get(i) else {
        return;
    };
    let raw = read(e, sprite, Sprite::SOURCE.index).to_js_string();
    let url = e.resolve_url(sprite, raw.as_str());
    let key = ImageKey {
        url: url.clone(),
        request: Default::default(),
    };
    let sheet = ImageStore::of(e)
        .load_now(&key, true)
        .map(|img| (f64::from(img.width()), f64::from(img.height())))
        .ok();
    let count = int(e, sprite, Sprite::FRAME_COUNT.index).max(1);
    let frame = if read(e, sprite, Sprite::REVERSE.index).to_boolean() {
        count - 1 - frame
    } else {
        frame
    };
    let clip = sheet.map(|(sw, sh)| {
        let fw = match int(e, sprite, Sprite::FRAME_WIDTH.index) {
            w if w > 0 => f64::from(w),
            _ => sw / f64::from(count),
        };
        let fh = match int(e, sprite, Sprite::FRAME_HEIGHT.index) {
            h if h > 0 => f64::from(h),
            _ => sh,
        };
        let mut x = f64::from(int(e, sprite, Sprite::FRAME_X.index));
        let mut y = f64::from(int(e, sprite, Sprite::FRAME_Y.index));
        for _ in 0..frame {
            x += fw;
            if x + fw > sw {
                x = 0.0;
                y += fh;
            }
        }
        Rect {
            x,
            y,
            width: fw,
            height: fh,
        }
    });
    e.batch(|| {
        let w = |p: PropIndex, v: Value| warn(e, write_computed(e, picture, p, v));
        w(
            ImageBase::SOURCE.index,
            Value::Url(Atom::from(url.as_str())),
        );
        if let Some(clip) = clip {
            w(Image::SOURCE_CLIP_RECT.index, Value::Rect(clip.into()));
        }
    });
}

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [Sprite, SpriteSequence]));
}
