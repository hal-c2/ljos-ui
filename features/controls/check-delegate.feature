# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-checkdelegate.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-radiodelegate.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-switchdelegate.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: CheckDelegate, RadioDelegate and SwitchDelegate
  The rows of a list that check, choose or switch: an item delegate with
  the indicator of a check box, a radio button or a switch at its far
  end, which a click anywhere on the row works.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        CheckDelegate {
          objectName: "c"; width: 200; text: "Check"
          property string log: ""
          onToggled: log += "t "; onClicked: log += "c "
        }
        RadioDelegate { objectName: "r1"; y: 60; width: 200; text: "A" }
        RadioDelegate { objectName: "r2"; y: 120; width: 200; text: "B" }
        SwitchDelegate {
          objectName: "s"; y: 180; width: 200; text: "S"
          property string log: ""
          onPressed: log += "pressed "; onReleased: log += "released "
          onToggled: log += "toggled "; onClicked: log += "clicked "
        }
        CheckDelegate { objectName: "e"; y: 240 }
        CheckDelegate {
          objectName: "t"; y: 300; width: 200; tristate: true
          property string log: ""
          onToggled: log += "t "; onClicked: log += "c "
        }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: A row with its indicator at the far end
      Then the property "<property>" of "<row>" is <value>

      Examples:
        | row | property               | value |
        | c   | implicitHeight         | 52    |
        | c   | padding                | 12    |
        | c   | spacing                | 12    |
        | c   | focusPolicy            | 0     |
        | c   | checkable              | true  |
        | c   | autoExclusive          | false |
        | c   | highlighted            | false |
        | c   | checkState             | 0     |
        | c   | tristate               | false |
        | c   | indicator.x            | 160   |
        | c   | indicator.y            | 12    |
        | c   | indicator.width        | 28    |
        | c   | indicator.height       | 28    |
        | c   | implicitIndicatorWidth | 28    |
        | c   | contentItem.x          | 12    |
        | c   | contentItem.width      | 176   |
        | c   | background.visible     | false |
        | e   | implicitWidth          | 100   |
        | e   | indicator.x            | 60    |
        | r1  | implicitHeight         | 52    |
        | r1  | autoExclusive          | true  |
        | r1  | checkable              | true  |
        | r1  | indicator.x            | 160   |
        | r1  | indicator.width        | 28    |
        | s   | implicitHeight         | 52    |
        | s   | checkable              | true  |
        | s   | position               | 0     |
        | s   | visualPosition         | 0     |
        | s   | indicator.x            | 132   |
        | s   | indicator.width        | 56    |
        | s   | indicator.height       | 28    |
        | s   | implicitIndicatorWidth | 56    |

  @qt-parity
  Rule: A click works the indicator

    Scenario: A check delegate is checked and unchecked
      When the user presses the pointer at 100, 26
      And the user releases the pointer at 100, 26
      Then the property "checked" of "c" is true
      And the property "checkState" of "c" is 2
      And the property "log" of "c" is "t c "
      When the user presses the pointer at 100, 26
      And the user releases the pointer at 100, 26
      Then the property "checked" of "c" is false

    @api-QtQuick.Controls.CheckDelegate.tristate
    Scenario: One with three states goes round them and does not say toggled
      When the user presses the pointer at 100, 326
      And the user releases the pointer at 100, 326
      Then the property "checkState" of "t" is 1
      When the user presses the pointer at 100, 326
      And the user releases the pointer at 100, 326
      Then the property "checkState" of "t" is 2
      When the user presses the pointer at 100, 326
      And the user releases the pointer at 100, 326
      Then the property "checkState" of "t" is 0
      And the property "log" of "t" is "c c c "

    Scenario: Partly checked is not checked
      When the property "checkState" of "c" is set to 1
      Then the property "checked" of "c" is false
      When the property "checked" of "c" is set to true
      Then the property "checkState" of "c" is 2

    Scenario: Radio delegates beside each other are exclusive
      When the user presses the pointer at 100, 86
      And the user releases the pointer at 100, 86
      And the user presses the pointer at 100, 146
      And the user releases the pointer at 100, 146
      Then the property "checked" of "r1" is false
      And the property "checked" of "r2" is true
      When the user presses the pointer at 100, 146
      And the user releases the pointer at 100, 146
      Then the property "checked" of "r2" is true

    @api-QtQuick.Controls.SwitchDelegate.position
    Scenario: A switch delegate's handle goes with the check
      When the user presses the pointer at 100, 206
      And the user releases the pointer at 100, 206
      Then the property "checked" of "s" is true
      And the property "position" of "s" is 1
      And the property "visualPosition" of "s" is 1

    Scenario: A mirrored switch delegate has its handle the other way round and its indicator at the left
      When the property "checked" of "s" is set to true
      And the property "LayoutMirroring.enabled" of "s" is set to true
      Then the property "visualPosition" of "s" is 0
      And the property "indicator.x" of "s" is 12
      And the property "contentItem.x" of "s" is 12

  # The Qt oracle sends whole clicks; what a drag does was read from Qt
  # 6.11.2 with its own test tool (QtTest) instead. The indicator of "s" is
  # 56 wide at x 132, and the row's middle is at y 206.
  @ljos-only @api-QtQuick.Controls.SwitchDelegate.position
  Rule: The handle of a switch delegate can be dragged

    Scenario: The handle follows the pointer while it is dragged
      When the user presses the pointer at 138, 206
      And the user moves the pointer to 156, 206
      And the user moves the pointer to 174, 206
      Then the property "position" of "s" is 0.75
      And the property "visualPosition" of "s" is 0.75
      And the property "checked" of "s" is false
      And the property "pressed" of "s" is true

    Scenario: Let go past the middle, the switch delegate is on
      When the user presses the pointer at 138, 206
      And the user moves the pointer to 156, 206
      And the user moves the pointer to 174, 206
      And the user releases the pointer at 174, 206
      Then the property "checked" of "s" is true
      And the property "position" of "s" is 1
      And the property "log" of "s" is "pressed toggled released clicked "

    Scenario: Let go at the middle, it stays off
      When the user presses the pointer at 138, 206
      And the user moves the pointer to 156, 206
      And the user moves the pointer to 160, 206
      Then the property "position" of "s" is 0.5
      When the user releases the pointer at 160, 206
      Then the property "checked" of "s" is false
      And the property "position" of "s" is 0
      And the property "log" of "s" is "pressed released clicked "

    Scenario: Dragged back past the middle, it is off
      When the property "checked" of "s" is set to true
      And the user presses the pointer at 182, 206
      And the user moves the pointer to 158, 206
      And the user moves the pointer to 146, 206
      Then the property "position" of "s" is 0.25
      When the user releases the pointer at 146, 206
      Then the property "checked" of "s" is false
      And the property "position" of "s" is 0
      And the property "log" of "s" is "pressed toggled released clicked "

    Scenario: A drag away from the handle does not move it, and is a click
      When the user presses the pointer at 20, 206
      And the user moves the pointer to 40, 206
      And the user moves the pointer to 120, 206
      Then the property "position" of "s" is 0
      And the property "pressed" of "s" is true
      When the user moves the pointer to 190, 206
      Then the property "position" of "s" is 0
      When the user releases the pointer at 190, 206
      Then the property "checked" of "s" is true
      And the property "log" of "s" is "pressed toggled released clicked "

    Scenario: A drag that reaches the handle moves it
      When the user presses the pointer at 20, 206
      And the user moves the pointer to 40, 206
      And the user moves the pointer to 120, 206
      And the user moves the pointer to 174, 206
      Then the property "position" of "s" is 0.75
      When the user moves the pointer to 120, 206
      Then the property "position" of "s" is 0
      When the user releases the pointer at 120, 206
      Then the property "checked" of "s" is false
      And the property "log" of "s" is "pressed released clicked "

    Scenario: A drag may leave the row and still switches
      When the user presses the pointer at 138, 206
      And the user moves the pointer to 156, 206
      And the user moves the pointer to 350, 300
      Then the property "pressed" of "s" is true
      And the property "position" of "s" is 1
      When the user releases the pointer at 350, 300
      Then the property "checked" of "s" is true
      And the property "log" of "s" is "pressed toggled released clicked "

    Scenario: A mirrored handle is dragged the other way
      When the property "LayoutMirroring.enabled" of "s" is set to true
      And the user presses the pointer at 62, 206
      And the user moves the pointer to 44, 206
      And the user moves the pointer to 26, 206
      Then the property "position" of "s" is 0.75
      And the property "visualPosition" of "s" is 0.25
      When the user releases the pointer at 26, 206
      Then the property "checked" of "s" is true
