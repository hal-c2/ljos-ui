# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-switch.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Switch
  A switch is on or off. Its handle can be clicked or dragged to either end.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Switch {
          objectName: "s"
          property string log: ""
          onPressed: log += "pressed "
          onReleased: log += "released "
          onCanceled: log += "canceled "
          onToggled: log += "toggled "
          onClicked: log += "clicked "
        }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: A switch without text is as large as its indicator and padding
      Then the property "<property>" of "s" is <value>

      Examples:
        | property                | value |
        | implicitWidth           | 74    |
        | implicitHeight          | 40    |
        | padding                 | 6     |
        | spacing                 | 6     |
        | implicitIndicatorWidth  | 56    |
        | implicitIndicatorHeight | 28    |
        | implicitContentWidth    | 62    |
        | indicator.x             | 9     |
        | indicator.y             | 6     |
        | indicator.width         | 56    |
        | indicator.height        | 28    |

    @api-QtQuick.Controls.Switch.position @api-QtQuick.Controls.Switch.visualPosition
    Scenario Outline: A switch starts out off
      Then the property "<property>" of "s" is <value>

      Examples:
        | property       | value |
        | checkable      | true  |
        | checked        | false |
        | position       | 0     |
        | visualPosition | 0     |
        | autoExclusive  | false |

    Scenario: With a text the indicator sits at the leading edge
      When the property "text" of "s" is set to "Wi-Fi"
      Then the property "indicator.x" of "s" is 6

    Scenario: The content makes room for the indicator and the spacing
      When the property "text" of "s" is set to "Wi-Fi"
      And the property "text" of "s" is bound to "implicitWidth - implicitContentWidth"
      Then the property "text" of "s" is "12"

  @qt-parity
  Rule: On and off

    Scenario: A click switches it on
      When the user touches 30, 20
      Then the property "checked" of "s" is true
      And the property "position" of "s" is 1
      And the property "log" of "s" is "pressed toggled released clicked "

    Scenario: A second click switches it off
      When the user touches 30, 20
      And 1 second passes
      And the user touches 30, 20
      Then the property "checked" of "s" is false
      And the property "position" of "s" is 0

    Scenario: Switching on from a script moves the handle
      When the property "checked" of "s" is set to true
      Then the property "position" of "s" is 1
      And the property "visualPosition" of "s" is 1
      And the property "log" of "s" is ""

    Scenario: Mirrored, the handle travels the other way
      When the property "LayoutMirroring.enabled" of "s" is set to true
      Then the property "visualPosition" of "s" is 1
      When the property "checked" of "s" is set to true
      Then the property "position" of "s" is 1
      And the property "visualPosition" of "s" is 0

    @api-QtQuick.Controls.AbstractButton.toggle
    Scenario: toggle() switches
      When the method "toggle" of "s" is called
      Then the property "checked" of "s" is true
      And the property "position" of "s" is 1

  # The Qt oracle sends whole clicks; what a drag does was read from Qt
  # with its own test tool (QtTest) instead.
  @ljos-only
  Rule: The handle can be dragged

    Scenario: The position follows the pointer while it is dragged
      When the user presses the pointer at 12, 20
      And the user moves the pointer to 30, 20
      And the user moves the pointer to 51, 20
      Then the property "position" of "s" is 0.75
      And the property "checked" of "s" is false
      And the property "pressed" of "s" is true

    Scenario: Let go in the second half, the switch is on
      When the user presses the pointer at 12, 20
      And the user moves the pointer to 30, 20
      And the user moves the pointer to 51, 20
      And the user releases the pointer at 51, 20
      Then the property "checked" of "s" is true
      And the property "position" of "s" is 1
      And the property "log" of "s" is "pressed toggled released clicked "

    Scenario: Let go in the first half, it stays off
      When the user presses the pointer at 12, 20
      And the user moves the pointer to 30, 20
      And the user moves the pointer to 16, 20
      And the user releases the pointer at 16, 20
      Then the property "checked" of "s" is false
      And the property "position" of "s" is 0
      And the property "log" of "s" is "pressed released clicked "

    Scenario: A drag may leave the switch and still switches
      When the user presses the pointer at 12, 20
      And the user moves the pointer to 30, 20
      And the user moves the pointer to 250, 150
      Then the property "pressed" of "s" is true
      And the property "position" of "s" is 1
      When the user releases the pointer at 250, 150
      Then the property "checked" of "s" is true
      And the property "log" of "s" is "pressed toggled released clicked "

    Scenario: The position stops at the ends
      When the user presses the pointer at 30, 20
      And the user moves the pointer to 20, 20
      And the user moves the pointer to 0, 20
      Then the property "position" of "s" is 0

    # Read from Qt 6.11.2 with QtTest: halfway is not past the middle.
    Scenario: Let go at the middle, it stays off
      When the user presses the pointer at 12, 20
      And the user moves the pointer to 30, 20
      And the user moves the pointer to 37, 20
      Then the property "position" of "s" is 0.5
      When the user releases the pointer at 37, 20
      Then the property "checked" of "s" is false
      And the property "log" of "s" is "pressed released clicked "

    # Read from Qt 6.11.2 with QtTest: a drag moves the handle only once
    # it started on the indicator or has come to it.
    Scenario: A drag that starts on the text moves the handle once it reaches it
      When the property "width" of "s" is set to 200
      And the property "text" of "s" is set to "Wifi"
      And the user presses the pointer at 150, 20
      And the user moves the pointer to 120, 20
      And the user moves the pointer to 80, 20
      Then the property "position" of "s" is 0
      When the user moves the pointer to 48, 20
      Then the property "position" of "s" is 0.75
      When the user releases the pointer at 48, 20
      Then the property "checked" of "s" is true
      And the property "log" of "s" is "pressed toggled released clicked "
