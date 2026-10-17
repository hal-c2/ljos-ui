# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#pressAndHold-signal
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#pressAndHoldInterval-prop
#   https://doc.qt.io/qt-6/qml-qtquick-mouseevent.html#wasHeld-prop
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: A MouseArea's press and hold
  A press held for `pressAndHoldInterval` milliseconds says `pressAndHold`,
  wherever the pointer has moved in the meantime. When something handles
  `pressAndHold`, letting go then says `released` with `wasHeld` and is no
  click; when nothing does, the press is an ordinary click. The interval
  is the system's (800 ms) until given; a negative value, or `undefined`,
  reads as the system's again.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 100
        MouseArea {
          objectName: "held"
          width: 100; height: 100
          pressAndHoldInterval: 200
          property string log: ""
          onPressAndHold: (m) => log += "hold " + m.wasHeld + " " + m.x + " " + pressed + "; "
          onReleased: (m) => log += "released " + m.wasHeld + "; "
          onClicked: (m) => log += "clicked " + m.wasHeld + "; "
          function unset() { pressAndHoldInterval = undefined }
        }
        MouseArea {
          objectName: "unhandled"
          x: 100; width: 100; height: 100
          pressAndHoldInterval: 200
          property string log: ""
          onReleased: (m) => log += "released " + m.wasHeld + "; "
          onClicked: (m) => log += "clicked " + m.wasHeld + "; "
        }
        MouseArea { objectName: "plain" }
        MouseArea { objectName: "negative"; pressAndHoldInterval: -5 }
        MouseArea { objectName: "zero"; pressAndHoldInterval: 0 }
      }
      """

  Rule: A press held for the interval is held

    Scenario: The press is held once the interval is over
      When the user presses the pointer at 10, 10
      And 150 milliseconds pass
      Then the property "log" of "held" is ""
      When 100 milliseconds pass
      Then the property "log" of "held" is "hold true 10 true; "
      When the user releases the pointer at 10, 10
      Then the property "log" of "held" is "hold true 10 true; released true; "

    Scenario: Moving does not end the wait, and the event says where the pointer is
      When the user presses the pointer at 10, 10
      And the user moves the pointer to 60, 10
      And 300 milliseconds pass
      And the user releases the pointer at 60, 10
      Then the property "log" of "held" is "hold true 60 true; released true; "

    Scenario: A press let go before the interval is a click
      When the user presses the pointer at 10, 10
      And 100 milliseconds pass
      And the user releases the pointer at 10, 10
      And 300 milliseconds pass
      Then the property "log" of "held" is "released false; clicked false; "

    Scenario: With nothing handling it, a long press is still a click
      When the user presses the pointer at 110, 10
      And 300 milliseconds pass
      And the user releases the pointer at 110, 10
      Then the property "log" of "unhandled" is "released false; clicked false; "

  Rule: The interval reads as the system's until given one of its own

    Scenario Outline: The interval of "<area>" is <ms>
      Then the property "pressAndHoldInterval" of "<area>" is <ms>

      Examples:
        | area     | ms  |
        | plain    | 800 |
        | negative | 800 |
        | zero     | 0   |
        | held     | 200 |

    Scenario: A negative value written later reads as the system's
      When the property "pressAndHoldInterval" of "held" is set to -3
      Then the property "pressAndHoldInterval" of "held" is 800

    Scenario: undefined takes the interval back to the system's
      When the method "unset" of "held" is called
      Then the property "pressAndHoldInterval" of "held" is 800

    Scenario: A negative interval waits as long as the system's
      When the property "pressAndHoldInterval" of "held" is set to -1
      And the user presses the pointer at 10, 10
      And 500 milliseconds pass
      Then the property "log" of "held" is ""
      When 400 milliseconds pass
      Then the property "log" of "held" is "hold true 10 true; "
