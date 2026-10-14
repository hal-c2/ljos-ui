# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-checkbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: CheckBox
  A check box is a button that stays checked. With `tristate` it has a third,
  partially checked state.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        CheckBox {
          objectName: "c"
          property string log: ""
          onToggled: log += "toggled "
          onClicked: log += "clicked "
          onCheckStateChanged: log += "state" + checkState + " "
        }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: A check box without text is as large as its indicator and padding
      Then the property "<property>" of "c" is <value>

      Examples:
        | property                | value |
        | implicitWidth           | 46    |
        | implicitHeight          | 40    |
        | padding                 | 6     |
        | leftPadding             | 6     |
        | topPadding              | 6     |
        | spacing                 | 6     |
        | implicitIndicatorWidth  | 28    |
        | implicitIndicatorHeight | 28    |
        | implicitContentWidth    | 34    |
        | indicator.width         | 28    |
        | indicator.height        | 28    |
        | indicator.y             | 6     |
        | indicator.x             | 9     |
        | contentItem.x           | 6     |
        | contentItem.width       | 34    |

    @api-QtQuick.Controls.CheckBox.checkState @api-QtQuick.Controls.CheckBox.tristate
    Scenario Outline: A check box starts out unchecked
      Then the property "<property>" of "c" is <value>

      Examples:
        | property      | value |
        | checkable     | true  |
        | checked       | false |
        | checkState    | 0     |
        | tristate      | false |
        | autoExclusive | false |
        | icon.width    | 0     |

    Scenario: With a text the indicator sits at the leading edge
      When the property "text" of "c" is set to "Remember me"
      Then the property "indicator.x" of "c" is 6
      And the property "contentItem.x" of "c" is 6
      And the property "implicitWidth" of "c" is at least 60

    Scenario: The content makes room for the indicator and the spacing
      When the property "text" of "c" is set to "Remember me"
      And the property "spacing" of "c" is set to 20
      And the property "text" of "c" is bound to "implicitWidth - implicitContentWidth"
      Then the property "text" of "c" is "12"

    Scenario: Mirrored, the indicator sits at the other edge
      When the property "text" of "c" is set to "Remember me"
      And the property "width" of "c" is set to 200
      And the property "LayoutMirroring.enabled" of "c" is set to true
      Then the property "mirrored" of "c" is true
      And the property "indicator.x" of "c" is 166

    Scenario Outline: The colour of the indicator tells whether it is held down
      When the property "down" of "c" is set to <down>
      Then the property "indicator.color" of "c" is "<colour>"
      And the property "indicator.border.color" of "c" is "#bdbdbd"
      And the property "indicator.border.width" of "c" is 1

      Examples:
        | down  | colour  |
        | false | #ffffff |
        | true  | #f6f6f6 |

  @qt-parity
  Rule: A click checks and unchecks

    Scenario: A click checks the box
      When the user touches 20, 20
      Then the property "checked" of "c" is true
      And the property "checkState" of "c" is 2
      And the property "log" of "c" is "state2 toggled clicked "

    Scenario: A second click unchecks it
      When the user touches 20, 20
      And 1 second passes
      And the user touches 20, 20
      Then the property "checked" of "c" is false
      And the property "checkState" of "c" is 0

    Scenario: Checking from a script changes the state without a signal of the user
      When the property "checked" of "c" is set to true
      Then the property "checkState" of "c" is 2
      And the property "log" of "c" is "state2 "

    Scenario: Setting the state checks the box
      When the property "checkState" of "c" is set to 2
      Then the property "checked" of "c" is true

    @api-QtQuick.Controls.AbstractButton.toggle
    Scenario: toggle() checks and unchecks
      When the method "toggle" of "c" is called
      Then the property "checkState" of "c" is 2
      When the method "toggle" of "c" is called
      Then the property "checkState" of "c" is 0

    Scenario Outline: The states have names
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "names"
          property int unchecked: Qt.Unchecked
          property int partially: Qt.PartiallyChecked
          property int checked: Qt.Checked
        }
        """
      Then the property "<property>" of "names" is <value>

      Examples:
        | property  | value |
        | unchecked | 0     |
        | partially | 1     |
        | checked   | 2     |

  @qt-parity @api-QtQuick.Controls.CheckBox.tristate
  Rule: A tristate check box has a state between unchecked and checked

    Scenario: Clicks go from unchecked to partially checked to checked and back
      When the property "tristate" of "c" is set to true
      And the user touches 20, 20
      Then the property "checkState" of "c" is 1
      When 1 second passes
      And the user touches 20, 20
      Then the property "checkState" of "c" is 2
      When 1 second passes
      And the user touches 20, 20
      Then the property "checkState" of "c" is 0

    Scenario: A partially checked box does not count as checked
      When the property "tristate" of "c" is set to true
      And the property "checkState" of "c" is set to 1
      Then the property "checked" of "c" is false

    Scenario: A click on a tristate box is a click, not a toggle
      When the property "tristate" of "c" is set to true
      And the user touches 20, 20
      Then the property "log" of "c" is "state1 clicked "

    Scenario: toggle() skips the partial state
      When the property "tristate" of "c" is set to true
      And the method "toggle" of "c" is called
      Then the property "checkState" of "c" is 2
      When the method "toggle" of "c" is called
      Then the property "checkState" of "c" is 0

    Scenario: Without tristate a script can still set the partial state
      When the property "checkState" of "c" is set to 1
      Then the property "checkState" of "c" is 1
      And the property "checked" of "c" is false

    Scenario: A click on a partially checked box that is not tristate checks it
      When the property "checkState" of "c" is set to 1
      And the user touches 20, 20
      Then the property "checkState" of "c" is 2

  @qt-parity @api-QtQuick.Controls.CheckBox.nextCheckState
  Rule: The application can say which state comes next

    Scenario: nextCheckState decides what a click does
      Given the file "Next.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 200
          CheckBox {
            objectName: "n"
            y: 100
            tristate: true
            nextCheckState: function() { return checkState === Qt.Checked ? Qt.Unchecked : Qt.Checked }
          }
        }
        """
      When "Next.qml" is loaded
      And the user touches 20, 120
      Then the property "checkState" of "n" is 2
      When 1 second passes
      And the user touches 20, 120
      Then the property "checkState" of "n" is 0
