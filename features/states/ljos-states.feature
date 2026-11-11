# Sources:
#   docs/DIALECT.md §4.9 (states in Ljós's API)
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
#   https://doc.qt.io/qt-6/qml-qtquick-state.html
#   https://www.typescriptlang.org/docs/handbook/2/everyday-types.html#literal-types
# Verified on Qt 6.11.2 (qml6, offscreen).
# Qt's QML says what a state changes in two ways: `PropertyChanges { target: box; width: 30 }` and
# `PropertyChanges { box.width: 30 }`. Both run on Qt, and the file that writes them side by side is
# `@qt-parity`. Ljós's API has only the second, and lowering a file to it is Ljós's: those scenarios,
# and the checker's, are `@ljos-only`.

Feature: States in Ljós's API
  A state says what its objects look like while it holds, each change
  named by the object's id: `PropertyChanges { box.width: 30 }`. A file
  in Qt's QML that names the object once, as a `target`, is lowered to
  that form. The names a file gives its states are the only values its
  `state` takes: a script that sets or compares another, or a state that
  extends one, is a check error before anything runs.

  Rule: A change written through a target is the change written through the id

    @qt-parity
    Scenario: A target's changes and an id's changes do the same
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 100; height: 60
          property bool lit: false
          Rectangle { id: left; objectName: "left"; width: 50; height: 60; color: "#2060c0" }
          Rectangle { id: right; objectName: "right"; x: 50; width: 50; height: 60; color: "#2060c0" }
          states: State {
            name: "lit"; when: root.lit
            PropertyChanges { target: left; color: "#ffffff"; anchors.leftMargin: 4; width: 40 }
            PropertyChanges { right.color: "#ffffff"; right.anchors.leftMargin: 4; right.width: 40 }
          }
        }
        """
      When the property "lit" of "root" is set to true
      Then the property "state" of "root" is "lit"
      And the property "width" of "left" is 40
      And the property "width" of "right" is 40
      And the pixel at 20, 30 of the window is "#ffffff"
      And the pixel at 70, 30 of the window is "#ffffff"
      When the property "lit" of "root" is set to false
      Then the property "width" of "left" is 50
      And the property "width" of "right" is 50
      And the pixel at 20, 30 of the window is "#2060c0"
      And the pixel at 70, 30 of the window is "#2060c0"

    @ljos-only
    Scenario: Lowered, a target's changes are written through its id
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 100; height: 60
          property bool lit: false
          Rectangle { id: left; objectName: "left"; width: 50; height: 60; color: "#2060c0" }
          Rectangle { id: right; objectName: "right"; x: 50; width: 50; height: 60; color: "#2060c0" }
          states: State {
            name: "lit"; when: root.lit
            PropertyChanges { target: left; color: "#ffffff"; anchors.leftMargin: 4; width: 40 }
            PropertyChanges { right.color: "#ffffff"; right.anchors.leftMargin: 4; right.width: 40 }
          }
        }
        """
      Then the property "fill" of "left" is "#2060c0"
      When the property "lit" of "root" is set to true
      Then the property "state" of "root" is "lit"
      And the property "fill" of "left" is "#ffffff"
      And the property "width" of "left" is 40
      And the property "width" of "right" is 40
      And the pixel at 20, 30 of the window is "#ffffff"
      And the pixel at 70, 30 of the window is "#ffffff"
      When the property "lit" of "root" is set to false
      Then the property "fill" of "left" is "#2060c0"
      And the property "width" of "left" is 50
      And the pixel at 20, 30 of the window is "#2060c0"

    @ljos-only @headless
    Scenario: ljos migrate writes a target's changes through its id
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          id: root
          Rectangle { id: face; color: "#2060c0" }
          states: [
            State { name: "lit"; PropertyChanges { target: face; color: "#ffffff"; border.width: 2 } },
            State {
              name: "dim"
              PropertyChanges {
                target: face
                explicit: true
                color: "#202020"
                onColorChanged: root.seen()
              }
            }
          ]
          function seen() {}
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (8 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        Item {
          id: root
          Rectangle { id: face; fill: "#2060c0" }
          states: [
            State { name: "lit"; PropertyChanges { face.fill: "#ffffff"; face.border.width: 2 } },
            State {
              name: "dim"
              PropertyChanges {

                explicit: true
                face.fill: "#202020"
                face.onFillChanged: root.seen()
              }
            }
          ]
          function seen() {}
        }
        """

    # The id form names an object by its id. A target that is a property or a path has none, and
    # stays a target, which Ljós's `PropertyChanges` still has.
    @ljos-only @headless
    Scenario: A target that is not an id stays a target
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          id: root
          property Item shown: face
          Rectangle { id: face }
          states: State { name: "lit"; PropertyChanges { target: root.shown; opacity: 0.5 } }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (1 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        Item {
          id: root
          property Item shown: face
          Rectangle { id: face }
          states: State { name: "lit"; PropertyChanges { target: root.shown; opacity: 0.5 } }
        }
        """

  Rule: An animation names a member by the name its object has in Ljós

    # An animation finds what it moves by name, on that object as it is: a `Rectangle`'s `color` is
    # `fill` in Ljós's API, so `Behavior on color` and `property: "color"` are lowered with it.
    @ljos-only
    Scenario: Lowered, a Behavior and an animation move the member by its Ljós name
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 100; height: 60
          Rectangle {
            id: left; objectName: "left"; width: 50; height: 60; color: "#000000"
            Behavior on color { ColorAnimation { duration: 1000 } }
          }
          Rectangle { id: right; objectName: "right"; x: 50; width: 50; height: 60; color: "#000000" }
          ColorAnimation { id: fade; target: right; property: "color"; to: "#ffffff"; duration: 1000 }
          function give() {
            left.color = "#ffffff"
            fade.start()
          }
        }
        """
      When the method "give" of "root" is called
      Then the property "fill" of "left" is "#000000"
      And the property "fill" of "right" is "#000000"
      When 1500 milliseconds pass
      Then the property "fill" of "left" is "#ffffff"
      And the property "fill" of "right" is "#ffffff"

    @ljos-only @headless
    Scenario: ljos migrate renames the members an animation names where it knows what it moves
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          Rectangle {
            id: face
            color: "#2060c0"
            Behavior on color { ColorAnimation { duration: 200 } }
          }
          Text { id: label; color: "#202020" }
          ColorAnimation { id: flash; target: face; properties: "color,opacity" }
          NumberAnimation { target: label; property: "color" }
          states: State { name: "lit"; PropertyChanges { face.color: "#ffffff" } }
          transitions: Transition { ColorAnimation { property: "color"; duration: 300 } }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (6 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui
        Item {
          Rectangle {
            id: face
            fill: "#2060c0"
            Behavior on fill { ColorAnimation { duration: 200 } }
          }
          Text { id: label; color: "#202020" }
          ColorAnimation { id: flash; target: face; properties: "fill,opacity" }
          NumberAnimation { target: label; property: "color" }
          states: State { name: "lit"; PropertyChanges { face.fill: "#ffffff" } }
          transitions: Transition { ColorAnimation { property: "fill"; duration: 300 } }
        }
        """

    # One string names the member on each object the animation moves; where that is `fill` on one
    # and `color` on another, no string is right for both, and a person splits the animation.
    @ljos-only @headless
    Scenario: An animation whose name is a different member on what it moves keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          Rectangle { id: face }
          Text { id: label }
          ColorAnimation { targets: [face, label]; property: "color" }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:5:55: an animation's "color" is `fill` on some of what it moves and `color` on the rest
        1 files, 0 lowered, 1 kept
        """

  Rule: A file's `state` is one of the names its states are given

    @ljos-only @headless
    Scenario: A state the file declares, or none, is set and compared with nothing to report
      Given the file "Button.qml":
        """qml
        import Ljos.Ui
        Item {
          id: button
          state: "normal"
          property bool down: state === "pressed"
          function press() { button.state = "pressed" }
          function reset() { state = "" }
          states: [
            State { name: "normal" },
            State { name: "pressed"; extend: "normal" }
          ]
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    @ljos-only @headless
    Scenario: A state the file does not declare is reported where it is set, compared or extended
      Given the file "Button.qml":
        """qml
        import Ljos.Ui
        Item {
          id: button
          state: "norml"
          property bool down: state === "presed"
          function press() { button.state = "presed" }
          states: [
            State { name: "normal" },
            State { name: "pressed"; extend: "norml" }
          ]
        }
        """
      When the files are checked
      Then the check reports:
        """
        Button.qml:4:18: error: `state` is `"" | "normal" | "pressed"`; this gives it `"norml"` [ts-assign-type]
        Button.qml:5:31: error: `===` compares `"" | "normal" | "pressed"` with `"presed"`, which it never is [ts-assign-type]
        Button.qml:6:45: error: `state` is `"" | "normal" | "pressed"`; this gives it `"presed"` [ts-assign-type]
        Button.qml:9:46: error: `extend` names `"norml"`, and the states are `"" | "normal" | "pressed"` [ts-assign-type]
        1 files, 4 errors, 0 warnings
        ljos: 4 errors
        """

    # Qt's apps keep the next state in a string property (coffee's `previousState`), and qml6 runs
    # them: a string a script holds names a state only as it runs, so it is let in, as `===` lets it.
    @ljos-only @headless
    Scenario: A string a script holds goes into `state`, and a name written out it lacks does not
      Given the file "Button.qml":
        """qml
        import Ljos.Ui
        Item {
          id: button
          property string previous: ""
          property bool wide: false
          function go(next: string) { button.state = next }
          function back() { state = previous }
          function pick() { state = wide ? "on" : "of" }
          states: [ State { name: "on" } ]
        }
        """
      When the files are checked
      Then the check reports:
        """
        Button.qml:8:37: error: `state` is `"" | "on"`; this gives it `"on" | "of"` [ts-assign-type]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    # A state named by an expression is known only as the file runs, and a component's states are
    # declared in its own file: `state` is then any string.
    @ljos-only @headless
    Scenario: A state named by an expression leaves `state` a string
      Given the file "Button.qml":
        """qml
        import Ljos.Ui
        Item {
          id: button
          property string prefix: "on"
          function press() { button.state = "anything" }
          states: [ State { name: button.prefix + "1" } ]
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """
