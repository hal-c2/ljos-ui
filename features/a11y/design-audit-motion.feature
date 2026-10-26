# Sources:
#   docs/design/motion.md
#   https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html
#   https://m3.material.io/styles/motion/easing-and-duration
#   https://developer.apple.com/design/human-interface-guidelines/motion
# The design audit is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @a11y
Feature: The design audit runs the window's motion to check it
  How long an animation runs, how it eases and what it passes through
  cannot be read off one frame. The design audit closes and opens the
  popups that are open, moves focus, switches tabs, switches the colour
  scheme and reduces motion, and follows every frame on its own clock. It
  reports the animations the application declared, and puts back what it
  changed.

  Rule: Motion

    Scenario: A popup that eases in on a curve it takes from a property
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Rectangle {
          id: page
          width: 400; height: 300
          color: palette.window
          property int curve: Easing.InCubic
          Popup {
            x: 40; y: 40
            width: 200; height: 120
            Component.onCompleted: open()
            enter: Transition {
              NumberAnimation {
                property: "opacity"
                from: 0
                to: 1
                duration: 200
                easing.type: page.curve
              }
            }
            Label { text: qsTr("Saved") }
          }
        }
        """
      Then the design audit reports "motion-ease-in-enter"

    Scenario: A popup that eases out
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Rectangle {
          id: page
          width: 400; height: 300
          color: palette.window
          property int curve: Easing.OutCubic
          Popup {
            x: 40; y: 40
            width: 200; height: 120
            Component.onCompleted: open()
            enter: Transition {
              NumberAnimation {
                property: "opacity"
                from: 0
                to: 1
                duration: 200
                easing.type: page.curve
              }
            }
            Label { text: qsTr("Saved") }
          }
        }
        """
      Then the design audit does not report "motion-ease-in-enter"

    Scenario: A popup whose enter runs for a duration it takes from a property
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Rectangle {
          id: page
          width: 400; height: 300
          color: palette.window
          property int slow: 600
          Popup {
            x: 40; y: 40
            width: 200; height: 120
            Component.onCompleted: open()
            enter: Transition {
              NumberAnimation {
                property: "opacity"
                from: 0
                to: 1
                duration: page.slow
                easing.type: Easing.OutCubic
              }
            }
            Label { text: qsTr("Saved") }
          }
        }
        """
      Then the design audit reports "motion-duration"

    Scenario: A popup that comes in within the standard time
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Rectangle {
          id: page
          width: 400; height: 300
          color: palette.window
          property int slow: 200
          Popup {
            x: 40; y: 40
            width: 200; height: 120
            Component.onCompleted: open()
            enter: Transition {
              NumberAnimation {
                property: "opacity"
                from: 0
                to: 1
                duration: page.slow
                easing.type: Easing.OutCubic
              }
            }
            Label { text: qsTr("Saved") }
          }
        }
        """
      Then the design audit does not report "motion-duration"

    Scenario: A check box whose own feedback runs past the micro time
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Rectangle {
          id: page
          width: 300; height: 200
          color: palette.window
          property int feedback: 400
          CheckBox {
            id: box
            x: 20; y: 20
            text: qsTr("Wrap lines")
            indicator: Rectangle {
              x: box.leftPadding
              y: (box.height - height) / 2
              implicitWidth: 20; implicitHeight: 20
              radius: 4
              color: box.checked ? box.palette.highlight : box.palette.base
              border.color: box.palette.mid
              Behavior on color {
                ColorAnimation { duration: page.feedback }
              }
            }
          }
        }
        """
      Then the design audit reports "motion-duration"

    Scenario: A check box whose feedback is quick
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Rectangle {
          id: page
          width: 300; height: 200
          color: palette.window
          property int feedback: 150
          CheckBox {
            id: box
            x: 20; y: 20
            text: qsTr("Wrap lines")
            indicator: Rectangle {
              x: box.leftPadding
              y: (box.height - height) / 2
              implicitWidth: 20; implicitHeight: 20
              radius: 4
              color: box.checked ? box.palette.highlight : box.palette.base
              border.color: box.palette.mid
              Behavior on color {
                ColorAnimation { duration: page.feedback }
              }
            }
          }
        }
        """
      Then the design audit does not report "motion-duration"

    Scenario: A popup that grows from a scale it takes from a property
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Rectangle {
          id: page
          width: 400; height: 300
          color: palette.window
          property real start: 0
          Popup {
            x: 40; y: 40
            width: 200; height: 120
            Component.onCompleted: open()
            enter: Transition {
              NumberAnimation {
                property: "scale"
                from: page.start
                to: 1
                duration: 200
                easing.type: Easing.OutCubic
              }
            }
            Label { text: qsTr("Saved") }
          }
        }
        """
      Then the design audit reports "motion-scale-from-zero"

    Scenario: A popup that grows from most of its size
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Rectangle {
          id: page
          width: 400; height: 300
          color: palette.window
          property real start: 0.95
          Popup {
            x: 40; y: 40
            width: 200; height: 120
            Component.onCompleted: open()
            enter: Transition {
              NumberAnimation {
                property: "scale"
                from: page.start
                to: 1
                duration: 200
                easing.type: Easing.OutCubic
              }
            }
            Label { text: qsTr("Saved") }
          }
        }
        """
      Then the design audit does not report "motion-scale-from-zero"

    Scenario: A focus colour that fades in slowly on every Tab
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Rectangle {
          id: page
          width: 300; height: 200; color: palette.window
          property int fade: 300
          Column {
            x: 20; y: 20; spacing: 8
            Repeater {
              model: [qsTr("Open"), qsTr("Save")]
              Button {
                id: button
                required property string modelData
                text: modelData
                background: Rectangle {
                  implicitWidth: 100; implicitHeight: 40
                  radius: 4
                  color: button.activeFocus ? button.palette.highlight : button.palette.button
                  Behavior on color { ColorAnimation { duration: page.fade } }
                }
              }
            }
          }
        }
        """
      Then the design audit reports "motion-frequent"

    Scenario: A focus colour that changes at once
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Rectangle {
          id: page
          width: 300; height: 200; color: palette.window
          property int fade: 80
          Column {
            x: 20; y: 20; spacing: 8
            Repeater {
              model: [qsTr("Open"), qsTr("Save")]
              Button {
                id: button
                required property string modelData
                text: modelData
                background: Rectangle {
                  implicitWidth: 100; implicitHeight: 40
                  radius: 4
                  color: button.activeFocus ? button.palette.highlight : button.palette.button
                  Behavior on color { ColorAnimation { duration: page.fade } }
                }
              }
            }
          }
        }
        """
      Then the design audit does not report "motion-frequent"

    Scenario: A tab bar whose underline slides slowly
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Rectangle {
          id: page
          width: 400; height: 300; color: palette.window
          property int slide: 300
          TabBar {
            id: bar
            width: parent.width
            TabButton { text: qsTr("General") }
            TabButton { text: qsTr("Advanced") }
          }
          Item {
            anchors.top: bar.bottom; anchors.bottom: parent.bottom
            width: parent.width; clip: true
            Row {
              x: -bar.currentIndex * page.width
              Behavior on x {
                NumberAnimation { duration: page.slide; easing.type: Easing.OutCubic }
              }
              Label { width: page.width; text: qsTr("General settings") }
              Label { width: page.width; text: qsTr("Advanced settings") }
            }
          }
        }
        """
      Then the design audit reports "motion-frequent"

    Scenario: A tab bar whose underline moves quickly
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Rectangle {
          id: page
          width: 400; height: 300; color: palette.window
          property int slide: 80
          TabBar {
            id: bar
            width: parent.width
            TabButton { text: qsTr("General") }
            TabButton { text: qsTr("Advanced") }
          }
          Item {
            anchors.top: bar.bottom; anchors.bottom: parent.bottom
            width: parent.width; clip: true
            Row {
              x: -bar.currentIndex * page.width
              Behavior on x {
                NumberAnimation { duration: page.slide; easing.type: Easing.OutCubic }
              }
              Label { width: page.width; text: qsTr("General settings") }
              Label { width: page.width; text: qsTr("Advanced settings") }
            }
          }
        }
        """
      Then the design audit does not report "motion-frequent"

    Scenario: Delays a delegate takes from its model, far apart
      Given the QML:
        """qml
        import QtQuick

        Column {
          width: 200; height: 200
          Repeater {
            // The delays come from the model, which only the made rows know.
            model: [0, 150, 300]
            Rectangle {
              required property int modelData
              width: 100
              height: 20
              opacity: 0
              SequentialAnimation on opacity {
                PauseAnimation {
                  duration: modelData
                }
                NumberAnimation {
                  to: 1
                  duration: 150
                  easing.type: Easing.OutCubic
                }
              }
            }
          }
        }
        """
      Then the design audit reports "motion-stagger"

    Scenario: Delays a delegate takes from its model, close together
      Given the QML:
        """qml
        import QtQuick

        Column {
          width: 200; height: 200
          Repeater {
            // The delays come from the model, which only the made rows know.
            model: [0, 30, 60, 90]
            Rectangle {
              required property int modelData
              width: 100
              height: 20
              opacity: 0
              SequentialAnimation on opacity {
                PauseAnimation {
                  duration: modelData
                }
                NumberAnimation {
                  to: 1
                  duration: 150
                  easing.type: Easing.OutCubic
                }
              }
            }
          }
        }
        """
      Then the design audit does not report "motion-stagger"

    Scenario: A window that cross-fades into the other colour scheme
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Pane {
          id: page
          width: 300; height: 200
          background: Rectangle {
            color: page.palette.window
            Behavior on color {
              ColorAnimation { duration: 600 }
            }
          }
          Label { text: qsTr("Inbox") }
        }
        """
      Then the design audit reports "motion-theme-switch"

    Scenario: A window that switches colour scheme at once
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Pane {
          id: page
          width: 300; height: 200
          background: Rectangle {
            color: page.palette.window
          }
          Label { text: qsTr("Inbox") }
        }
        """
      Then the design audit does not report "motion-theme-switch"

    Scenario: A card that passes through black on its way to the other scheme
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Pane {
          id: page
          width: 300; height: 200
          Rectangle {
            width: 260; height: 60
            color: page.palette.button
            // Through black on the way, from one theme's card to the other's.
            Behavior on color {
              SequentialAnimation {
                ColorAnimation { to: "black"; duration: 40 }
                ColorAnimation { duration: 40 }
              }
            }
          }
        }
        """
      Then the design audit reports "motion-theme-switch"

    Scenario: A card that goes straight to the other scheme
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Pane {
          id: page
          width: 300; height: 200
          Rectangle {
            width: 260; height: 60
            color: page.palette.button
            Behavior on color {
              ColorAnimation { duration: 80 }
            }
          }
        }
        """
      Then the design audit does not report "motion-theme-switch"

    Scenario: A badge that keeps bobbing when motion is reduced
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Rectangle {
          width: 300; height: 200
          color: palette.window
          BusyIndicator { x: 200; y: 20; running: true }
          Rectangle {
            x: 20; y: 40
            width: 40; height: 40
            radius: 20
            color: palette.highlight
            SequentialAnimation on y {
              loops: Animation.Infinite
              NumberAnimation { to: 80; duration: 600; easing.type: Easing.InOutQuad }
              NumberAnimation { to: 40; duration: 600; easing.type: Easing.InOutQuad }
            }
          }
        }
        """
      Then the design audit reports "motion-reduced-ignored"

    Scenario: A badge that pulses when motion is reduced
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls

        Rectangle {
          width: 300; height: 200
          color: palette.window
          BusyIndicator { x: 200; y: 20; running: true }
          Rectangle {
            x: 20; y: 40
            width: 40; height: 40
            radius: 20
            color: palette.highlight
            // Pulses instead of moving: a fade is what reduced motion keeps.
            SequentialAnimation on opacity {
              loops: Animation.Infinite
              NumberAnimation { to: 0.4; duration: 600; easing.type: Easing.InOutQuad }
              NumberAnimation { to: 1; duration: 600; easing.type: Easing.InOutQuad }
            }
          }
        }
        """
      Then the design audit does not report "motion-reduced-ignored"

    Scenario: A binding that keeps every frame of an animation busy
      Given the QML:
        """qml
        import QtQuick

        Rectangle {
          width: 300; height: 200
          Rectangle {
            width: 40; height: 40
            color: "steelblue"
            NumberAnimation on x {
              from: 0; to: 200
              duration: 1000
              loops: Animation.Infinite
            }
            // Every step of x runs 30 ms of script.
            onXChanged: {
              const until = Date.now() + 30
              while (Date.now() < until) {}
            }
          }
        }
        """
      Then the design audit reports "motion-dropped-frames"

    Scenario: An animation whose frames do little
      Given the QML:
        """qml
        import QtQuick

        Rectangle {
          width: 300; height: 200
          Rectangle {
            width: 40; height: 40
            color: "steelblue"
            NumberAnimation on x {
              from: 0; to: 200
              duration: 1000
              loops: Animation.Infinite
            }

          }
        }
        """
      Then the design audit does not report "motion-dropped-frames"

