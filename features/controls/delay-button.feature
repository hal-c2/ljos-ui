# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-delaybutton.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: DelayButton
  A delay button has to be held down for `delay` milliseconds before it
  does anything: its `progress` goes from 0 to 1 while it is held, it says
  `activated` when the progress is full, and it is checked when it is let
  go. Let go too soon, the progress goes back to 0 and nothing happens.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        DelayButton {
          objectName: "b"; text: "Hold"
          property string log: ""
          onPressed: log += "p "
          onReleased: log += "r "
          onClicked: log += "c "
          onActivated: log += "a "
          onToggled: log += "t "
          onCheckedChanged: log += "k "
        }
        DelayButton { objectName: "slow"; y: 100; delay: 1000 }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: A delay button is a checkable button that waits 300 milliseconds
      Then the property "<property>" of "b" is <value>

      Examples:
        | property       | value |
        | delay          | 300   |
        | progress       | 0     |
        | checkable      | true  |
        | checked        | false |
        | autoRepeat     | false |
        | focusPolicy    | 11    |
        | implicitWidth  | 100   |
        | implicitHeight | 40    |
        | leftPadding    | 8     |
        | rightPadding   | 8     |
        | topPadding     | 6     |

  @qt-parity
  Rule: Holding it down

    @api-QtQuick.Controls.DelayButton.activated
    Scenario: Held long enough, it is activated, and checked when let go
      When the user presses the pointer at 20, 20
      And 400 milliseconds pass
      Then the property "progress" of "b" is 1
      And the property "checked" of "b" is false
      And the property "log" of "b" is "p a "
      When the user releases the pointer at 20, 20
      Then the property "checked" of "b" is true
      And the property "log" of "b" is "p a k r c "

    Scenario: A checked one is unchecked by the next click, at once
      When the property "checked" of "b" is set to true
      And the property "log" of "b" is set to ""
      And the user presses the pointer at 20, 20
      And 400 milliseconds pass
      Then the property "progress" of "b" is 1
      And the property "log" of "b" is "p "
      When the user releases the pointer at 20, 20
      Then the property "progress" of "b" is 0
      And the property "checked" of "b" is false
      And the property "log" of "b" is "p k r c "

    @api-QtQuick.Controls.DelayButton.progress
    Scenario: Let go too soon, the progress goes back and nothing happens
      When the user presses the pointer at 20, 120
      And 500 milliseconds pass
      Then the property "progress" of "slow" is within 0.05 of 0.5
      When the user releases the pointer at 20, 120
      Then the property "progress" of "slow" is within 0.05 of 0.5
      And the property "checked" of "slow" is false
      When 400 milliseconds pass
      Then the property "progress" of "slow" is 0
      And the property "checked" of "slow" is false

    Scenario: A key that is let go at once is a click that does nothing
      When the method "forceActiveFocus" of "b" is called
      And the user presses "Space"
      Then the property "checked" of "b" is false
      And the property "log" of "b" is "p r c "

  @qt-parity
  Rule: The progress follows the check

    Scenario: A button checked by the application is full
      When the property "checked" of "b" is set to true
      Then the property "progress" of "b" is 1
      When the property "checked" of "b" is set to false
      Then the property "progress" of "b" is 0

    @api-QtQuick.Controls.DelayButton.delay
    Scenario: A longer delay takes longer
      When the user presses the pointer at 20, 120
      And 400 milliseconds pass
      Then the property "progress" of "slow" is within 0.05 of 0.4
      And the property "checked" of "slow" is false
