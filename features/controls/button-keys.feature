# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

Feature: The keys that press a button
  A key that clicks a button presses it when it goes down and clicks it
  when it comes up, as a finger does.

  @qt-parity
  Scenario Outline: <key> presses a <control>, lets it go and clicks it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        <control> {
          objectName: "button"
          focus: true
          property string said: ""
          onPressed: said += "pressed."
          onReleased: said += "released."
          onClicked: said += "clicked."
        }
      }
      """
    When the user presses "<key>"
    Then the property "said" of "button" is "pressed.released.clicked."

    Examples:
      | control  | key    |
      | Button   | Space  |
      | Button   | Return |
      | Button   | Enter  |
      | CheckBox | Return |
      | Switch   | Enter  |
      | MenuItem | Space  |

  @qt-parity
  Scenario: A key that presses nothing is left to whoever wants it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        width: 300; height: 200
        property int keys: 0
        Keys.onPressed: keys++
        Button { objectName: "button"; focus: true; onClicked: root.keys = -1 }
      }
      """
    When the user presses "A"
    Then the property "keys" of "root" is 1

  @qt-parity
  Scenario: A button that is switched off is not pressed by a key
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Button {
          objectName: "button"
          focus: true
          property int clicks: 0
          onClicked: clicks++
        }
      }
      """
    When the property "enabled" of "button" is set to false
    And the user presses "Space"
    Then the property "clicks" of "button" is 0

  # The oracle's input platform does not mark repeated presses as repeats.
  @ljos-only
  Scenario: A key held down clicks once
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Button {
          objectName: "button"
          focus: true
          property int clicks: 0
          onClicked: clicks++
        }
      }
      """
    When the user holds down "Space" until it repeats 3 times
    Then the property "clicks" of "button" is 1
