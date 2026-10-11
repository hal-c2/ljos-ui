# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-combobox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#wheelEnabled-prop
#
# What a held, a resting and a turning pointer do to a combo box. The oracle
# sends whole clicks only, cannot move the pointer over a popup and has no
# wheel, so these run on Ljós only; every value here was read off Qt 6.11.2
# with Qt Test (mousePress, mouseMove, mouseRelease, mouseWheel).

@ljos-only
Feature: ComboBox and the pointer, where the oracle cannot look
  A combo box is pressed while the pointer is held on it, the row under the
  pointer is the highlighted one, and the wheel steps through the rows of a
  combo box that asks for it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string log: ""
        property string lit: ""
        ComboBox {
          id: c; objectName: "c"
          x: 20; y: 20; width: 100; height: 30
          model: ["One", "Two", "Three"]
          delegate: ItemDelegate {
            objectName: "d" + index; width: 100; height: 30; text: modelData
            highlighted: c.highlightedIndex === index
          }
          onActivated: (i) => root.log += "act" + i + ";"
          onCurrentIndexChanged: root.log += "ci" + currentIndex + ";"
          onHighlighted: (i) => root.lit += i
        }
      }
      """

  Rule: A combo box is pressed while the pointer is held on it

    @api-QtQuick.Controls.ComboBox.pressed
    Scenario: A held pointer presses the combo box without opening it
      When the user presses the pointer at 70, 35
      Then the property "pressed" of "c" is true
      And the property "down" of "c" is true
      And the property "activeFocus" of "c" is true
      And the property "popup.visible" of "c" is false

    @api-QtQuick.Controls.ComboBox.popup
    Scenario: The popup opens when the pointer is let go
      When the user presses the pointer at 70, 35
      And the user releases the pointer at 70, 35
      Then the property "pressed" of "c" is false
      And the property "popup.visible" of "c" is true
      And the property "down" of "c" is true

    Scenario: A pointer dragged off the combo box lets go of it
      When the user presses the pointer at 70, 35
      And the user moves the pointer to 300, 300
      Then the property "pressed" of "c" is false
      And the property "down" of "c" is false

    Scenario: A pointer let go off the combo box opens nothing
      When the user presses the pointer at 70, 35
      And the user moves the pointer to 300, 300
      And the user releases the pointer at 300, 300
      Then the property "popup.visible" of "c" is false
      And the property "pressed" of "c" is false

  Rule: The row under the pointer is the highlighted one

    @api-QtQuick.Controls.ComboBox.highlightedIndex
    Scenario: A pointer on a row highlights it
      When the user touches 70, 35
      And the user moves the pointer to 70, 95
      Then the property "highlightedIndex" of "c" is 1
      And the property "highlighted" of "d1" is true
      And the property "highlighted" of "d0" is false
      And the property "currentIndex" of "c" is 0

    @api-QtQuick.Controls.ComboBox.highlighted
    Scenario: The application is told of every row the pointer comes to
      When the user touches 70, 35
      And the user moves the pointer to 70, 95
      And the user moves the pointer to 70, 125
      Then the property "lit" of "root" is "12"
      And the property "highlightedIndex" of "c" is 2

    Scenario: A pointer on the row that is highlighted already says nothing
      When the user touches 70, 35
      And the user moves the pointer to 70, 65
      Then the property "lit" of "root" is ""
      And the property "highlightedIndex" of "c" is 0

  Rule: The wheel steps through the rows of a combo box that asks for it

    @api-QtQuick.Controls.Control.wheelEnabled
    Scenario: The wheel does nothing to a combo box that did not ask
      When the user turns the wheel 1 notch down at 70, 35
      Then the property "currentIndex" of "c" is 0
      And the property "log" of "root" is "ci0;"

    @api-QtQuick.Controls.Control.wheelEnabled
    Scenario Outline: The wheel turned <turns> ends on row <row>
      When the property "wheelEnabled" of "c" is set to true
      And the user turns the wheel 1 notch <first> at 70, 35
      And the user turns the wheel 1 notch <second> at 70, 35
      Then the property "currentIndex" of "c" is <row>
      And the property "log" of "root" is "<log>"

      Examples:
        | turns          | first | second | row | log                     |
        | down twice     | down  | down   | 2   | ci0;ci1;act1;ci2;act2;  |
        | down, then up  | down  | up     | 0   | ci0;ci1;act1;ci0;act0;  |
        | up twice       | up    | up     | 0   | ci0;                    |

    Scenario: The wheel stops at the last row
      When the property "wheelEnabled" of "c" is set to true
      And the property "currentIndex" of "c" is set to 2
      And the user turns the wheel 1 notch down at 70, 35
      Then the property "currentIndex" of "c" is 2
      And the property "log" of "root" is "ci0;ci2;"

    Scenario: The wheel leaves the rows of an open popup alone
      When the property "wheelEnabled" of "c" is set to true
      And the user touches 70, 35
      And the user turns the wheel 1 notch down at 70, 35
      Then the property "currentIndex" of "c" is 0
      And the property "highlightedIndex" of "c" is 0
