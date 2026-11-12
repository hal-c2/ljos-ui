# Sources:
#   docs/DIALECT.md (Ljós's own API: §4.1 the modules, §4.16 the controls, §6 the compat pass)
#   https://doc.qt.io/qt-6/qtquickcontrols-index.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-label.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tooltip.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollview.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Controls.Basic` is Ljós's own module, and lowering a file to it is Ljós's: those scenarios are
# `@ljos-only`. The file as written for Qt Quick Controls runs on both, and the file in Ljós's API
# is held to its numbers. The shapes are those of a settings pane and of a panel with a scrolled
# list, written here as cases of their own.

Feature: Ljós's controls as Basic draws them: the module Ljos.Controls.Basic
  A file written in Ljós's API that wants Qt's Basic look imports its
  controls from Ljos.Controls.Basic (Ljos.Controls is Ljós's own style,
  features/controls/ljos-style.feature): a Label, a Button and the AbstractButton under it, the attached
  ToolTip, a ScrollView and its ScrollBar, and a Popup. They are the
  controls of Qt Quick Controls' Basic style, made, drawn and taking
  input as they are, so a file of Qt's that imports QtQuick.Controls or
  its Basic style is lowered to Ljos.Controls.Basic with no other edit for
  them; a control the module has not taken yet keeps the file as it is
  written.

  Rule: A button, a label and a tool tip are Qt's

    @qt-parity
    Scenario: A button, a label and a tool tip as written for Qt
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 240; height: 160
          property int saves: 0
          property int resets: 0
          Button {
            id: save; objectName: "save"; x: 10; y: 10; text: "Save"
            onClicked: root.saves += 1
            ToolTip.text: "Writes the file"
            ToolTip.delay: 500
            readonly property string hint: ToolTip.text + " after " + ToolTip.delay
          }
          AbstractButton {
            objectName: "reset"; x: 10; y: 60; width: 40; height: 30
            onClicked: root.resets += 1
          }
          Label { objectName: "status"; x: 10; y: 100; text: "Saved " + root.saves }
          Label { objectName: "hint"; x: 10; y: 130; text: save.hint }
        }
        """
      When the user presses the pointer at 20, 20
      And the user releases the pointer at 20, 20
      And the user presses the pointer at 30, 75
      And the user releases the pointer at 30, 75
      Then the property "saves" of "root" is 1
      And the property "resets" of "root" is 1
      And the property "text" of "status" is "Saved 1"
      And the property "text" of "hint" is "Writes the file after 500"
      And the property "implicitHeight" of "save" is 40

    @ljos-only
    Scenario: The same in Ljós's API
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 240; height: 160
          property int saves: 0
          property int resets: 0
          Button {
            id: save; objectName: "save"; x: 10; y: 10; text: "Save"
            onClicked: root.saves += 1
            ToolTip.text: "Writes the file"
            ToolTip.delay: 500
            readonly property string hint: ToolTip.text + " after " + ToolTip.delay
          }
          AbstractButton {
            objectName: "reset"; x: 10; y: 60; width: 40; height: 30
            onClicked: root.resets += 1
          }
          Label { objectName: "status"; x: 10; y: 100; text: "Saved " + root.saves }
          Label { objectName: "hint"; x: 10; y: 130; text: save.hint }
        }
        """
      When the user presses the pointer at 20, 20
      And the user releases the pointer at 20, 20
      And the user presses the pointer at 30, 75
      And the user releases the pointer at 30, 75
      Then the property "saves" of "root" is 1
      And the property "resets" of "root" is 1
      And the property "text" of "status" is "Saved 1"
      And the property "text" of "hint" is "Writes the file after 500"
      And the property "implicitHeight" of "save" is 40

    @ljos-only
    Scenario: The file as written for Qt, lowered, does the same
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 240; height: 160
          property int saves: 0
          property int resets: 0
          Button {
            id: save; objectName: "save"; x: 10; y: 10; text: "Save"
            onClicked: root.saves += 1
            ToolTip.text: "Writes the file"
            ToolTip.delay: 500
            readonly property string hint: ToolTip.text + " after " + ToolTip.delay
          }
          AbstractButton {
            objectName: "reset"; x: 10; y: 60; width: 40; height: 30
            onClicked: root.resets += 1
          }
          Label { objectName: "status"; x: 10; y: 100; text: "Saved " + root.saves }
          Label { objectName: "hint"; x: 10; y: 130; text: save.hint }
        }
        """
      When the user presses the pointer at 20, 20
      And the user releases the pointer at 20, 20
      And the user presses the pointer at 30, 75
      And the user releases the pointer at 30, 75
      Then the property "saves" of "root" is 1
      And the property "resets" of "root" is 1
      And the property "text" of "status" is "Saved 1"
      And the property "text" of "hint" is "Writes the file after 500"
      And the property "implicitHeight" of "save" is 40

  Rule: A scroll view, its scroll bar and a popup are Qt's

    @qt-parity
    Scenario: A scrolled list and a popup as written for Qt
      Given the application starts with the controls style "Basic"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          width: 240; height: 200
          ScrollView {
            id: view; objectName: "view"; width: 100; height: 100
            ScrollBar.vertical: ScrollBar { objectName: "bar"; policy: ScrollBar.AlwaysOn }
            Rectangle { implicitWidth: 80; implicitHeight: 400; color: "#336699" }
          }
          Popup {
            id: popup; objectName: "popup"; x: 120; y: 10; width: 80; height: 40
            focus: true; closePolicy: Popup.CloseOnEscape
          }
        }
        """
      When 100 milliseconds pass
      Then the property "contentHeight" of "view" is 400
      And the property "size" of "bar" is 0.25
      And the property "visible" of "bar" is true
      When the method "open" of "popup" is called
      And 300 milliseconds pass
      Then the property "visible" of "popup" is true
      When the user presses "Escape"
      And 300 milliseconds pass
      Then the property "visible" of "popup" is false

    @ljos-only
    Scenario: The same in Ljós's API
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls.Basic
        Item {
          width: 240; height: 200
          ScrollView {
            id: view; objectName: "view"; width: 100; height: 100
            ScrollBar.vertical: ScrollBar { objectName: "bar"; policy: ScrollBar.AlwaysOn }
            Rectangle { implicitWidth: 80; implicitHeight: 400; fill: "#336699" }
          }
          Popup {
            id: popup; objectName: "popup"; x: 120; y: 10; width: 80; height: 40
            focus: true; closePolicy: Popup.CloseOnEscape
          }
        }
        """
      When 100 milliseconds pass
      Then the property "contentHeight" of "view" is 400
      And the property "size" of "bar" is 0.25
      And the property "visible" of "bar" is true
      When the method "open" of "popup" is called
      And 300 milliseconds pass
      Then the property "visible" of "popup" is true
      When the user presses "Escape"
      And 300 milliseconds pass
      Then the property "visible" of "popup" is false

    @ljos-only
    Scenario: The file as written for Qt, lowered where the style is Basic, does the same
      Given the application starts with the controls style "Basic"
      And Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          width: 240; height: 200
          ScrollView {
            id: view; objectName: "view"; width: 100; height: 100
            ScrollBar.vertical: ScrollBar { objectName: "bar"; policy: ScrollBar.AlwaysOn }
            Rectangle { implicitWidth: 80; implicitHeight: 400; color: "#336699" }
          }
          Popup {
            id: popup; objectName: "popup"; x: 120; y: 10; width: 80; height: 40
            focus: true; closePolicy: Popup.CloseOnEscape
          }
        }
        """
      When 100 milliseconds pass
      Then the property "contentHeight" of "view" is 400
      And the property "size" of "bar" is 0.25
      And the property "visible" of "bar" is true
      When the method "open" of "popup" is called
      And 300 milliseconds pass
      Then the property "visible" of "popup" is true
      When the user presses "Escape"
      And 300 milliseconds pass
      Then the property "visible" of "popup" is false

  Rule: Qt's controls are lowered to Ljos.Controls.Basic

    @ljos-only @headless
    Scenario: A file of the controls is lowered, and draws the same
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 240; height: 160; color: "white"
          Button { x: 10; y: 10; text: "Save"; ToolTip.text: "Writes the file" }
          Label { x: 10; y: 60; text: "Ready"; color: "#204080" }
          ScrollView {
            x: 120; y: 10; width: 100; height: 100
            ScrollBar.vertical.policy: ScrollBar.AlwaysOn
            Rectangle { implicitWidth: 80; implicitHeight: 400; color: "#336699" }
          }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (4 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Controls.Basic
        Rectangle {
          width: 240; height: 160; fill: "white"
          Button { x: 10; y: 10; text: "Save"; ToolTip.text: "Writes the file" }
          Label { x: 10; y: 60; text: "Ready"; color: "#204080" }
          ScrollView {
            x: 120; y: 10; width: 100; height: 100
            ScrollBar.vertical.policy: ScrollBar.AlwaysOn
            Rectangle { implicitWidth: 80; implicitHeight: 400; fill: "#336699" }
          }
        }
        """

    @ljos-only @headless
    Scenario: A lowered file of the controls draws what it drew as written
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 240; height: 160; color: "white"
          Button { x: 10; y: 10; text: "Save" }
          Label { x: 10; y: 60; text: "Ready"; color: "#204080" }
          Popup { x: 120; y: 10; width: 80; height: 40; visible: true }
        }
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

    @ljos-only @headless
    Scenario: The controls of the style the program starts with are lowered where that is Basic
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        Label { text: "Ready" }
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
        Label { text: "Ready" }
        """

    # Without `QtQuick` or `QtQml` the controls' import has the line the pass writes, and an import
    # of a module of Ljós's stays as it is.
    @ljos-only @headless
    Scenario: A file that imports only the controls is lowered too
      Given the file "Main.qml":
        """qml
        import QtQuick.Controls
        import Ljos
        Menu { title: qsTr("Window"); SystemMenu.role: SystemMenu.WindowRole }
        """
      When the author runs "ljos migrate --style Basic --write ."
      Then the command prints:
        """
        lowered ./Main.qml (1 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Controls.Basic
        import Ljos
        Menu { title: qsTr("Window"); SystemMenu.role: SystemMenu.WindowRole }
        """
      When the author runs "ljos check ."
      Then the command exits with status 0

    @ljos-only @headless
    Scenario: The controls of a program that starts with another style keep the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        Label { text: "Ready" }
        """
      When the author runs "ljos migrate --style Fusion ."
      Then the command prints:
        """
        kept ./Main.qml:2:1: it imports `QtQuick.Controls`, which is the Fusion style where it runs, and Ljos.Controls.Basic is Basic
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A control Ljos.Controls.Basic has not taken yet keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          Label { text: "Volume" }
          Dial { y: 20 }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:5:3: `Dial` is not in Ljos.Controls.Basic yet
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: An enum of a control Ljos.Controls.Basic has not taken yet keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Label { text: "Volume"; property int mode: Dial.SnapAlways }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:3:44: `Dial` is not in Ljos.Controls.Basic yet
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A style of its own, and the controls under another name, keep the file
      Given the file "Styled.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Material
        Button { text: "Save"; Material.accent: Material.Teal }
        """
      And the file "Named.qml":
        """qml
        import QtQuick
        import QtQuick.Controls as C
        C.Button { text: "Save" }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Named.qml:2:1: it imports `QtQuick.Controls` under a name
        kept ./Styled.qml:2:1: it imports `QtQuick.Controls.Material`, which is not lowered yet
        2 files, 0 lowered, 2 kept
        """

  Rule: A file in Ljós's API is checked against the controls

    @ljos-only @headless
    Scenario: A file of the controls in Ljós's API has nothing to report
      Given the file "Pane.qml":
        """qml
        import Ljos.Ui
        import Ljos.Controls.Basic
        Item {
          id: pane
          property int saves: 0
          Button { text: "Save"; onClicked: pane.saves += 1; ToolTip.text: "Writes the file" }
          Label { text: `Saved ${pane.saves}` }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """
