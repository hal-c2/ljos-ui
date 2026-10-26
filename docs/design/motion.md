# Motion

Motion in an interface has a job: confirming that input landed, showing where something came from or went, linking a before and an after. Motion with no job is a delay. The rules here check the curve, the duration, what is animated and how often.

## Motion for people who are not animators

### Easing: why ease-in feels late

Nothing in the world starts or stops instantly, so linear motion looks mechanical. The curve decides how it feels:

- **Ease-out** (fast start, gentle stop) for anything that **enters or answers the user**: a menu opening, a dialog appearing, a press. It moves at once, so the interface feels quick.
- **Ease-in-out** for something that **moves from one place on screen to another**.
- **Ease-in** (slow start) only for things **leaving**. On an entrance it spends its first frames barely moving, right when the user is watching for a response, so the interface feels late (`motion-ease-in-enter`).
- **Linear** only for constant motion (a spinner, a progress bar) or a value that tracks time.

QML names: `Easing.OutCubic` or `Easing.OutQuint` to enter, `Easing.InOutCubic` to move, `Easing.InCubic` to leave. For a named curve use `easing.type: Easing.BezierSpline` with `easing.bezierCurve: [x1, y1, x2, y2, 1, 1]`. Useful curves: a strong ease-out (0.23, 1, 0.32, 1), Material's emphasized decelerate (0.05, 0.7, 0.1, 1) and emphasized accelerate (0.3, 0, 0.8, 0.15), and a drawer curve (0.32, 0.72, 0, 1).

### Duration by frequency and size

How often a user sees it matters more than how big it is. Something done a hundred times a day must not make anyone wait a hundred times.

| Kind | Examples | Limit |
|---|---|---|
| Frequent | selection, tab switch, focus, a shortcut | instant, or up to `limits::motion::FREQUENT_MAX_MS` |
| Micro | press, check, toggle, tooltip | `limits::motion::MICRO_MAX_MS` |
| Standard | menus, dropdowns, panels, dialogs, drawers, page changes | `limits::motion::STANDARD_MAX_MS` |
| Emphasis | rare, large moves; a first-run moment | `limits::motion::EMPHASIS_MAX_MS` |

Larger distances take a little longer than small ones within a kind. Nothing visible lasts less than `limits::motion::MIN_VISIBLE_MS`, which reads as a flicker; use 0 instead.

### Enter and exit

Exits are shorter than entrances, about 60 to 70 % of them (`motion-exit-slower`): the user has already decided to move on. Entrances ease out, exits ease in.

```qml
Popup {
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 200; easing.type: Easing.OutCubic }
        NumberAnimation { property: "scale"; from: 0.95; to: 1; duration: 200; easing.type: Easing.OutCubic }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: 130; easing.type: Easing.InCubic }
    }
}
```

### Origin at the trigger

A menu or popover grows from the control that opened it, so the eye follows cause to effect: set `transformOrigin` to the corner nearest the trigger (`Item.TopLeft` for a menu below a button on its left). Modal dialogs are the exception and stay centred. Start at scale `limits::motion::ENTER_SCALE_RECOMMENDED` with a fade, never at 0.

### Springs and interruptions

A curve has a fixed duration; a spring has a stiffness, damping and mass, and settles when it settles. Springs suit things that follow the user (a dragged sheet let go, a value that keeps changing) because they keep their velocity when the target changes mid-flight. Curves suit enter and exit. Keep springs well damped — a damping ratio of at least `limits::motion::SPRING_MIN_DAMPING_RATIO`, a slight settle and no wobble — and do not spring colour or opacity.

Any motion must be interruptible: if the user acts again, the animation turns toward the new target from where it is. In QML a `Behavior` or a `SpringAnimation` retargets; a fixed `SequentialAnimation` does not.

```qml
Rectangle {
    x: target
    Behavior on x { SpringAnimation { spring: 4; damping: 0.4; epsilon: 0.25 } }
}
```

### Stagger

Items of a list may enter one after another, by at most `limits::motion::STAGGER_STEP_MAX_MS` a step and `limits::motion::STAGGER_TOTAL_MAX_MS` in all (past that, the last items just wait). Stagger the first few and show the rest at once.

### Reduced motion is a cross-fade, not nothing

When the system asks for reduced motion (`Qt.styleHints.accessibility.motionPreference === Qt.MotionPreference.ReducedMotion`), drop movement, scaling, parallax and springs, and keep a short opacity cross-fade so a change is still visible. Opacity and colour changes are not motion for WCAG 2.3.3.

On Ljós the runtime does this for every animation, whether the component asks or not:

- What moves (a place, a size, a scale, a turn, a scroll, how far a Drawer is in) is at its end at once.
- A fade or a change of colour keeps its curve and lasts at most `limits::motion::REDUCED_FADE_MAX_MS`; a shorter one keeps its own length.
- A popup, menu, dialog or drawer, and a StackView page, whose way in or out would move it and does not fade it, fades instead: in where it ends, out where it is. A fade the transition has is kept, and capped. `opened`, `closed` and the page signals come in the same order, when the fade ends.
- An endless animation keeps running at its own pace: a BusyIndicator still turns, as the platforms keep their spinners. Gate one that is decoration (`motion-reduced-ignored`).
- A change of the setting while something runs goes on from where it is: nothing jumps back.

Qt only tells the application and leaves the rest to it. A component that also targets Qt reads the setting itself, which is also right on Ljós:

```qml
readonly property bool reduced: Qt.styleHints.accessibility.motionPreference === Qt.MotionPreference.ReducedMotion
enter: Transition {
    NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 150; easing.type: Easing.OutCubic }
    NumberAnimation { property: "y"; from: reduced ? 0 : 12; to: 0; duration: reduced ? 0 : 200; easing.type: Easing.OutCubic }
}
```

Give each entrance and exit its own opacity fade so that the reduced path on Qt still shows the change, keep the fade within `limits::motion::REDUCED_FADE_MAX_MS`, and stop or hide endless decoration when `reduced` is true.

### Performance: animate what is cheap

`opacity`, `scale`, `rotation` and `x`/`y` of an item that does not drive a layout only move pixels. `width`, `height`, `implicitHeight` and anchor margins lay the interface out again every frame (`motion-layout-property`) and drop frames on small machines. Every frame has `limits::motion::FRAME_BUDGET_MS`.

## Rules

<a id="motion-ease-in-enter"></a>
### `motion-ease-in-enter`

**Error, static and runtime.** Something appears or responds with an ease-in curve, so it starts slow and the interface feels late.

```qml
// Bad
enter: Transition { NumberAnimation { property: "opacity"; to: 1; duration: 200; easing.type: Easing.InQuad } }
// Good
enter: Transition { NumberAnimation { property: "opacity"; to: 1; duration: 200; easing.type: Easing.OutCubic } }
```

Sources: https://animations.dev/learn/animation-theory/the-easing-blueprint , https://m3.material.io/styles/motion/easing-and-duration/applying-easing-and-duration

<a id="motion-default-easing"></a>
### `motion-default-easing`

**Warning, static.** An animation moves, scales or fades with no `easing.type`, which in QML is linear: a mechanical start and stop.

```qml
// Bad
NumberAnimation { property: "y"; duration: 200 }
// Good
NumberAnimation { property: "y"; duration: 200; easing.type: Easing.InOutCubic }
```

Sources: https://animations.dev/learn/animation-theory/the-easing-blueprint , https://web.dev/articles/animations-guide

<a id="motion-duration"></a>
### `motion-duration`

**Warning, static and runtime.** An interface animation is longer than its kind allows (`limits::motion::STANDARD_MAX_MS` for a panel, `limits::motion::EMPHASIS_MAX_MS` at most), or shorter than `limits::motion::MIN_VISIBLE_MS`. Past about 400 ms (the Doherty threshold) people wait for the interface instead of the other way round.

```qml
// Bad
Drawer { enter: Transition { NumberAnimation { property: "position"; duration: 800 } } }
// Good
Drawer { enter: Transition { NumberAnimation { property: "position"; duration: 300; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.32, 0.72, 0, 1, 1, 1] } } }
```

Sources: https://www.nngroup.com/articles/animation-duration/ , https://m3.material.io/styles/motion/easing-and-duration/tokens-specs , https://lawsofux.com/doherty-threshold/

<a id="motion-scale-from-zero"></a>
### `motion-scale-from-zero`

**Error, static and runtime.** Something grows from a scale below `limits::motion::ENTER_SCALE_MIN`, often 0, so it seems to come from nowhere and its content is unreadable for most of the animation.

```qml
// Bad
NumberAnimation { property: "scale"; from: 0; to: 1; duration: 200 }
// Good
ParallelAnimation {
    NumberAnimation { property: "scale"; from: 0.95; to: 1; duration: 200; easing.type: Easing.OutCubic }
    NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 200; easing.type: Easing.OutCubic }
}
```

Sources: https://github.com/emilkowalski/skills , https://interfaces.rauno.me/

<a id="motion-bounce"></a>
### `motion-bounce`

**Warning, static.** A control or panel moves with a bounce, elastic or back curve, or an underdamped spring (damping ratio under `limits::motion::SPRING_MIN_DAMPING_RATIO`). It is fun once and tiring the fiftieth time, and it delays the end of the motion.

```qml
// Bad
NumberAnimation { property: "y"; duration: 400; easing.type: Easing.OutBounce }
// Good
NumberAnimation { property: "y"; duration: 250; easing.type: Easing.OutCubic }
```

Sources: https://www.joshwcomeau.com/animation/a-friendly-introduction-to-spring-physics/ , https://developer.apple.com/design/human-interface-guidelines/motion

<a id="motion-layout-property"></a>
### `motion-layout-property`

**Warning, static.** An animation changes a size or an anchor margin, which lays the interface out again every frame.

```qml
// Bad
Behavior on height { NumberAnimation { duration: 200 } }
// Good: a fixed height, a moving reveal
Item { clip: true; height: content.height
    Item { id: content; y: open ? 0 : -height; Behavior on y { NumberAnimation { duration: 200; easing.type: Easing.OutCubic } } } }
```

Sources: https://web.dev/articles/animations-guide , https://web.dev/articles/stick-to-compositor-only-properties-and-manage-layer-count

<a id="motion-exit-slower"></a>
### `motion-exit-slower`

**Warning, static.** Something leaves as slowly as it came, or more slowly. The user has moved on; the leftover blocks the next thing.

```qml
// Bad
enter: Transition { NumberAnimation { property: "opacity"; to: 1; duration: 200 } }
exit:  Transition { NumberAnimation { property: "opacity"; to: 0; duration: 300 } }
// Good
enter: Transition { NumberAnimation { property: "opacity"; to: 1; duration: 200; easing.type: Easing.OutCubic } }
exit:  Transition { NumberAnimation { property: "opacity"; to: 0; duration: 130; easing.type: Easing.InCubic } }
```

Sources: https://www.nngroup.com/articles/animation-duration/ , https://m3.material.io/styles/motion/transitions/transition-patterns

<a id="motion-stagger"></a>
### `motion-stagger`

**Warning, static and runtime.** Items of a group start one after another with a step over `limits::motion::STAGGER_STEP_MAX_MS`, or the last starts more than `limits::motion::STAGGER_TOTAL_MAX_MS` after the first.

```qml
// Bad
delegate: ItemDelegate {
    required property int index
    opacity: 0
    SequentialAnimation on opacity { PauseAnimation { duration: index * 120 } NumberAnimation { to: 1; duration: 150 } }
}
// Good: a short step, and only the first few wait
delegate: ItemDelegate {
    required property int index
    opacity: 0
    SequentialAnimation on opacity {
        PauseAnimation { duration: Math.min(index, 4) * 40 }
        NumberAnimation { to: 1; duration: 150; easing.type: Easing.OutCubic }
    }
}
```

Sources: https://github.com/emilkowalski/skills , https://github.com/nextlevelbuilder/ui-ux-pro-max-skill

<a id="motion-frequent"></a>
### `motion-frequent`

**Warning, static and runtime.** Something done many times a day (selection, a tab switch, focus, a shortcut) animates longer than `limits::motion::FREQUENT_MAX_MS`. Fine in a demo, a tax in daily use.

```qml
// Bad
ListView { highlightMoveDuration: 300 }
// Good
ListView { highlightMoveDuration: 0 }
```

Sources: https://github.com/emilkowalski/skills , https://interfaces.rauno.me/ , https://developer.apple.com/design/human-interface-guidelines/motion

<a id="motion-timer-driven"></a>
### `motion-timer-driven`

**Warning, static.** A repeating `Timer` moves or fades something by hand. It bypasses reduced motion, the animation clock and idle: it keeps the CPU awake and stutters under load.

```qml
// Bad
Timer { interval: 16; repeat: true; running: true; onTriggered: dot.opacity = (dot.opacity + 0.05) % 1 }
// Good
SequentialAnimation on opacity { loops: Animation.Infinite; running: busy
    NumberAnimation { to: 0.3; duration: 600; easing.type: Easing.InOutSine }
    NumberAnimation { to: 1; duration: 600; easing.type: Easing.InOutSine } }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html , https://web.dev/articles/animations-guide

<a id="motion-reduced-ignored"></a>
### `motion-reduced-ignored`

**Hint, static.** An endless animation (`loops: Animation.Infinite`) moves something (`x`, `y`, `scale`, `rotation`, a size) and nothing in it reads `Qt.styleHints.accessibility.motionPreference`. Ljós keeps an endless run going under reduced motion, as a busy indicator must; on Qt the application alone decides, so endless decoration keeps moving for someone who asked for less motion (WCAG 2.3.3). Gate it, or animate opacity instead.

```qml
// Bad: a badge that bobs for ever
SequentialAnimation on y { loops: Animation.Infinite
    NumberAnimation { to: 4; duration: 600; easing.type: Easing.InOutSine }
    NumberAnimation { to: 0; duration: 600; easing.type: Easing.InOutSine } }
// Good: still under reduced motion, or a pulse of opacity
SequentialAnimation on y { loops: Animation.Infinite
    running: Qt.styleHints.accessibility.motionPreference !== Qt.MotionPreference.ReducedMotion
    NumberAnimation { to: 4; duration: 600; easing.type: Easing.InOutSine }
    NumberAnimation { to: 0; duration: 600; easing.type: Easing.InOutSine } }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html , https://doc.qt.io/qt-6/qaccessibilityhints.html#motionPreference-prop , https://web.dev/articles/prefers-reduced-motion

<a id="motion-dropped-frames"></a>
### `motion-dropped-frames`

**Warning, runtime.** An animation took longer than `limits::motion::FRAME_BUDGET_MS` for some frames while it ran. Stutter reads as slowness, whatever the duration says. Profile with `app_frames`.

```qml
// Bad: a blur re-rendered every frame of a slide
NumberAnimation { target: blurredPanel; property: "width"; duration: 250 }
// Good: slide the finished panel
NumberAnimation { target: blurredPanel; property: "x"; duration: 250; easing.type: Easing.OutCubic }
```

Sources: https://web.dev/articles/rendering-performance , https://web.dev/articles/animations-guide

<a id="motion-flash"></a>
### `motion-flash`

**Error, static.** Something flashes more than three times a second (WCAG 2.2 2.3.1): a `Timer` with `repeat: true` whose `interval` is under half of `limits::motion::FLASH_MIN_PASS_MS` and whose `onTriggered` toggles `visible`, `opacity` or `color` (each tick is half of a flash), or a looping animation of `opacity`, `visible` or `color` whose pass is shorter than `limits::motion::FLASH_MIN_PASS_MS`. Only literal numbers are read, so a computed interval is never reported. Flashing can trigger seizures; no taste or brand reason outweighs that.

```qml
// Bad: four flashes a second
Timer { interval: 100; repeat: true; running: true; onTriggered: alert.visible = !alert.visible }
// Good: a slow pulse, and one that stops
SequentialAnimation on opacity { loops: Animation.Infinite; running: alert.pending
    NumberAnimation { to: 0.4; duration: 800; easing.type: Easing.InOutSine }
    NumberAnimation { to: 1; duration: 800; easing.type: Easing.InOutSine } }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/three-flashes-or-below-threshold.html , https://www.w3.org/WAI/WCAG22/Understanding/pause-stop-hide.html

<a id="motion-endless"></a>
### `motion-endless`

**Warning, static.** An animation with `loops: Animation.Infinite` or a `Timer` with `repeat: true` changes something visible (opacity, colour, visibility) from the moment it loads, and nothing can stop it: no `running:` binding, no `stop()` or `running = false` in the file. Motion that starts by itself and lasts longer than `limits::motion::ENDLESS_MAX_MS` needs a way to pause, stop or hide it (WCAG 2.2 2.2.2). A busy indicator or a progress bar is exempt, its motion is the message; moving animations are `motion-reduced-ignored`'s, not this rule's. Bind `running` to the state it shows.

```qml
// Bad: a pulse that never stops
SequentialAnimation on opacity { loops: Animation.Infinite
    NumberAnimation { to: 0.3; duration: 900 } NumberAnimation { to: 1; duration: 900 } }
// Good: it runs only while there is something to say
SequentialAnimation on opacity { loops: Animation.Infinite; running: syncing
    NumberAnimation { to: 0.3; duration: 900 } NumberAnimation { to: 1; duration: 900 } }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/pause-stop-hide.html , https://web.dev/articles/prefers-reduced-motion

<a id="motion-origin"></a>
### `motion-origin`

**Hint, static.** A `Popup`, `Menu` or `ToolTip` has an `enter` transition that animates `scale` and keeps the default `transformOrigin`, the centre. A popup that grows from its middle seems to come from nowhere; it should grow from the control that opened it. Set `transformOrigin` toward that control, or fade instead of scaling. A `Drawer` slides from its edge and is not reported.

```qml
// Bad: a menu that grows from its middle
Menu { enter: Transition { NumberAnimation { property: "scale"; from: 0.9; to: 1; duration: 150; easing.type: Easing.OutCubic } } }
// Good: from the corner the button is at
Menu { transformOrigin: Popup.TopLeft
    enter: Transition { NumberAnimation { property: "scale"; from: 0.9; to: 1; duration: 150; easing.type: Easing.OutCubic } } }
```

Sources: https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#transformOrigin-prop , https://animations.dev/learn/animation-theory/the-easing-blueprint

## Not checked yet

- **No motion on a theme switch** (proposed `motion-theme-switch`): colours that animate while the theme changes make every surface pulse.

## Sources

- animations.dev, the easing blueprint: https://animations.dev/learn/animation-theory/the-easing-blueprint ; Emil Kowalski's skills (MIT): https://github.com/emilkowalski/skills
- Material 3 motion, easing and duration: https://m3.material.io/styles/motion/easing-and-duration/tokens-specs ; tokens: https://raw.githubusercontent.com/material-components/material-web/main/tokens/versions/v0_192/_md-sys-motion.scss
- Apple HIG motion: https://developer.apple.com/design/human-interface-guidelines/motion
- NN/g, animation duration: https://www.nngroup.com/articles/animation-duration/ ; Laws of UX, Doherty threshold: https://lawsofux.com/doherty-threshold/
- Josh W. Comeau, spring physics: https://www.joshwcomeau.com/animation/a-friendly-introduction-to-spring-physics/
- WCAG 2.2, animation from interactions, pause stop hide, three flashes: https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html , https://www.w3.org/WAI/WCAG22/Understanding/pause-stop-hide.html , https://www.w3.org/WAI/WCAG22/Understanding/three-flashes-or-below-threshold.html
- web.dev, animations and rendering performance: https://web.dev/articles/animations-guide , https://web.dev/articles/rendering-performance
- Rauno Freiberg, web interface guidelines: https://interfaces.rauno.me/
- Val Head, *Designing Interface Animation*: https://www.rosenfeldmedia.com/books/designing-interface-animation/ ; Issara Willenskomer, the UX in Motion manifesto: https://uxmag.com/articles/creating-usability-with-motion-the-ux-in-motion-manifesto
