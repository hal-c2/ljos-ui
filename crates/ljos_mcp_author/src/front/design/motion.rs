//! Motion: curves, durations, what is animated and how (`docs/design/motion.md`).

use ljos_design::limits::motion as limits;
use oxc_ast_visit::{Visit, walk};
use qml_syntax::{
    ast::{QmlBindingValue, QmlObject},
    js::{
        AssignmentExpression, AssignmentTarget, BinaryOperator, Expression, SimpleAssignmentTarget,
        UpdateExpression,
    },
};

use super::{Found, Frame, bound, children, class, number};

/// What moves, scales or fades on screen.
const MOVES: &[&str] = &["x", "y", "scale", "opacity", "width", "height", "rotation"];

/// What lays the interface out again when it changes.
const LAYOUT: &[&str] = &[
    "width",
    "height",
    "implicitWidth",
    "implicitHeight",
    "anchors.margins",
    "anchors.leftMargin",
    "anchors.rightMargin",
    "anchors.topMargin",
    "anchors.bottomMargin",
    "anchors.horizontalCenterOffset",
    "anchors.verticalCenterOffset",
    "Layout.preferredWidth",
    "Layout.preferredHeight",
    "Layout.minimumWidth",
    "Layout.minimumHeight",
    "Layout.maximumWidth",
    "Layout.maximumHeight",
    "Layout.margins",
    "Layout.leftMargin",
    "Layout.rightMargin",
    "Layout.topMargin",
    "Layout.bottomMargin",
];

/// Names a binding reads when it follows small feedback on a control.
const MICRO_STATES: &[&str] = &[
    "hovered",
    "pressed",
    "checked",
    "down",
    "containsMouse",
    "containsPress",
];

/// Names a binding reads when it follows something done many times a day.
const FREQUENT_STATES: &[&str] = &[
    "currentIndex",
    "isCurrentItem",
    "activeFocus",
    "visualFocus",
    "selected",
    "highlighted",
    "current",
];

/// The duration Qt gives an animation that does not set one.
const DEFAULT_DURATION_MS: f64 = 250.0;

/// Where an animation runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Context {
    /// A popup's `enter`, or a view's `add` or `populate`.
    Enter,
    /// A popup's `exit`, or a view's `remove`.
    Exit,
    /// A transition into a state that shows something.
    Shows,
    /// A `Behavior`, with the index in the chain of the item it is on.
    Behavior(usize),
    /// Anything else, where the source cannot tell whether it enters.
    Unknown,
}

struct Animation<'f> {
    /// The index in the chain of the first frame that is not an animation.
    owner: Option<usize>,
    context: Context,
    /// Repeats, or is part of a group that does.
    looped: bool,
    /// Inside a busy indicator or a progress bar, whose motion is the point.
    progress: bool,
    /// What it animates, if the source says.
    properties: Option<Vec<String>>,
    _marker: std::marker::PhantomData<&'f ()>,
}

pub(super) fn object<'a>(found: &mut Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) {
    let frame = chain.last().expect("the object itself");
    if found.is(frame, class::ANIMATION) && !found.is(frame, class::PAUSE) {
        let animation = describe(found, chain);
        let eased = !found.is(frame, class::SELF_EASED);
        if eased {
            ease_in_enter(found, frame, &animation);
            bounce(found, frame);
            default_easing(found, frame, &animation);
        }
        scale_from_zero(found, frame, &animation);
        layout_property(found, frame, &animation);
        duration(found, chain, &animation);
    }
    if found.is(frame, class::POPUP) {
        exit_slower(found, frame);
    }
    if found.is(frame, class::VIEW) {
        for name in ["highlightMoveDuration", "highlightResizeDuration"] {
            if let Some(b) = bound(frame.object, &[name])
                && let Some(ms) = b.number()
                && ms > f64::from(limits::FREQUENT_MAX_MS)
            {
                found.report(
                    b.at,
                    "motion-frequent",
                    format!(
                        "`{name}: {ms}` animates the current item for longer than the \
                         {} ms something done many times a day may take; use at most that, \
                         or 0",
                        limits::FREQUENT_MAX_MS
                    ),
                );
            }
        }
    }
    if found.is(frame, class::TIMER) {
        timer_driven(found, frame);
    }
    stagger(found, chain);
}

fn describe<'f, 'a>(found: &Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) -> Animation<'f> {
    let frame = chain.last().expect("the object itself");
    let mut looped = false;
    let mut owner = None;
    for (i, f) in chain.iter().enumerate().rev() {
        if found.is(f, class::ANIMATION) {
            looped |= bound(f.object, &["loops"]).is_some();
            continue;
        }
        owner = Some(i);
        break;
    }
    let progress = chain.iter().any(|f| found.is(f, class::PROGRESS));
    // An animation in a group animates what the group was put `on`.
    let group = || {
        chain[owner.map_or(0, |o| o + 1)..]
            .iter()
            .rev()
            .find_map(|f| f.object.on.as_ref().map(|on| vec![on.parts.join(".")]))
    };
    let mut context = Context::Unknown;
    let mut properties = animated(found, frame).or_else(group);
    if let Some(o) = owner {
        let f = &chain[o];
        if found.is(f, class::BEHAVIOR) {
            if properties.is_none() {
                properties = f.object.on.as_ref().map(|on| vec![on.parts.join(".")]);
            }
            if o > 0 {
                context = Context::Behavior(o - 1);
            }
        } else if found.is(f, class::TRANSITION) && o > 0 {
            let parent = &chain[o - 1];
            context = match f.via.as_deref() {
                Some("enter") if found.is(parent, class::POPUP) => Context::Enter,
                Some("exit") if found.is(parent, class::POPUP) => Context::Exit,
                Some("add" | "populate") if found.is(parent, class::VIEW) => Context::Enter,
                Some("remove") if found.is(parent, class::VIEW) => Context::Exit,
                _ if shows_state(f.object, parent.object) => Context::Shows,
                _ => Context::Unknown,
            };
        }
    }
    Animation {
        owner,
        context,
        looped,
        progress,
        properties,
        _marker: std::marker::PhantomData,
    }
}

/// What an animation animates, from its `property`, `properties`, `on` or
/// the kind of animator it is.
fn animated(found: &Found<'_, '_, '_, '_>, frame: &Frame<'_, '_>) -> Option<Vec<String>> {
    if let Some(on) = &frame.object.on {
        return Some(vec![on.parts.join(".")]);
    }
    let mut out = Vec::new();
    for name in ["property", "properties"] {
        if let Some(b) = bound(frame.object, &[name]) {
            // A property named by a binding the source cannot read.
            out.extend(
                b.string()?
                    .split(',')
                    .map(|p| p.trim().to_owned())
                    .filter(|p| !p.is_empty()),
            );
        }
    }
    if !out.is_empty() {
        return Some(out);
    }
    let animator = [
        ("XAnimator", "x"),
        ("YAnimator", "y"),
        ("ScaleAnimator", "scale"),
        ("OpacityAnimator", "opacity"),
        ("RotationAnimator", "rotation"),
    ];
    let ty = found.base(frame)?;
    animator.iter().find_map(|(name, property)| {
        ty.0.is_a(&[&format!("QQuick{name}"), &format!("QtQuick.{name}")])
            .then(|| vec![(*property).to_owned()])
    })
}

/// A transition to one state, written out, that makes something visible or
/// opaque.
fn shows_state(transition: &QmlObject<'_>, owner: &QmlObject<'_>) -> bool {
    let Some(to) = bound(transition, &["to"]).and_then(|b| b.string()) else {
        return false;
    };
    if to.is_empty() || to.contains(['*', ',']) {
        return false;
    }
    let Some(states) = bound(owner, &["states"]) else {
        return false;
    };
    let list: Vec<&QmlObject<'_>> = match states.value {
        QmlBindingValue::Object(o) => vec![o],
        QmlBindingValue::Objects(os) => os.iter().collect(),
        _ => return false,
    };
    list.into_iter()
        .filter(|state| bound(state, &["name"]).and_then(|b| b.string()) == Some(to))
        .flat_map(|state| children(state).into_iter().map(|(c, _)| c))
        .any(|change| {
            // `PropertyChanges { target: panel; visible: true }` or `{ panel.visible: true }`.
            super::bindings(change).iter().any(|(name, b)| {
                let last = name.rsplit('.').next().unwrap_or(name);
                last == "visible" && b.boolean() == Some(true)
                    || last == "opacity" && b.number() == Some(1.0)
            })
        })
}

fn easing<'o, 'a>(object: &'o QmlObject<'a>) -> Option<(u32, Option<&'o str>)> {
    let b = bound(object, &["easing", "type"])?;
    Some((
        b.at,
        b.enumeration()
            .filter(|(e, _)| *e == "Easing")
            .map(|(_, n)| n),
    ))
}

fn is_ease_in(name: &str) -> bool {
    name.starts_with("In") && !name.starts_with("InOut")
}

fn ease_in_enter<'a>(
    found: &mut Found<'_, '_, '_, 'a>,
    frame: &Frame<'_, 'a>,
    animation: &Animation<'_>,
) {
    let Some((at, Some(name))) = easing(frame.object) else {
        return;
    };
    if !is_ease_in(name) {
        return;
    }
    let what = match animation.context {
        Context::Enter => "something entering",
        Context::Shows => "a transition that shows something",
        Context::Behavior(_) => "a `Behavior`, which answers a change the user made",
        Context::Exit | Context::Unknown => return,
    };
    found.report(
        at,
        "motion-ease-in-enter",
        format!(
            "`Easing.{name}` starts slow, and this is {what}, so the interface feels late; \
             use an ease-out curve (`Easing.OutCubic`) and keep ease-in for exits"
        ),
    );
}

fn bounce<'a>(found: &mut Found<'_, '_, '_, 'a>, frame: &Frame<'_, 'a>) {
    let Some((at, Some(name))) = easing(frame.object) else {
        return;
    };
    if ["Bounce", "Elastic", "Back"]
        .iter()
        .any(|k| name.ends_with(k))
    {
        found.report(
            at,
            "motion-bounce",
            format!(
                "`Easing.{name}` overshoots or bounces, which a control or a panel should not; \
                 use `Easing.OutCubic`, and keep bounce for a rare moment worth celebrating"
            ),
        );
    }
}

fn default_easing<'a>(
    found: &mut Found<'_, '_, '_, 'a>,
    frame: &Frame<'_, 'a>,
    animation: &Animation<'_>,
) {
    if animation.looped || animation.progress || found.is(frame, class::COLOR_ANIMATION) {
        return;
    }
    if !(found.is(frame, class::PROPERTY_ANIMATION) || found.is(frame, class::ANIMATOR)) {
        return;
    }
    if bound(frame.object, &["easing"]).is_some() || easing(frame.object).is_some() {
        return;
    }
    if bound(frame.object, &["duration"]).and_then(|b| b.number()) == Some(0.0) {
        return;
    }
    let Some(properties) = &animation.properties else {
        return;
    };
    let Some(moved) = properties.iter().find(|p| MOVES.contains(&p.as_str())) else {
        return;
    };
    found.report(
        frame.object.type_name.span.start,
        "motion-default-easing",
        format!(
            "`{}` animates `{moved}` with no `easing`, which is linear: a mechanical start and \
             stop; set `easing.type` (`Easing.OutCubic` to enter, `Easing.InOutCubic` to move)",
            found.name(frame)
        ),
    );
}

fn scale_from_zero<'a>(
    found: &mut Found<'_, '_, '_, 'a>,
    frame: &Frame<'_, 'a>,
    animation: &Animation<'_>,
) {
    if animation.context == Context::Exit || animation.looped {
        return;
    }
    if !animation
        .properties
        .as_ref()
        .is_some_and(|p| p.iter().any(|p| p == "scale"))
    {
        return;
    }
    let Some(from) = bound(frame.object, &["from"]) else {
        return;
    };
    let Some(start) = from.number() else { return };
    let end = bound(frame.object, &["to"])
        .and_then(|b| b.number())
        .unwrap_or(1.0);
    if start < f64::from(limits::ENTER_SCALE_MIN) && end > start {
        found.report(
            from.at,
            "motion-scale-from-zero",
            format!(
                "scaling from {start} makes it come from nowhere; start at {} or more (the \
                 guidelines say {}) together with an opacity fade",
                limits::ENTER_SCALE_MIN,
                limits::ENTER_SCALE_RECOMMENDED
            ),
        );
    }
}

fn layout_property<'a>(
    found: &mut Found<'_, '_, '_, 'a>,
    frame: &Frame<'_, 'a>,
    animation: &Animation<'_>,
) {
    let Some(properties) = &animation.properties else {
        return;
    };
    let Some(laid) = properties.iter().find(|p| LAYOUT.contains(&p.as_str())) else {
        return;
    };
    found.report(
        frame.object.type_name.span.start,
        "motion-layout-property",
        format!(
            "`{}` animates `{laid}`, which lays the interface out again every frame; animate \
             `scale`, `opacity` or a transform and set the size once",
            found.name(frame)
        ),
    );
}

/// Whether the binding of `property` on `object` reads one of `names`.
fn reads(
    found: &Found<'_, '_, '_, '_>,
    object: &QmlObject<'_>,
    property: &str,
    names: &[&str],
) -> bool {
    let path: Vec<&str> = property.split('.').collect();
    let Some(span) = bound(object, &path).and_then(|b| b.span()) else {
        return false;
    };
    found
        .text(span)
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|word| names.contains(&word))
}

fn duration<'a>(
    found: &mut Found<'_, '_, '_, 'a>,
    chain: &[Frame<'_, 'a>],
    animation: &Animation<'_>,
) {
    let frame = chain.last().expect("the object itself");
    if animation.looped || animation.progress {
        return;
    }
    let written = bound(frame.object, &["duration"]);
    let ms = written.and_then(|b| b.number());
    let at = written.map_or(frame.object.type_name.span.start, |b| b.at);
    let behavior_on = |owner: usize| -> Option<(usize, String)> {
        let o = &chain[owner + 1];
        o.object.on.as_ref().map(|on| (owner, on.parts.join(".")))
    };
    if let Context::Behavior(item) = animation.context
        && let Some((_, property)) = behavior_on(item)
        && reads(found, chain[item].object, &property, FREQUENT_STATES)
    {
        // Qt's default duration counts here: the default is too long.
        let ms = match ms {
            Some(ms) => ms,
            None if written.is_none() && found.is(frame, class::PROPERTY_ANIMATION) => {
                DEFAULT_DURATION_MS
            }
            None => return,
        };
        if ms > f64::from(limits::FREQUENT_MAX_MS) {
            found.report(
                at,
                "motion-frequent",
                format!(
                    "this `Behavior on {property}` follows the current or focused item, which \
                     changes many times a day, and takes {ms} ms; make it instant or at most {} ms",
                    limits::FREQUENT_MAX_MS
                ),
            );
        }
        return;
    }
    let Some(ms) = ms else { return };
    if ms > 0.0 && ms < f64::from(limits::MIN_VISIBLE_MS) {
        found.report(
            at,
            "motion-duration",
            format!(
                "{ms} ms is too short to be seen and reads as a flicker; use 0 or at least {} ms",
                limits::MIN_VISIBLE_MS
            ),
        );
        return;
    }
    let in_drawer = chain.iter().any(|f| found.is(f, class::DRAWER));
    let (kind, max) = match animation.context {
        Context::Enter | Context::Exit if in_drawer || popup_owner(found, chain, animation) => {
            ("a popup's entrance or exit", limits::STANDARD_MAX_MS)
        }
        Context::Behavior(item)
            if behavior_on(item)
                .is_some_and(|(_, p)| reads(found, chain[item].object, &p, MICRO_STATES)) =>
        {
            ("feedback on hover, press or check", limits::MICRO_MAX_MS)
        }
        _ if in_drawer => ("a drawer", limits::STANDARD_MAX_MS),
        _ => ("interface motion", limits::EMPHASIS_MAX_MS),
    };
    if ms > f64::from(max) {
        found.report(
            at,
            "motion-duration",
            format!(
                "{ms} ms is longer than {kind} may take ({max} ms); use the duration for its \
                 kind (the motion tokens)"
            ),
        );
    }
}

fn popup_owner(
    found: &Found<'_, '_, '_, '_>,
    chain: &[Frame<'_, '_>],
    animation: &Animation<'_>,
) -> bool {
    animation
        .owner
        .and_then(|o| o.checked_sub(1))
        .is_some_and(|p| found.is(&chain[p], class::POPUP))
}

/// How long a transition or an animation takes, if the source says.
fn length(
    found: &Found<'_, '_, '_, '_>,
    object: &QmlObject<'_>,
    index: Option<usize>,
) -> Option<f64> {
    let frame = Frame {
        object,
        index,
        via: None,
    };
    let parts = || -> Option<Vec<f64>> {
        children(object)
            .into_iter()
            .map(|(c, _)| length(found, c, found.cx.tree.index_of(c)))
            .collect()
    };
    let sequential = ["QQuickSequentialAnimation", "QtQuick.SequentialAnimation"];
    if found.is(&frame, &sequential) {
        return Some(parts()?.into_iter().sum());
    }
    if found.is(&frame, class::TRANSITION)
        || !found.is(&frame, class::PROPERTY_ANIMATION)
            && !found.is(&frame, class::ANIMATOR)
            && !found.is(&frame, class::PAUSE)
    {
        if !found.is(&frame, class::TRANSITION) && !found.is(&frame, class::ANIMATION) {
            return Some(0.0);
        }
        return Some(parts()?.into_iter().fold(0.0, f64::max));
    }
    match bound(object, &["duration"]) {
        Some(b) => b.number(),
        None => Some(DEFAULT_DURATION_MS),
    }
}

fn exit_slower<'a>(found: &mut Found<'_, '_, '_, 'a>, frame: &Frame<'_, 'a>) {
    let transition = |name: &str| {
        let b = bound(frame.object, &[name])?;
        match b.value {
            QmlBindingValue::Object(o) => Some((b.at, o)),
            _ => None,
        }
    };
    let (Some((_, enter)), Some((at, exit))) = (transition("enter"), transition("exit")) else {
        return;
    };
    let index = |o| found.cx.tree.index_of(o);
    let (Some(entering), Some(leaving)) = (
        length(found, enter, index(enter)),
        length(found, exit, index(exit)),
    ) else {
        return;
    };
    if entering > 0.0 && leaving >= entering {
        found.report(
            at,
            "motion-exit-slower",
            format!(
                "it leaves in {leaving} ms and enters in {entering} ms; an exit should be shorter \
                 than the entrance (about two thirds of it)"
            ),
        );
    }
}

/// `index * N` in a delay: each item waits N ms longer than the one before.
fn stagger<'a>(found: &mut Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) {
    let frame = chain.last().expect("the object itself");
    for (name, b) in super::bindings(frame.object) {
        let delay = name == "duration" && found.is(frame, class::PAUSE)
            || name == "interval"
            || name.ends_with("delay")
            || name.ends_with("Delay");
        if !delay {
            continue;
        }
        let Some(Expression::BinaryExpression(e)) = b.expression().map(|e| e.without_parentheses())
        else {
            continue;
        };
        if e.operator != BinaryOperator::Multiplication {
            continue;
        }
        let is_index = |e: &Expression<'_>| match e.without_parentheses() {
            Expression::Identifier(id) => id.name == "index",
            Expression::StaticMemberExpression(m) => m.property.name == "index",
            _ => false,
        };
        let step = match (is_index(&e.left), is_index(&e.right)) {
            (true, false) => number(&e.right),
            (false, true) => number(&e.left),
            _ => None,
        };
        let Some(step) = step else { continue };
        // How many items there are, where a model is a number written out.
        let count = chain
            .iter()
            .rev()
            .find(|f| found.is(f, class::REPEATER) || found.is(f, class::VIEW))
            .and_then(|f| bound(f.object, &["model"]))
            .and_then(|b| b.number());
        if step > f64::from(limits::STAGGER_STEP_MAX_MS) {
            found.report(
                b.at,
                "motion-stagger",
                format!(
                    "each item waits {step} ms more than the one before; keep a step of a stagger \
                     within {} ms",
                    limits::STAGGER_STEP_MAX_MS
                ),
            );
        } else if let Some(count) = count
            && (count - 1.0) * step > f64::from(limits::STAGGER_TOTAL_MAX_MS)
        {
            found.report(
                b.at,
                "motion-stagger",
                format!(
                    "the last of {count} items waits {} ms; keep the whole stagger within {} ms",
                    (count - 1.0) * step,
                    limits::STAGGER_TOTAL_MAX_MS
                ),
            );
        }
    }
}

/// What a script assigns to.
#[derive(Default)]
struct Assigned<'a> {
    names: Vec<&'a str>,
}

impl<'a> Assigned<'a> {
    fn target(&mut self, target: &SimpleAssignmentTarget<'a>) {
        match target {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                self.names.push(id.name.as_str())
            }
            SimpleAssignmentTarget::StaticMemberExpression(m) => {
                self.names.push(m.property.name.as_str())
            }
            _ => {}
        }
    }
}

impl<'a> Visit<'a> for Assigned<'a> {
    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if let Some(simple) = it.left.as_simple_assignment_target() {
            self.target(simple);
        } else if let AssignmentTarget::AssignmentTargetIdentifier(id) = &it.left {
            self.names.push(id.name.as_str());
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        self.target(&it.argument);
        walk::walk_update_expression(self, it);
    }
}

fn timer_driven<'a>(found: &mut Found<'_, '_, '_, 'a>, frame: &Frame<'_, 'a>) {
    if bound(frame.object, &["repeat"]).and_then(|b| b.boolean()) != Some(true) {
        return;
    }
    let Some(handler) = bound(frame.object, &["onTriggered"]) else {
        return;
    };
    let mut assigned = Assigned::default();
    match handler.value {
        QmlBindingValue::Expression(e) => assigned.visit_expression(e),
        QmlBindingValue::Statement(s) => assigned.visit_statement(s),
        _ => {}
    }
    if let Some(moved) = assigned.names.iter().find(|n| MOVES.contains(n)) {
        found.report(
            frame.object.type_name.span.start,
            "motion-timer-driven",
            format!(
                "this repeating `Timer` sets `{moved}` by hand, past reduced motion and the \
                 animation clock; use an animation or a `Behavior on {moved}`"
            ),
        );
    }
}
