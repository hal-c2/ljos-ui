# Sources:
#   docs/DIALECT.md (Ljós's own API: §4.16 the controls, §6 the compat pass)
#   https://doc.qt.io/qt-6/qml-qtquick-controls-checkbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-switch.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-combobox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-spinbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-slider.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Controls.Basic` is Ljós's own module, and lowering a file to it is Ljós's: those scenarios are
# `@ljos-only`. The file as written for Qt Quick Controls runs on both, and the file in Ljós's API
# is held to its numbers. The shape is that of a settings form, written here as a case of its own.
# The oracle cannot move the pointer over a popup, so the combo box is chosen from with its keys.

Feature: Ljós's controls: check boxes, switches, combo boxes, spin boxes and sliders
  Ljos.Controls.Basic has the check box, switch, combo box, spin box and
  slider of Qt Quick Controls' Basic style, set with the pointer and the
  keys as they are. A file of Qt's that uses them is lowered to
  Ljos.Controls.Basic with no other edit for them.

  Rule: The inputs are Qt's

    @qt-parity
    Scenario: A settings form as written for Qt
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 300; height: 300
          property int toggles: 0
          property int activations: 0
          property int moves: 0
          CheckBox {
            objectName: "wrap"; x: 10; y: 10; text: "Wrap"
            onToggled: root.toggles += 1
          }
          Switch { objectName: "dark"; x: 10; y: 60; text: "Dark" }
          ComboBox {
            objectName: "size"; x: 10; y: 110; width: 160
            model: ["Small", "Medium", "Large"]
            onActivated: root.activations += 1
          }
          SpinBox { objectName: "count"; x: 10; y: 170; from: 0; to: 10; value: 3 }
          Slider {
            objectName: "zoom"; x: 10; y: 230; width: 200
            from: 0; to: 1; stepSize: 0.25; value: 0.5
            onMoved: root.moves += 1
          }
        }
        """
      When the user presses the pointer at 30, 30
      And the user releases the pointer at 30, 30
      And the user presses the pointer at 30, 80
      And the user releases the pointer at 30, 80
      Then the property "checked" of "wrap" is true
      And the property "toggles" of "root" is 1
      And the property "checked" of "dark" is true
      When the method "forceActiveFocus" of "size" is called
      And the user presses "Down"
      Then the property "currentIndex" of "size" is 1
      And the property "currentText" of "size" is "Medium"
      And the property "activations" of "root" is 1
      When the method "forceActiveFocus" of "count" is called
      And the user presses "Up"
      Then the property "value" of "count" is 4
      When the method "forceActiveFocus" of "zoom" is called
      And the user presses "Right"
      Then the property "value" of "zoom" is 0.75
      And the property "moves" of "root" is 1

    @ljos-only
    Scenario: The same in Ljós's API
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 300; height: 300
          property int toggles: 0
          property int activations: 0
          property int moves: 0
          CheckBox {
            objectName: "wrap"; x: 10; y: 10; text: "Wrap"
            onToggled: root.toggles += 1
          }
          Switch { objectName: "dark"; x: 10; y: 60; text: "Dark" }
          ComboBox {
            objectName: "size"; x: 10; y: 110; width: 160
            model: ["Small", "Medium", "Large"]
            onActivated: root.activations += 1
          }
          SpinBox { objectName: "count"; x: 10; y: 170; from: 0; to: 10; value: 3 }
          Slider {
            objectName: "zoom"; x: 10; y: 230; width: 200
            from: 0; to: 1; stepSize: 0.25; value: 0.5
            onMoved: root.moves += 1
          }
        }
        """
      When the user presses the pointer at 30, 30
      And the user releases the pointer at 30, 30
      And the user presses the pointer at 30, 80
      And the user releases the pointer at 30, 80
      Then the property "checked" of "wrap" is true
      And the property "toggles" of "root" is 1
      And the property "checked" of "dark" is true
      When the method "forceActiveFocus" of "size" is called
      And the user presses "Down"
      Then the property "currentIndex" of "size" is 1
      And the property "currentText" of "size" is "Medium"
      And the property "activations" of "root" is 1
      When the method "forceActiveFocus" of "count" is called
      And the user presses "Up"
      Then the property "value" of "count" is 4
      When the method "forceActiveFocus" of "zoom" is called
      And the user presses "Right"
      Then the property "value" of "zoom" is 0.75
      And the property "moves" of "root" is 1

    @ljos-only
    Scenario: The file as written for Qt, lowered, does the same
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 300; height: 300
          property int toggles: 0
          property int activations: 0
          property int moves: 0
          CheckBox {
            objectName: "wrap"; x: 10; y: 10; text: "Wrap"
            onToggled: root.toggles += 1
          }
          Switch { objectName: "dark"; x: 10; y: 60; text: "Dark" }
          ComboBox {
            objectName: "size"; x: 10; y: 110; width: 160
            model: ["Small", "Medium", "Large"]
            onActivated: root.activations += 1
          }
          SpinBox { objectName: "count"; x: 10; y: 170; from: 0; to: 10; value: 3 }
          Slider {
            objectName: "zoom"; x: 10; y: 230; width: 200
            from: 0; to: 1; stepSize: 0.25; value: 0.5
            onMoved: root.moves += 1
          }
        }
        """
      When the user presses the pointer at 30, 30
      And the user releases the pointer at 30, 30
      And the user presses the pointer at 30, 80
      And the user releases the pointer at 30, 80
      Then the property "checked" of "wrap" is true
      And the property "toggles" of "root" is 1
      And the property "checked" of "dark" is true
      When the method "forceActiveFocus" of "size" is called
      And the user presses "Down"
      Then the property "currentIndex" of "size" is 1
      And the property "currentText" of "size" is "Medium"
      And the property "activations" of "root" is 1
      When the method "forceActiveFocus" of "count" is called
      And the user presses "Up"
      Then the property "value" of "count" is 4
      When the method "forceActiveFocus" of "zoom" is called
      And the user presses "Right"
      Then the property "value" of "zoom" is 0.75
      And the property "moves" of "root" is 1

  Rule: Qt's inputs are lowered to Ljos.Controls.Basic

    @ljos-only @headless
    Scenario: A file of inputs is lowered
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 240; height: 260; color: "white"
          CheckBox { y: 0; text: "Wrap" }
          Switch { y: 50; text: "Dark" }
          ComboBox { y: 100; model: ["Small", "Large"] }
          SpinBox { y: 150; value: 3 }
          Slider { y: 200; value: 0.5 }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (3 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Controls.Basic
        Rectangle {
          width: 240; height: 260; fill: "white"
          CheckBox { y: 0; text: "Wrap" }
          Switch { y: 50; text: "Dark" }
          ComboBox { y: 100; model: ["Small", "Large"] }
          SpinBox { y: 150; value: 3 }
          Slider { y: 200; value: 0.5 }
        }
        """

    @ljos-only @headless
    Scenario: A lowered file of inputs draws what it drew as written
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 240; height: 260; color: "white"
          CheckBox { y: 0; text: "Wrap"; checked: true }
          Switch { y: 50; text: "Dark"; checked: true }
          ComboBox { y: 100; model: ["Small", "Large"] }
          SpinBox { y: 150; value: 3 }
          Slider { y: 200; value: 0.5 }
        }
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

    @ljos-only @headless
    Scenario: A check box's partial state lowers with it
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        CheckBox { tristate: true; checkState: Qt.PartiallyChecked; text: "Some" }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        would lower ./Main.qml (3 edits)
        1 files, 1 lowered, 0 kept
        """

    @ljos-only
    Scenario: A lowered check box is still partly checked
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        CheckBox { objectName: "some"; tristate: true; checkState: Qt.PartiallyChecked; text: "Some" }
        """
      Then the property "checkState" of "some" is 1
      And the property "checked" of "some" is false

    @ljos-only @headless
    Scenario: A form of inputs in Ljós's API has nothing to report
      Given the file "Form.qml":
        """qml
        import Ljos.Ui
        import Ljos.Layout
        import Ljos.Controls.Basic
        Column {
          CheckBox { text: "Wrap" }
          Switch { text: "Dark" }
          Label { text: "Size" }
          ComboBox { model: ["Small", "Large"] }
          Label { text: "Count" }
          SpinBox { value: 3 }
          Label { text: "Zoom" }
          Slider { value: 0.5 }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """
