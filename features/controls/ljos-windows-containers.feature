# Sources:
#   docs/DIALECT.md (Ljós's own API: §4.16 the controls, §6 the compat pass, §8 ljos migrate)
#   https://doc.qt.io/qt-6/qml-qtquick-controls-applicationwindow.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-pane.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-page.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-toolbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-toolbutton.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-delaybutton.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-progressbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tabbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-buttongroup.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Controls.Basic` is Ljós's own module, and lowering a file to it is Ljós's: those scenarios are
# `@ljos-only`. The file as written for Qt Quick Controls runs on both, and the lowered file is held
# to its numbers. The shapes are those of a notes window with bars and of a settings row, written
# here as cases of their own.

Feature: Ljós's controls as Basic draws them: windows, containers and more buttons
  Ljos.Controls.Basic has the application window with its header and
  footer, the containers (Control, Pane, Frame, GroupBox, Page, ToolBar),
  the tool, round and delay buttons, the progress bar and busy indicator,
  the tab bar with its tab buttons and the swipe view, and the radio
  button with the button group, which draws nothing. They are the
  controls of Qt Quick Controls' Basic style, made, laid out and taking
  input as they are, so a file of Qt's that uses them is lowered to
  Ljos.Controls.Basic with no other edit for them.

  Rule: An application window, its bars and its panes are Qt's

    @qt-parity
    Scenario: A window with bars as written for Qt
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        ApplicationWindow {
          objectName: "win"; width: 300; height: 200; visible: true; color: "#f0f0f0"
          header: ToolBar {
            objectName: "head"
            ToolButton { objectName: "open"; width: 60; text: "Open" }
          }
          footer: Pane { objectName: "foot"; implicitHeight: 30 }
          Page { objectName: "page"; title: "Notes"; anchors.fill: parent
            Frame { objectName: "frame"; GroupBox { title: "Size" } }
          }
        }
        """
      Then the property "color" of "win" is "#f0f0f0"
      And the property "width" of "head" is 300
      And the property "contentItem.y" of "win" is 40
      And the property "contentItem.height" of "win" is 130
      And the property "y" of "foot" is 170
      And the property "height" of "page" is 130

    @ljos-only
    Scenario: The same in Ljós's API
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls.Basic
        ApplicationWindow {
          objectName: "win"; width: 300; height: 200; visible: true; color: "#f0f0f0"
          header: ToolBar {
            objectName: "head"
            ToolButton { objectName: "open"; width: 60; text: "Open" }
          }
          footer: Pane { objectName: "foot"; implicitHeight: 30 }
          Page { objectName: "page"; title: "Notes"; anchors.fill: parent
            Frame { objectName: "frame"; GroupBox { title: "Size" } }
          }
        }
        """
      Then the property "color" of "win" is "#f0f0f0"
      And the property "width" of "head" is 300
      And the property "contentItem.y" of "win" is 40
      And the property "contentItem.height" of "win" is 130
      And the property "y" of "foot" is 170
      And the property "height" of "page" is 130

    @ljos-only
    Scenario: The window as written for Qt, lowered, does the same
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        ApplicationWindow {
          objectName: "win"; width: 300; height: 200; visible: true; color: "#f0f0f0"
          header: ToolBar {
            objectName: "head"
            ToolButton { objectName: "open"; width: 60; text: "Open" }
          }
          footer: Pane { objectName: "foot"; implicitHeight: 30 }
          Page { objectName: "page"; title: "Notes"; anchors.fill: parent
            Frame { objectName: "frame"; GroupBox { title: "Size" } }
          }
        }
        """
      Then the property "color" of "win" is "#f0f0f0"
      And the property "width" of "head" is 300
      And the property "contentItem.y" of "win" is 40
      And the property "contentItem.height" of "win" is 130
      And the property "y" of "foot" is 170
      And the property "height" of "page" is 130

  Rule: Tabs, progress, held buttons and button groups are Qt's

    @qt-parity
    Scenario: A settings row as written for Qt
      Given the application starts with the controls style "Basic"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          width: 300; height: 260
          TabBar { objectName: "bar"; width: 200
            TabButton { text: "Inbox" }
            TabButton { text: "Sent" }
          }
          DelayButton { objectName: "hold"; y: 50; width: 80; height: 40; text: "Hold" }
          ProgressBar { objectName: "progress"; y: 100; width: 100; to: 200; value: 50 }
          ButtonGroup { id: size; objectName: "size" }
          RadioButton { y: 130; width: 100; height: 40; text: "Small"; checked: true; ButtonGroup.group: size }
          RadioButton { y: 180; width: 100; height: 40; text: "Large"; ButtonGroup.group: size }
          RoundButton { objectName: "round"; x: 200; y: 50; text: "+" }
          BusyIndicator { objectName: "busy"; x: 200; y: 120; running: false }
          ToolButton { objectName: "tool"; x: 200; y: 180; width: 60; height: 40; text: "Tool"
            property int clicks: 0; onClicked: clicks += 1 }
        }
        """
      When the user presses the pointer at 150, 20
      And the user releases the pointer at 150, 20
      And the user presses the pointer at 20, 70
      And 400 milliseconds pass
      And the user releases the pointer at 20, 70
      And the user presses the pointer at 20, 200
      And the user releases the pointer at 20, 200
      And the user presses the pointer at 220, 200
      And the user releases the pointer at 220, 200
      Then the property "currentIndex" of "bar" is 1
      And the property "checked" of "hold" is true
      And the property "position" of "progress" is 0.25
      And the property "checkedButton.text" of "size" is "Large"
      And the property "radius" of "round" is 20
      And the property "running" of "busy" is false
      And the property "clicks" of "tool" is 1

    @ljos-only
    Scenario: The settings row as written for Qt, lowered where the style is Basic, does the same
      Given the application starts with the controls style "Basic"
      And Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          width: 300; height: 260
          TabBar { objectName: "bar"; width: 200
            TabButton { text: "Inbox" }
            TabButton { text: "Sent" }
          }
          DelayButton { objectName: "hold"; y: 50; width: 80; height: 40; text: "Hold" }
          ProgressBar { objectName: "progress"; y: 100; width: 100; to: 200; value: 50 }
          ButtonGroup { id: size; objectName: "size" }
          RadioButton { y: 130; width: 100; height: 40; text: "Small"; checked: true; ButtonGroup.group: size }
          RadioButton { y: 180; width: 100; height: 40; text: "Large"; ButtonGroup.group: size }
          RoundButton { objectName: "round"; x: 200; y: 50; text: "+" }
          BusyIndicator { objectName: "busy"; x: 200; y: 120; running: false }
          ToolButton { objectName: "tool"; x: 200; y: 180; width: 60; height: 40; text: "Tool"
            property int clicks: 0; onClicked: clicks += 1 }
        }
        """
      When the user presses the pointer at 150, 20
      And the user releases the pointer at 150, 20
      And the user presses the pointer at 20, 70
      And 400 milliseconds pass
      And the user releases the pointer at 20, 70
      And the user presses the pointer at 20, 200
      And the user releases the pointer at 20, 200
      And the user presses the pointer at 220, 200
      And the user releases the pointer at 220, 200
      Then the property "currentIndex" of "bar" is 1
      And the property "checked" of "hold" is true
      And the property "position" of "progress" is 0.25
      And the property "checkedButton.text" of "size" is "Large"
      And the property "radius" of "round" is 20
      And the property "running" of "busy" is false
      And the property "clicks" of "tool" is 1

  Rule: ljos migrate lowers them

    @ljos-only @headless
    Scenario: A window of Qt's controls is lowered to Ljos.Controls.Basic
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        ApplicationWindow {
          width: 320; height: 200; visible: true; title: "Notes"
          header: ToolBar { ToolButton { text: "Open" } }
          footer: Label { text: "Ready" }
          Pane { Label { text: "Content between the bars" } }
        }
        """
      When the author runs "ljos migrate --style Basic --write ."
      Then the command prints:
        """
        lowered ./Main.qml (2 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Controls.Basic
        ApplicationWindow {
          width: 320; height: 200; visible: true; title: "Notes"
          header: ToolBar { ToolButton { text: "Open" } }
          footer: Label { text: "Ready" }
          Pane { Label { text: "Content between the bars" } }
        }
        """

  Rule: A file in Ljós's API is checked against them

    @ljos-only @headless
    Scenario: A window of the controls in Ljós's API has nothing to report
      Given the file "Notes.qml":
        """qml
        import Ljos.Ui
        import Ljos.Controls.Basic
        ApplicationWindow {
          id: win
          visible: true
          property int opens: 0
          header: ToolBar { ToolButton { text: "Open"; onClicked: win.opens += 1 } }
          footer: ProgressBar { value: 0.5 }
          TabBar { TabButton { text: `Opened ${win.opens}` } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """
