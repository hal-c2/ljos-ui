# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-spinbox.html#editable-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-spinbox.html#valueFromText-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-spinbox.html#live-prop
# Verified on Qt 6.11.2 (qml6, offscreen; pointer and key input over its vnc platform).

Feature: A SpinBox and the keyboard
  The arrow keys step a spin box that has the keyboard. In an editable one
  the number can be typed; it becomes the value when the typing is done.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 300
        SpinBox {
          objectName: "s"
          x: 20; y: 20
          property string log: ""
          onValueChanged: log += "v" + value + " "
          onValueModified: log += "m" + value + " "
        }
        SpinBox {
          objectName: "kg"
          x: 20; y: 100; value: 5; editable: true
          textFromValue: function(value, locale) { return value + " kg" }
          valueFromText: function(text, locale) { return parseInt(text) * 2 }
        }
        TextInput { objectName: "t"; y: 200; width: 100; height: 20 }
      }
      """

  @qt-parity
  Rule: A box takes the keyboard through its number

    Scenario: A click on the number gives the box the keyboard
      When the user touches 92, 40
      Then the property "activeFocus" of "s" is true
      And the property "contentItem.activeFocus" of "s" is true
      And the property "contentItem.cursorVisible" of "s" is false

    Scenario: The Tab key gives the box the keyboard
      When the user presses "Tab"
      Then the property "activeFocus" of "s" is true
      And the property "contentItem.activeFocus" of "s" is true
      When the user presses "Tab"
      Then the property "activeFocus" of "s" is false
      And the property "activeFocus" of "kg" is true

    Scenario: A script gives the keyboard to a box that is not editable, not to its number
      When the method "forceActiveFocus" of "s" is called
      Then the property "activeFocus" of "s" is true
      And the property "contentItem.activeFocus" of "s" is false

    Scenario: A script gives the keyboard to the number of an editable box
      When the property "editable" of "s" is set to true
      And the method "forceActiveFocus" of "s" is called
      Then the property "activeFocus" of "s" is true
      And the property "contentItem.activeFocus" of "s" is true
      And the property "contentItem.cursorVisible" of "s" is true

    Scenario: The number has a frame while the box has the keyboard
      Then the property "contentItem.children.0.visible" of "s" is false
      When the user presses "Tab"
      Then the property "contentItem.children.0.visible" of "s" is true

  @qt-parity
  Rule: The arrow keys step the value

    Scenario: Up is a step up by the user
      When the user presses "Tab"
      And the user presses "Up"
      Then the property "value" of "s" is 1
      And the property "log" of "s" is "v1 m1 "

    Scenario: Down is a step down
      When the property "value" of "s" is set to 5
      And the user presses "Tab"
      And the user presses "Down"
      Then the property "value" of "s" is 4

    Scenario: Down at the beginning does nothing
      When the user presses "Tab"
      And the user presses "Down"
      Then the property "value" of "s" is 0
      And the property "log" of "s" is ""

    Scenario: With wrap Down at the beginning goes to the end
      When the property "wrap" of "s" is set to true
      And the user presses "Tab"
      And the user presses "Down"
      Then the property "value" of "s" is 99

    Scenario: A held modifier changes nothing
      When the user presses "Tab"
      And the user presses "Shift+Up"
      Then the property "value" of "s" is 1

    Scenario Outline: <key> is not a key of a spin box
      When the property "value" of "s" is set to 5
      And the user presses "Tab"
      And the user presses "<key>"
      Then the property "value" of "s" is 5

      Examples:
        | key    |
        | PgUp   |
        | PgDown |
        | Home   |
        | End    |
        | Right  |
        | +      |

    Scenario: A box without the keyboard hears no keys
      When the user presses "Up"
      Then the property "value" of "s" is 0

    Scenario: Typing does nothing in a box that is not editable
      When the user presses "Tab"
      And the user types "5"
      Then the property "displayText" of "s" is "0"
      And the property "value" of "s" is 0

  # The oracle's input channel cannot hold a key down.
  @ljos-only
  Rule: A held arrow key steps again and again

    Scenario: Each repeat of the key is a step by the user
      When the user presses "Tab"
      And the user holds down "Up" until it repeats 3 times
      Then the property "value" of "s" is 4
      And the property "log" of "s" is "v1 m1 v2 m2 v3 m3 v4 m4 "

  @qt-parity
  Rule: In an editable box the number is typed

    Background:
      When the property "editable" of "s" is set to true

    @api-QtQuick.Controls.SpinBox.editable
    Scenario: An editable box has a number that can be changed
      Then the property "contentItem.readOnly" of "s" is false
      When the user touches 92, 40
      Then the property "contentItem.cursorVisible" of "s" is true

    Scenario: What is typed is shown and is not the value yet
      When the user touches 92, 40
      And the user types "42"
      Then the property "displayText" of "s" is "042"
      And the property "contentItem.text" of "s" is "042"
      And the property "value" of "s" is 0
      And the property "log" of "s" is ""

    Scenario Outline: <key> makes what was typed the value
      When the user touches 92, 40
      And the user types "42"
      And the user presses "<key>"
      Then the property "value" of "s" is 42
      And the property "displayText" of "s" is "42"
      And the property "log" of "s" is "v42 m42 "

      Examples:
        | key    |
        | Return |
        | Enter  |

    Scenario: Leaving the box with the Tab key makes what was typed the value
      When the user touches 92, 40
      And the user types "42"
      And the user presses "Tab"
      Then the property "value" of "s" is 42
      And the property "log" of "s" is "v42 m42 "
      And the property "activeFocus" of "s" is false

    Scenario: The keyboard going elsewhere makes what was typed the value
      When the user touches 92, 40
      And the user types "42"
      And the method "forceActiveFocus" of "t" is called
      Then the property "value" of "s" is 42
      And the property "log" of "s" is "v42 m42 "

    Scenario: A step by a button throws away what was typed
      When the user touches 92, 40
      And the user types "42"
      And the user touches 140, 40
      Then the property "value" of "s" is 1
      And the property "displayText" of "s" is "1"

    Scenario: A step by a key throws away what was typed
      When the user touches 92, 40
      And the user types "42"
      And the user presses "Up"
      Then the property "value" of "s" is 1
      And the property "displayText" of "s" is "1"

    Scenario: A digit that would take the number out of its range is refused
      When the user touches 92, 40
      And the user types "500"
      Then the property "displayText" of "s" is "050"
      When the user presses "Return"
      Then the property "value" of "s" is 50

    Scenario: Letters are refused
      When the user touches 92, 40
      And the user types "ab"
      Then the property "displayText" of "s" is "0"

    Scenario: An empty number is nought
      When the property "value" of "s" is set to 7
      And the user touches 92, 40
      And the user presses "Ctrl+A"
      And the user presses "Backspace"
      Then the property "displayText" of "s" is ""
      And the property "value" of "s" is 7
      When the user presses "Return"
      Then the property "value" of "s" is 0
      And the property "displayText" of "s" is "0"

    Scenario: A number below the range becomes its beginning
      When the property "from" of "s" is set to 10
      And the property "value" of "s" is set to 20
      And the user touches 92, 40
      And the user presses "Ctrl+A"
      And the user types "5"
      Then the property "value" of "s" is 20
      And the property "displayText" of "s" is "5"
      And the property "contentItem.acceptableInput" of "s" is false
      When the user presses "Return"
      Then the property "value" of "s" is 10
      And the property "displayText" of "s" is "10"
      And the property "log" of "s" is "v10 v20 v10 m10 "

    Scenario: Return on a number that was not changed modifies nothing
      When the user touches 92, 40
      And the user presses "Return"
      Then the property "log" of "s" is ""

    Scenario: A value set by a script replaces what is being typed
      When the user touches 92, 40
      And the user types "4"
      And the property "value" of "s" is set to 7
      Then the property "displayText" of "s" is "7"
      And the property "contentItem.text" of "s" is "7"

    @api-QtQuick.Controls.SpinBox.live
    Scenario: A live box takes each keystroke as its value, without calling it modified
      When the property "live" of "s" is set to true
      And the user touches 92, 40
      And the user types "42"
      Then the property "value" of "s" is 42
      And the property "displayText" of "s" is "42"
      And the property "log" of "s" is "v4 v42 "
      When the user presses "Return"
      Then the property "log" of "s" is "v4 v42 "

  @qt-parity
  Rule: valueFromText reads what was typed

    @api-QtQuick.Controls.SpinBox.valueFromText
    Scenario: The author's function makes the value
      When the user touches 90, 120
      And the user presses "Ctrl+A"
      And the user types "7"
      And the user presses "Return"
      Then the property "value" of "kg" is 14
      And the property "displayText" of "kg" is "14 kg"
