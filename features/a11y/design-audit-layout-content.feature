# Sources:
#   docs/design/layout.md, docs/design/content.md and docs/design/typography.md (the rules and why)
#   https://www.w3.org/WAI/WCAG22/Understanding/reflow.html
#   https://www.w3.org/WAI/WCAG22/Understanding/visual-presentation.html
#   https://web.dev/articles/cls
#   https://www.nngroup.com/articles/ten-usability-heuristics/
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#textFormat-prop
# The design audit is a Ljós extension: Qt has nothing to compare it with.
# The names and roles of the live tree are in design-audit-names-layout-content.feature.

@ljos-only @a11y
Feature: The design audit checks layout and content in the live tree
  How a window looks and what it says are only known once it is laid out
  and drawn. The design audit reads the drawn scene and says where things
  run into each other, miss a line, move after they are shown, or cannot
  be found, and where a text shows what a user should never read, says
  nothing, says it twice, or is too small or too wide to read.

  Rule: Layout, as the window draws it

    Scenario: Two labels placed apart that run into each other
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 200
          color: "white"
          Label { x: 20; y: 20; text: "Destination folder" }
          Label { x: 60; y: 22; text: "Downloads" }
        }
        """
      Then the design audit reports "layout-overlap" at line 7

    Scenario: Two labels a layout places side by side
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        import QtQuick.Layouts
        Rectangle {
          width: 400; height: 200
          color: "white"
          RowLayout {
            x: 20; y: 20
            Label { text: "Destination folder" }
            Label { text: "Downloads" }
          }
        }
        """
      Then the design audit does not report "layout-overlap"

    Scenario: One field of a column a little wider than the others
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 320; height: 200
          color: "white"
          Column {
            spacing: 8
            ComboBox { Accessible.name: "Paper"; model: ["A4", "A5"]; width: 200 }
            ComboBox { Accessible.name: "Tray"; model: ["Top", "Bottom"]; width: 200 }
            ComboBox { Accessible.name: "Sides"; model: ["One", "Two"]; width: 240 }
          }
        }
        """
      Then the design audit reports "layout-misaligned" at line 10

    Scenario: A status line that comes late pushes the rows under it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          id: page
          width: 300; height: 300
          color: "white"
          property bool busy: false
          Timer { interval: 200; running: true; onTriggered: page.busy = true }
          Column {
            Label { text: "Saving…"; visible: page.busy }
            Button { text: "Row one" }
            Button { text: "Row two" }
          }
        }
        """
      When 500 milliseconds pass
      Then the design audit reports "layout-shift" at line 10

    Scenario: A status line that has its room from the start
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          id: page
          width: 300; height: 300
          color: "white"
          property bool busy: false
          Timer { interval: 200; running: true; onTriggered: page.busy = true }
          Column {
            Label { text: "Saving…"; opacity: page.busy ? 1 : 0 }
            Button { text: "Row one" }
            Button { text: "Row two" }
          }
        }
        """
      When 500 milliseconds pass
      Then the design audit does not report "layout-shift"

    Scenario: Two popups that both dim the window
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 300
          color: "white"
          Popup { modal: true; Component.onCompleted: open(); Label { text: "First" } }
          Popup { modal: true; Component.onCompleted: open(); Label { text: "Second" } }
        }
        """
      Then the design audit reports "layout-stacked-scrims" at line 6

    Scenario: A split handle that draws nothing
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 200
          color: "white"
          SplitView {
            anchors.fill: parent
            handle: Item { implicitWidth: 6 }
            Rectangle { SplitView.preferredWidth: 150; color: "white" }
            Rectangle { SplitView.fillWidth: true; color: "white" }
          }
        }
        """
      Then the design audit reports "layout-resize-handle" at line 8

    Scenario: A wrapped row of buttons that starts a line with a separator
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 200; height: 160
          color: "white"
          Flow {
            width: parent.width; spacing: 6
            Repeater {
              model: ["Cut", "Copy", "Paste", "Undo"]
              Row {
                spacing: 6
                Rectangle { width: 1; height: 24; color: "#767676" }
                Button { text: modelData }
              }
            }
          }
        }
        """
      Then the design audit reports "layout-wrap-separator" at line 12

    Scenario: A menu a repeater fills with twelve items and no group
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 500
          color: "white"
          Menu {
            Component.onCompleted: open()
            Repeater { model: 12; MenuItem { text: "Item " + (index + 1) } }
          }
        }
        """
      Then the design audit reports "layout-menu-ungrouped" at line 6

    Scenario: Text that keeps its own width in a narrow window
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          anchors.fill: parent
          color: "white"
          Flickable {
            anchors.fill: parent
            contentWidth: notes.width; contentHeight: notes.height
            clip: true
            Label { id: notes; width: 640; wrapMode: Text.Wrap; text: "Release notes. ".repeat(300) }
          }
        }
        """
      When the design audit runs at font scale 1 and window size 320x480
      Then the design audit reports "layout-reflow" at line 6

    Scenario: Text that takes the width of a narrow window
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          anchors.fill: parent
          color: "white"
          Flickable {
            id: view
            anchors.fill: parent
            contentWidth: width; contentHeight: notes.height
            clip: true
            Label { id: notes; width: view.width; wrapMode: Text.Wrap; text: "Release notes. ".repeat(300) }
          }
        }
        """
      When the design audit runs at font scale 1 and window size 320x480
      Then the design audit does not report "layout-reflow"

