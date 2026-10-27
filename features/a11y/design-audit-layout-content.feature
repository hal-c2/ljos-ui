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

    Scenario: What the user's own click opens is not a shift
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          id: page
          width: 300; height: 300
          color: "white"
          property bool open: false
          Column {
            Button { text: "More"; onClicked: page.open = !page.open }
            Label { text: "The details"; visible: page.open }
            Button { text: "Row two" }
          }
        }
        """
      When the user clicks the "button" "More"
      And 500 milliseconds pass
      Then the design audit does not report "layout-shift"

    Scenario: What moves by itself after a click is still a shift
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          id: page
          width: 300; height: 300
          color: "white"
          property bool busy: false
          Column {
            Button { text: "Save"; onClicked: later.start() }
            Label { text: "Saving…"; visible: page.busy }
            Button { text: "Row two" }
          }
          Timer { id: later; interval: 700; onTriggered: page.busy = true }
        }
        """
      When the user clicks the "button" "Save"
      And 1000 milliseconds pass
      Then the design audit reports "layout-shift" at line 10

    Scenario: What the user's own click slides open is not a shift
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          id: page
          width: 300; height: 300
          color: "white"
          property bool open: false
          Column {
            Button { text: "More"; onClicked: page.open = !page.open }
            Rectangle {
              width: 200; height: page.open ? 80 : 0
              Behavior on height { NumberAnimation { duration: 250 } }
            }
            Button { text: "Row two" }
          }
        }
        """
      When the user clicks the "button" "More"
      And 1000 milliseconds pass
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

    Scenario: A side panel that keeps its width crushes the column that fills
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        import QtQuick.Layouts
        Rectangle {
          width: 400; height: 200
          color: "white"
          RowLayout {
            width: 300; height: 100
            ColumnLayout {
              Layout.fillWidth: true
              RowLayout {
                Flow {
                  Layout.fillWidth: true
                  Button { implicitWidth: 80; text: "Open" }
                  Button { implicitWidth: 80; text: "Save" }
                }
                Button { implicitWidth: 80; text: "Send" }
              }
            }
            Rectangle { Layout.preferredWidth: 180; Layout.fillHeight: true; color: "#eee" }
          }
        }
        """
      Then the design audit reports "layout-fill-crushed" at line 9

    Scenario: A column that fills with a minimum of its own
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        import QtQuick.Layouts
        Rectangle {
          width: 400; height: 200
          color: "white"
          RowLayout {
            width: 300; height: 100
            ColumnLayout {
              Layout.fillWidth: true
              Layout.minimumWidth: 165
              RowLayout {
                Flow {
                  Layout.fillWidth: true
                  Button { implicitWidth: 80; text: "Open" }
                  Button { implicitWidth: 80; text: "Save" }
                }
                Button { implicitWidth: 80; text: "Send" }
              }
            }
            Rectangle { Layout.preferredWidth: 120; Layout.fillHeight: true; color: "#eee" }
          }
        }
        """
      Then the design audit does not report "layout-fill-crushed"

    Scenario: A row too short for its content and the style's padding
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        import QtQuick.Layouts
        Rectangle {
          width: 400; height: 200
          color: "white"
          ItemDelegate {
            x: 10; y: 10; width: 300; height: 28
            contentItem: RowLayout {
              Label { text: "Medium"; Layout.fillWidth: true }
            }
          }
        }
        """
      Then the design audit reports "layout-content-overflow" at line 7

    Scenario: A row whose padding leaves its content room
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        import QtQuick.Layouts
        Rectangle {
          width: 400; height: 200
          color: "white"
          ItemDelegate {
            x: 10; y: 10; width: 300; height: 28
            verticalPadding: 4
            contentItem: RowLayout {
              Label { text: "Medium"; Layout.fillWidth: true }
            }
          }
        }
        """
      Then the design audit does not report "layout-content-overflow"

  Rule: Content, as the window says it

    Scenario: An identifier shown as it is stored
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 500; height: 100
          color: "white"
          Label { anchors.centerIn: parent; text: "Closed 3f2c8a1e-9b7d-4c1e-8f2a-5d6e7f8091a2" }
        }
        """
      Then the design audit reports "content-raw-identifier" at line 6

    Scenario: A name shown for what it is
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 500; height: 100
          color: "white"
          Label { anchors.centerIn: parent; text: "Closed “Quarterly report”" }
        }
        """
      Then the design audit does not report "content-raw-identifier"

    Scenario: Markdown marks in plain text
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 360; height: 100
          color: "white"
          Text {
            anchors.centerIn: parent
            textFormat: Text.PlainText
            text: "Run `make build` before **continuing**"
          }
        }
        """
      Then the design audit reports "text-raw-markup" at line 6

    Scenario: The same marks in text that renders them
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 360; height: 100
          color: "white"
          Text {
            anchors.centerIn: parent
            textFormat: Text.MarkdownText
            text: "Run `make build` before **continuing**"
          }
        }
        """
      Then the design audit does not report "text-raw-markup"

    Scenario: An empty list that shows only its heading
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 200
          color: "white"
          ListView {
            anchors.fill: parent
            model: 0
            header: Label { text: "Recent files"; font.bold: true }
            delegate: ItemDelegate { text: "Row" }
          }
        }
        """
      Then the design audit reports "content-empty-state" at line 6

    Scenario: An empty list that says it is empty
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 200
          color: "white"
          ListView {
            id: files
            anchors.fill: parent
            model: 0
            delegate: ItemDelegate { text: "Row" }
          }
          Label { anchors.centerIn: files; visible: files.count === 0; text: "No recent files yet" }
        }
        """
      Then the design audit does not report "content-empty-state"

    Scenario: An error that says its title again
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 160
          color: "white"
          Column {
            spacing: 4
            Label { text: "The export could not be finished." }
            Label { text: "The export could not be finished." }
          }
        }
        """
      Then the design audit reports "content-repeated-text" at line 9

    Scenario: Rows a user cannot tell apart
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 200
          color: "white"
          ListView {
            anchors.fill: parent
            model: 5
            delegate: ItemDelegate { width: ListView.view.width; text: "Run command" }
          }
        }
        """
      Then the design audit reports "content-identical-rows" at line 6

    Scenario: Rows named for what they are
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 200
          color: "white"
          ListView {
            anchors.fill: parent
            model: ["Build", "Test", "Deploy", "Clean", "Lint"]
            delegate: ItemDelegate { width: ListView.view.width; text: "Run " + modelData }
          }
        }
        """
      Then the design audit does not report "content-identical-rows"

    Scenario: One action named two ways
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 200
          color: "white"
          Column {
            spacing: 8
            Button { text: "Try again" }
            Button { text: "Retry" }
          }
        }
        """
      Then the design audit reports "content-inconsistent-words" at line 8
      And the design audit reports "content-inconsistent-words" at line 9

    Scenario: A menu item switched off with nothing to say why
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 200
          color: "white"
          Menu {
            Component.onCompleted: open()
            MenuItem { text: "Copy" }
            MenuItem { text: "Paste"; enabled: false }
          }
        }
        """
      Then the design audit reports "content-disabled-no-reason" at line 9

    Scenario: A menu item switched off that says what turns it on
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 200
          color: "white"
          Menu {
            Component.onCompleted: open()
            MenuItem { text: "Copy" }
            MenuItem {
              text: "Paste"; enabled: false
              Accessible.description: "Copy something first"
            }
          }
        }
        """
      Then the design audit does not report "content-disabled-no-reason"

    Scenario: A caption drawn too small to read
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 100
          color: "white"
          Label { x: 20; y: 20; text: "Updated just now"; font.pixelSize: 8 }
        }
        """
      Then the design audit reports "text-too-small" at line 6

    Scenario: A paragraph as wide as a wide window
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 1000; height: 400
          color: "white"
          Label {
            x: 20; width: 960; wrapMode: Text.Wrap
            text: "Each line of this paragraph runs the whole width of the window. ".repeat(12)
          }
        }
        """
      Then the design audit reports "text-line-length" at line 6

    Scenario: A paragraph with a measure of its own
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 1000; height: 400
          color: "white"
          Label {
            x: 20; width: 360; wrapMode: Text.Wrap
            text: "Each line of this paragraph runs the whole width of the window. ".repeat(12)
          }
        }
        """
      Then the design audit does not report "text-line-length"
