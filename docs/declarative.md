# Declarative QML

QML says what an interface is: a property is bound to what decides it, a mode is a state, and a change of state has a transition. The engine compiles a binding and updates it when what it reads changes; a state and a transition are objects the tools can read, so the motion and focus rules check them. A handler that does the same by hand is script the engine runs as it is and the tools cannot see through.

The `decl-*` lints of `qml_lint` hint where script says what a declaration would. They are hints: `ljos check` does not report them, and `// ljos-allow(<code>): <why>` keeps one where the script is meant. Each rule holds back wherever one file does not show the whole story, so a file with no hint may still have script a declaration says better.

Handlers are for what happens on an event. `onClicked: open = !open` changes state, and is never flagged; neither is a handler that computes, calls out, or writes a value it reads from somewhere other than the change it handles.

Sources: [Property binding](https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html), [States](https://doc.qt.io/qt-6/qtquick-statesanimations-states.html), [Animation and transitions](https://doc.qt.io/qt-6/qtquick-statesanimations-animations.html), [Behavior](https://doc.qt.io/qt-6/qml-qtquick-behavior.html).

## Rules

<a id="decl-mirror-handler"></a>
### `decl-mirror-handler`: a handler that only copies a value

A change handler whose whole body writes one property from the value that changed:

```qml
Item {
    property real level: 0
    property real shown: 0
    onLevelChanged: shown = level * 100
}
```

says what a binding does, without the binding's first value: `shown` is 0 until `level` first changes. Write the binding and drop the handler:

```qml
Item {
    property real level: 0
    property real shown: level * 100
}
```

Flagged: `on<Foo>Changed` whose body is one `=`, to a property of this object or of an object with an id in the same component, of an expression that reads `foo` (or `id.foo` of this object) and nothing but values written out, operators, enumeration keys, `Math` and methods such as `toFixed` that change nothing.

Not flagged:

- a property any other handler or function writes too (it is state, set from more than one place);
- a property states or a `Binding` set;
- a property the user sets on a control (`text`, `checked`, `value`, `currentIndex` and the like), where the handler is how it is kept in step: a binding there is undone by the first click;
- a property already given a value that is not written out;
- a handler that does anything else, or a handler in a `Connections`;
- a write to a singleton or another component's object, which one file cannot follow.

<a id="decl-mode-by-handlers"></a>
### `decl-mode-by-handlers`: handlers that switch a group of properties

Two or more handlers that each set the same properties to values of their own:

```qml
Button { text: qsTr("List"); onClicked: { list.opacity = 1; editor.opacity = 0 } }
Button { text: qsTr("Edit"); onClicked: { list.opacity = 0; editor.opacity = 1 } }
```

are modes switched by hand. Nothing says which modes there are, a third handler can set half of them, and nothing can animate the change. Make each mode a state:

```qml
Item {
    id: root
    states: [
        State { name: "list"; PropertyChanges { list.opacity: 1; editor.opacity: 0 } },
        State { name: "edit"; PropertyChanges { list.opacity: 0; editor.opacity: 1 } }
    ]
    transitions: Transition { NumberAnimation { property: "opacity"; duration: 150 } }
}
Button { text: qsTr("Edit"); onClicked: root.state = "edit" }
```

or give each state a `when:` and the handlers set what decides it.

Flagged: two handlers or functions that set, at their top level, at least two of the same properties to values written out (a number, a string, `true`, `null`, an enumeration key, the `qsTr` of a string), and differ in at least two of them. Handlers that pair up are reported together, once, at the first.

Not flagged: a handler that sets one property; values that are computed (`open = !open`, `x = mouse.x`); what `Component.onCompleted` or `onDestruction` sets, which is a first value, not a mode.

<a id="decl-imperative-animation"></a>
### `decl-imperative-animation`: animations started from script

An animation that starts the next one when it finishes:

```qml
NumberAnimation { id: grow; target: button; property: "scale"; to: 1.1
    onFinished: shrink.start() }
NumberAnimation { id: shrink; target: button; property: "scale"; to: 1 }
```

is a sequence written by hand: stopping it means stopping each, and nothing knows it is one. Put them in one `SequentialAnimation`:

```qml
SequentialAnimation {
    id: pulse
    NumberAnimation { target: button; property: "scale"; to: 1.1; duration: 120 }
    NumberAnimation { target: button; property: "scale"; to: 1; duration: 120 }
}
```

Opposite animations of one property, each started from a different handler (`show.start()` here, `hide.start()` there), are a change of state animated by hand. Make the states and give them a `Transition`, or put a `Behavior on opacity { NumberAnimation { } }` on the property and set it.

Flagged: `start()`, `restart()` or `running = true` of an animation with an id, from the `onFinished` or `onStopped` of another; and two or more animations of the same property of the same object that script starts.

Not flagged: a one-off animation a handler starts (a shake on a wrong password); timers, which `motion-timer-driven` is about.

<a id="decl-visibility-juggling"></a>
### `decl-visibility-juggling`: an item shown and hidden from several handlers

```qml
Rectangle { id: banner; visible: false }
Button { text: qsTr("Show"); onClicked: banner.visible = true }
Button { text: qsTr("Hide"); onClicked: banner.visible = false }
```

Whether `banner` shows is decided in as many places as there are handlers, and it appears and goes at once. Bind it to what decides it:

```qml
Rectangle { id: banner; visible: drawer.opened }
```

or make the shown and hidden states and give them a transition, which the motion rules check:

```qml
states: State { name: "shown"; PropertyChanges { banner.visible: true } }
```

Flagged: an item's `visible` or `opacity` set to values written out from two or more handlers, to at least two different values.

Not flagged: a toggle (`banner.visible = !banner.visible`); a `Popup` or a `Window`, whose `open()`/`close()` and `visible` are how they are shown; a property states or a `Binding` set; a property `decl-mode-by-handlers` already names; what `Component.onCompleted` sets.
