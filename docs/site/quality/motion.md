---
title: Motion and reduced motion
description: Choose easing curves, durations, and springs by how often an action occurs, and support reduced motion across Ljós and Qt.
section: quality
order: 5
---

# Motion and reduced motion

This page explains how to choose easing curves, durations, and springs in QML, how Ljós handles reduced motion automatically while keeping your code portable to Qt, and what the `motion-*` rules check.

## Easing curves and durations

Motion in an interface exists to confirm that input landed, show where an element came from or went, or connect a state before and after a change. Motion without a clear purpose is only a delay.

### Choosing the right easing curve

Linear motion starts and stops abruptly, so every spatial or opacity transition should set `easing.type`:

- **Ease-out (`Easing.OutCubic` or `Easing.OutQuint`):** Fast start and gentle stop. Use ease-out for anything that enters the screen or responds to user input (opening a menu or dialog, pressing a button) so the interface reacts on the very first frame.
- **Ease-in-out (`Easing.InOutCubic`):** Use for an item moving from one position on screen to another.
- **Ease-in (`Easing.InCubic`):** Slow start. Use ease-in **only** for elements leaving the screen. Using ease-in on an entrance makes the interface feel sluggish because the first frames barely move (`motion-ease-in-enter`).
- **Linear:** Reserve linear easing for constant loops such as spinners or time-tracking progress bars.
- **Custom cubic Bézier (`Easing.BezierSpline`):** Set `easing.bezierCurve: [x1, y1, x2, y2, 1, 1]` for named curves such as a strong ease-out (`[0.23, 1, 0.32, 1]`), Material emphasized decelerate (`[0.05, 0.7, 0.1, 1]`), Material emphasized accelerate (`[0.3, 0, 0.8, 0.15]`), or a drawer curve (`[0.32, 0.72, 0, 1]`).

### Duration limits by frequency

How often a user sees an animation matters more than how large it is:

| Kind | Examples | Maximum duration |
|---|---|---|
| **Frequent** | List selection, tab switch, keyboard focus, shortcut | Instant (`0 ms`), or at most `limits::motion::FREQUENT_MAX_MS` (`100 ms`) |
| **Micro** | Button press, checkbox, switch toggle, tooltip | `limits::motion::MICRO_MAX_MS` (`200 ms`) |
| **Standard** | Menus, dropdowns, panels, dialogs, drawers, page transitions | `limits::motion::STANDARD_MAX_MS` (`350 ms`; `Motion.enterPanel` is `250 ms`) |
| **Emphasis** | Rare, large transitions or first-run moments | `limits::motion::EMPHASIS_MAX_MS` (`500 ms`) |

Never use a visible duration below `limits::motion::MIN_VISIBLE_MS` (`50 ms`), which reads as a flicker; use `0` instead.

### Entrances, exits, origins, and springs

- **Exits are shorter than entrances (`motion-exit-slower`):** Make exit transitions about 60 to 70 % of the entrance duration, because the user has already decided to move on.
- **Scale from `0.95`, never `0` (`motion-scale-from-zero`):** Start popup entrances at scale `0.95` (`limits::motion::ENTER_SCALE_RECOMMENDED`) combined with an opacity fade so the content is legible throughout the transition.
- **Grow from the trigger (`motion-origin`):** Set `transformOrigin` on a `Menu`, `Popup`, or `ToolTip` toward the button that opened it (for example, `Popup.TopLeft`). Modal dialogs stay centred.
- **Use interruptible animations:** `Behavior` and `SpringAnimation` retarget smoothly when the user acts mid-animation. Keep springs well damped (damping ratio at least `limits::motion::SPRING_MIN_DAMPING_RATIO`, no bounce on controls or panels), and do not use springs for colour or opacity.

```qml
import QtQuick
import QtQuick.Controls

ApplicationWindow {
    width: 360
    height: 240
    visible: true
    title: qsTr("Menu transition")

    Button {
        id: menuButton
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.margins: 16
        text: qsTr("Options")
        onClicked: optionsMenu.open()

        Menu {
            id: optionsMenu
            y: menuButton.height
            transformOrigin: Popup.TopLeft

            enter: Transition {
                NumberAnimation {
                    property: "opacity"
                    from: 0
                    to: 1
                    duration: 200
                    easing.type: Easing.OutCubic
                }
                NumberAnimation {
                    property: "scale"
                    from: 0.95
                    to: 1
                    duration: 200
                    easing.type: Easing.OutCubic
                }
            }

            exit: Transition {
                NumberAnimation {
                    property: "opacity"
                    to: 0
                    duration: 130
                    easing.type: Easing.InCubic
                }
            }

            MenuItem {
                text: qsTr("Duplicate")
            }
            MenuItem {
                text: qsTr("Export…")
            }
        }
    }
}
```

## Reduced motion

When the user requests reduced motion (`Qt.styleHints.accessibility.motionPreference === Qt.MotionPreference.ReducedMotion`), replace spatial movement, scaling, parallax, and springs with a short opacity cross-fade so state changes remain clear without triggering vestibular discomfort (WCAG 2.2 SC 2.3.3).

### What Ljós does automatically

On Ljós, the runtime enforces reduced motion across every animation even if a component does not check the setting itself:

1. **Spatial properties snap at once:** Animations of position (`x`, `y`), size, scale, rotation, scroll position, or drawer position jump immediately to their target value.
2. **Fades are preserved and capped:** Opacity and colour animations keep their easing curve and are capped at `limits::motion::REDUCED_FADE_MAX_MS` (`150 ms`). Shorter fades keep their own duration.
3. **Moving popups and pages cross-fade instead:** If a `Popup`, `Menu`, `Dialog`, `Drawer`, or `StackView` page transition would move the item without fading it, Ljós substitutes an opacity fade at its final position and fires `opened`, `closed`, and page lifecycle signals in the normal order when the fade finishes.
4. **Essential endless spinners keep running:** A `BusyIndicator` or indeterminate `ProgressBar` continues to turn. Endless decorative animations should still be gated by the application (`motion-reduced-ignored`).
5. **Mid-flight changes never jump backwards:** Toggling reduced motion while an animation is running continues smoothly from its current state.

### Writing portable reduced-motion QML for Ljós and Qt

Qt exposes `Qt.styleHints.accessibility.motionPreference` but leaves the transition logic to the application. To write a component that respects reduced motion on both Ljós and Qt, include an explicit opacity fade and gate spatial motion and decorative loops on `motionPreference`:

```qml
import QtQuick
import QtQuick.Controls

ApplicationWindow {
    width: 360
    height: 240
    visible: true
    title: qsTr("Portable reduced motion")

    readonly property bool reduced: Qt.styleHints.accessibility.motionPreference === Qt.MotionPreference.ReducedMotion

    Popup {
        anchors.centerIn: parent
        width: Math.min(implicitWidth, 280)
        visible: true
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

        enter: Transition {
            NumberAnimation {
                property: "opacity"
                from: 0
                to: 1
                duration: 150
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                property: "y"
                from: reduced ? 0 : 12
                to: 0
                duration: reduced ? 0 : 200
                easing.type: Easing.OutCubic
            }
        }

        contentItem: Label {
            text: qsTr("Fades in on both Ljós and Qt when reduced motion is enabled.")
            wrapMode: Text.Wrap
        }
    }
}
```

## Motion rules summary

| Rule code | Severity | What it checks and how to fix it |
|---|---|---|
| `motion-ease-in-enter` | Error (static, runtime) | An entrance or response uses ease-in (or moves less than `limits::motion::EASE_IN_HALFWAY_MAX` halfway through). Use `Easing.OutCubic` instead. |
| `motion-scale-from-zero` | Error (static, runtime) | An entrance scales from below `limits::motion::ENTER_SCALE_MIN` (such as `0`). Start from `0.95` with an opacity fade. |
| `motion-flash` | Error (static) | A `Timer` or looping animation flashes `visible`, `opacity`, or `color` more than three times per second (`limits::motion::FLASH_MIN_PASS_MS`). Use a slow opacity pulse bound to `running`. |
| `motion-default-easing` | Warning (static) | An animation omits `easing.type`, defaulting to mechanical linear motion. Always set `easing.type`. |
| `motion-duration` | Warning (static, runtime) | An animation exceeds the limit for its kind (`200 ms` micro, `350 ms` standard, `500 ms` emphasis) or is shorter than `50 ms`. |
| `motion-bounce` | Warning (static) | A control or panel uses `OutBounce`, `OutElastic`, `OutBack`, or an underdamped spring. Use a smooth ease-out or well-damped spring. |
| `motion-layout-property` | Warning (static) | An animation changes `width`, `height`, `implicitHeight`, or anchor margins, forcing layout recalculation every frame. Animate `x`, `y`, `scale`, or `opacity` inside a clipped container instead. |
| `motion-exit-slower` | Warning (static) | An `exit` transition is as long as or longer than its `enter` transition. Make exits 60–70 % of the entrance duration. |
| `motion-stagger` | Warning (static, runtime) | List delegates stagger by more than `limits::motion::STAGGER_STEP_MAX_MS` (`50 ms`) per item or `limits::motion::STAGGER_TOTAL_MAX_MS` (`200 ms`) in total. Cap the stagger index (`Math.min(index, 4) * 40`). |
| `motion-frequent` | Warning (static, runtime) | A frequent action (focus, tab switch, `highlightMoveDuration`) animates for more than `100 ms`. Make it instant (`0 ms`) or under `100 ms`. |
| `motion-timer-driven` | Warning (static) | A repeating `Timer` steps visual properties manually. Use a declarative `NumberAnimation` or `SequentialAnimation` instead so it respects the animation clock, idle sleep, and reduced motion. |
| `motion-endless` | Warning (static) | An infinite animation (`loops: Animation.Infinite`) or repeating `Timer` changes visible state from load with no `running:` binding or `stop()` call. Bind `running` to the state it represents. |
| `motion-reduced-ignored` | Hint (static, runtime) | An endless spatial animation does not check `Qt.styleHints.accessibility.motionPreference`. Bind `running` to `motionPreference !== Qt.MotionPreference.ReducedMotion` or pulse `opacity` instead. |
| `motion-dropped-frames` | Hint (runtime) | Frames exceed `limits::motion::FRAME_BUDGET_MS` (`16.7 ms`) during an animation. Avoid animating expensive effects or layout properties, and inspect frame timings with `app_frames`. |
| `motion-origin` | Hint (static) | A `Popup`, `Menu`, or `ToolTip` scales in from the default centre `transformOrigin`. Set `transformOrigin` toward the trigger button. |
| `motion-theme-switch` | Hint (runtime) | Switching between light and dark schemes animates colours for longer than `limits::motion::THEME_SWITCH_MAX_MS` (`100 ms`) or flashes through an intermediate lightness. Switch theme colours immediately or in under `100 ms`. |

### Example: revealing a panel without animating layout properties

Animating `height` triggers `motion-layout-property` and relayouts the window on every frame. Instead, clip the container and animate `y` on the inner item:

```qml
import QtQuick
import QtQuick.Controls

ApplicationWindow {
    width: 360
    height: 220
    visible: true
    title: qsTr("Drawer reveal")

    property bool panelOpen: true

    Item {
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: contentPane.height
        clip: true

        Pane {
            id: contentPane
            width: parent.width
            y: panelOpen ? 0 : -height

            Behavior on y {
                NumberAnimation {
                    duration: 200
                    easing.type: Easing.OutCubic
                }
            }

            Label {
                width: parent.width
                text: qsTr("Slides smoothly without recalculating layout every frame.")
                wrapMode: Text.Wrap
            }
        }
    }
}
```

## See also

- [Design guardrails](design-guardrails.md)
- [Accessibility](accessibility.md)
- [Colour and contrast](color.md)
- [Performance and benchmarks](performance.md)
- [First app](../start/first-app.md)
