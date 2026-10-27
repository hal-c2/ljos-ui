# Sources:
#   docs/declarative.md (the rules and what to write instead)
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-states.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-animations.html
# The declarative lints are Ljós's own: Qt has nothing to compare them with.
# A file given here is written without the spaces its doc string is indented by, so the
# first column of a line is 1.

@ljos-only @headless
Feature: Script that a declaration says better
  A binding, a set of states and a transition are what the engine compiles
  and what the motion and focus rules can see; a handler that does the same
  by hand is neither. The authoring tool `qml_lint` hints where script does
  what a declaration says: a handler that copies a value, handlers that
  switch a group of properties between modes, animations started one after
  another, and an item shown and hidden from several handlers. Each hint
  says what to write instead. Changing state on an event, as a toggle does,
  is what handlers are for and is never flagged.

  Background:
    Given the authoring tools are connected

  Rule: A handler that only copies a value is a binding

    Scenario: A change handler that copies one property into another
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            id: root
            property real level: 0
            property real shown: 0
            onLevelChanged: shown = level * 100
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/0/code" is "decl-mirror-handler"
      And the answer's "/diagnostics/0/line" is 7
      And the answer's "/diagnostics/0/severity" is "hint"
      And the answer's "/diagnostics/0/message" matches "shown: level \* 100"
      And the answer's "/diagnostics/0/guide" is "docs/declarative.md#decl-mirror-handler"

    Scenario: A property other script writes too is state, not a copy
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            id: root
            property real level: 0
            property real shown: 0
            onLevelChanged: shown = level * 100
            Component.onCompleted: shown = 50
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer does not contain "decl-mirror-handler"

    Scenario: A handler that does more than copy is left alone
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            id: root
            property real level: 0
            property real shown: 0
            onLevelChanged: {
                shown = level * 100
                console.log("level", level)
            }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer does not contain "decl-mirror-handler"

  Rule: Handlers that switch a group of properties are states

    Scenario: Two buttons that each set the same two properties
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Column {
            Button { text: qsTr("List"); onClicked: { list.opacity = 1; editor.opacity = 0 } }
            Button { text: qsTr("Edit"); onClicked: { list.opacity = 0; editor.opacity = 1 } }
            Rectangle { id: list; width: 80; height: 40 }
            Rectangle { id: editor; width: 80; height: 40 }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/0/code" is "decl-mode-by-handlers"
      And the answer's "/diagnostics/0/line" is 5
      And the answer's "/diagnostics/0/message" matches "list\.opacity.*editor\.opacity"
      And the answer's "/diagnostics/0/fix" matches "PropertyChanges"
      And the answer does not contain "decl-visibility-juggling"

    Scenario: Handlers that set one property each are not a mode
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Column {
            property bool open: false
            Button { text: qsTr("Toggle"); onClicked: open = !open }
            Button { text: qsTr("Close"); onClicked: open = false }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer does not contain "decl-mode-by-handlers"

  Rule: Animations chained from script are one animation

    Scenario: An animation that starts the next when it finishes
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Button {
            id: button
            text: qsTr("Pulse")
            onClicked: grow.start()
            NumberAnimation { id: grow; target: button; property: "scale"; to: 1.1; duration: 120; easing.type: Easing.OutCubic
                onFinished: shrink.start()
            }
            NumberAnimation { id: shrink; target: button; property: "scale"; to: 1; duration: 120; easing.type: Easing.OutCubic }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/0/code" is "decl-imperative-animation"
      And the answer's "/diagnostics/0/line" is 9
      And the answer's "/diagnostics/0/message" matches "SequentialAnimation"

    Scenario: A one-off animation started by a click is left alone
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Button {
            id: button
            text: qsTr("Shake")
            onClicked: shake.start()
            NumberAnimation { id: shake; target: button; property: "rotation"; from: -3; to: 0; duration: 150; easing.type: Easing.OutCubic }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer does not contain "decl-imperative-animation"

  Rule: An item shown and hidden from several handlers is a state

    Scenario: A banner shown by one button and hidden by another
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Column {
            Rectangle { id: banner; width: 80; height: 20; visible: false }
            Button { text: qsTr("Show"); onClicked: banner.visible = true }
            Button { text: qsTr("Hide"); onClicked: banner.visible = false }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/0/code" is "decl-visibility-juggling"
      And the answer's "/diagnostics/0/line" is 6
      And the answer's "/diagnostics/0/message" matches "banner\.visible"

    Scenario: A toggle is a change of state, not juggling
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Column {
            Rectangle { id: banner; width: 80; height: 20; visible: false }
            Button { text: qsTr("Toggle"); onClicked: banner.visible = !banner.visible }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer does not contain "decl-visibility-juggling"

  Rule: The hints are the author's, and can be kept with a reason

    Scenario: An allow with a reason keeps the hint
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            property real level: 0
            property real shown: 0
            // ljos-allow(decl-mirror-handler): shown is reset by whoever loads this
            onLevelChanged: shown = level
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer does not contain "decl-mirror-handler"

    Scenario: The command line leaves the hints out
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            property real level: 0
            property real shown: 0
            onLevelChanged: shown = level
        }
        """
      When the author runs "ljos check ."
      Then the command exits with status 0
      And the command's output does not mention "decl-mirror-handler"

    Scenario: Each rule is explained under its code
      When the agent reads the resource "ljos://lints"
      Then the resource contains "docs/declarative.md#decl-mirror-handler"
      And the resource contains "docs/declarative.md#decl-mode-by-handlers"
      And the resource contains "docs/declarative.md#decl-imperative-animation"
      And the resource contains "docs/declarative.md#decl-visibility-juggling"
