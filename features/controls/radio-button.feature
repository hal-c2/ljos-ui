# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-radiobutton.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: RadioButton
  Of the radio buttons in one parent, one is checked. A click checks one and
  unchecks the other; it never unchecks the one clicked.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        RadioButton {
          objectName: "one"
          property string log: ""
          onToggled: log += "toggled "
          onClicked: log += "clicked "
        }
        RadioButton { objectName: "two"; y: 50 }
        Item {
          y: 100; width: 100; height: 50
          RadioButton { objectName: "apart"; checked: true }
        }
      }
      """

  Scenario Outline: A radio button without text is as large as its indicator and padding
    Then the property "<property>" of "one" is <value>

    Examples:
      | property                | value |
      | implicitWidth           | 46    |
      | implicitHeight          | 40    |
      | padding                 | 6     |
      | spacing                 | 6     |
      | implicitIndicatorWidth  | 28    |
      | implicitIndicatorHeight | 28    |
      | implicitContentWidth    | 34    |
      | indicator.x             | 9     |
      | indicator.y             | 6     |
      | indicator.radius        | 14    |
      | indicator.border.width  | 1     |

  Scenario Outline: A radio button is checkable and exclusive from the start
    Then the property "<property>" of "one" is <value>

    Examples:
      | property      | value |
      | checkable     | true  |
      | autoExclusive | true  |
      | checked       | false |

  Scenario: With a text the indicator sits at the leading edge
    When the property "text" of "one" is set to "First"
    Then the property "indicator.x" of "one" is 6

  Scenario Outline: The colour of the indicator tells whether it is held down
    When the property "down" of "one" is set to <down>
    Then the property "indicator.color" of "one" is "<colour>"
    And the property "indicator.border.color" of "one" is "#bdbdbd"

    Examples:
      | down  | colour  |
      | false | #ffffff |
      | true  | #f6f6f6 |

  Scenario: A click checks it
    When the user touches 20, 20
    Then the property "checked" of "one" is true
    And the property "log" of "one" is "toggled clicked "

  Scenario: A second click leaves it checked
    When the user touches 20, 20
    And 1 second passes
    And the user touches 20, 20
    Then the property "checked" of "one" is true
    And the property "log" of "one" is "toggled clicked clicked "

  Scenario: A click on another one moves the check
    When the user touches 20, 20
    And 1 second passes
    And the user touches 20, 70
    Then the property "checked" of "two" is true
    And the property "checked" of "one" is false

  Scenario: Checking from a script unchecks the other
    When the property "checked" of "one" is set to true
    And the property "checked" of "two" is set to true
    Then the property "checked" of "one" is false

  Scenario: Radio buttons in another parent are a group of their own
    When the user touches 20, 20
    Then the property "checked" of "apart" is true

  Scenario: A script can uncheck it
    When the property "checked" of "one" is set to true
    And the property "checked" of "one" is set to false
    Then the property "checked" of "one" is false
    And the property "checked" of "two" is false

  @api-QtQuick.Controls.AbstractButton.toggle
  Scenario: toggle() checks it and unchecks the other
    When the property "checked" of "two" is set to true
    And the method "toggle" of "one" is called
    Then the property "checked" of "one" is true
    And the property "checked" of "two" is false
