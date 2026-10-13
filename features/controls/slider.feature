# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-slider.html
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html#customizing-slider
#   https://doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Slider
  A slider picks a number between two ends by moving a handle along a
  track. The value is a number between `from` and `to`; the position is
  the same as a share of the track.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 500
        Slider {
          objectName: "s"
          property int moves: 0
          property real third: valueAt(0.33)
          onMoved: moves++
        }
        Slider {
          objectName: "tens"
          y: 50; from: 0; to: 100; stepSize: 10; value: 50
          property real half: valueAt(0.5)
        }
        Slider { objectName: "down"; x: 300; y: 100; orientation: Qt.Vertical; value: 0.25 }
        Slider { objectName: "back"; y: 350; from: 10; to: 0; value: 2 }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: A slider starts at the beginning of 0 to 1
      Then the property "<property>" of "s" is <value>

      Examples:
        | property           | value |
        | from               | 0     |
        | to                 | 1     |
        | value              | 0     |
        | stepSize           | 0     |
        | snapMode           | 0     |
        | live               | true  |
        | orientation        | 1     |
        | horizontal         | true  |
        | vertical           | false |
        | position           | 0     |
        | visualPosition     | 0     |
        | pressed            | false |
        | touchDragThreshold | -1    |
        | wheelEnabled       | false |
        | focusPolicy        | 11    |

    Scenario Outline: It is a track with a round handle on it
      Then the property "<property>" of "s" is <value>

      Examples:
        | property                 | value     |
        | implicitWidth            | 200       |
        | implicitHeight           | 40        |
        | padding                  | 6         |
        | availableWidth           | 188       |
        | availableHeight          | 28        |
        | implicitHandleWidth      | 28        |
        | implicitHandleHeight     | 28        |
        | implicitBackgroundWidth  | 200       |
        | implicitBackgroundHeight | 6         |
        | background.x             | 6         |
        | background.y             | 17        |
        | background.width         | 188       |
        | background.height        | 6         |
        | background.radius        | 3         |
        | background.color         | "#e4e4e4" |
        | handle.x                 | 6         |
        | handle.y                 | 6         |
        | handle.width             | 28        |
        | handle.height            | 28        |
        | handle.radius            | 14        |
        | handle.color             | "#ffffff" |
        | handle.border.color      | "#bdbdbd" |

    Scenario: The handle and the filled track follow the value
      When the property "value" of "s" is set to 0.5
      Then the property "position" of "s" is 0.5
      And the property "visualPosition" of "s" is 0.5
      And the property "handle.x" of "s" is 86

    Scenario: A vertical slider goes from the bottom up
      Then the property "implicitWidth" of "down" is 40
      And the property "implicitHeight" of "down" is 200
      And the property "position" of "down" is 0.25
      And the property "visualPosition" of "down" is 0.75
      And the property "handle.x" of "down" is 6
      And the property "handle.y" of "down" is 126
      And the property "background.x" of "down" is 17
      And the property "background.width" of "down" is 6
      And the property "background.height" of "down" is 188

  @qt-parity
  Rule: The value

    Scenario Outline: The value stays between the ends
      When the property "value" of "tens" is set to <given>
      Then the property "value" of "tens" is <kept>

      Examples:
        | given | kept |
        | 33    | 33   |
        | 200   | 100  |
        | -5    | 0    |

    Scenario: Moving an end takes the value with it
      When the property "value" of "tens" is set to 33
      And the property "from" of "tens" is set to 40
      Then the property "value" of "tens" is 40
      And the property "position" of "tens" is 0

    @api-QtQuick.Controls.Slider.increase
    Scenario: increase() and decrease() go one step and are not a move by the user
      When the method "increase" of "tens" is called
      Then the property "value" of "tens" is 60
      When the method "decrease" of "tens" is called
      And the method "decrease" of "tens" is called
      Then the property "value" of "tens" is 40

    Scenario: Without a step size a step is a tenth
      When the method "increase" of "s" is called
      Then the property "value" of "s" is within 0.0001 of 0.1
      And the property "moves" of "s" is 0

    Scenario: The ends may be given the other way round
      Then the property "position" of "back" is 0.8
      When the method "increase" of "back" is called
      Then the property "value" of "back" is within 0.0001 of 2.1
      And the property "position" of "back" is within 0.0001 of 0.79

    @api-QtQuick.Controls.Slider.valueAt
    Scenario: valueAt() is the value at a position
      Then the property "half" of "tens" is 50
      And the property "third" of "s" is 0.33

  @qt-parity
  Rule: The keyboard

    Scenario: The arrow keys along the slider step it, and that is a move
      When the method "forceActiveFocus" of "s" is called
      And the user presses "Right"
      Then the property "value" of "s" is within 0.0001 of 0.1
      And the property "moves" of "s" is 1
      When the user presses "Left"
      Then the property "value" of "s" is 0
      And the property "moves" of "s" is 2

    Scenario Outline: Other keys do not move a horizontal slider
      When the method "forceActiveFocus" of "tens" is called
      And the user presses "<key>"
      Then the property "value" of "tens" is 50

      Examples:
        | key       |
        | Up        |
        | Down      |
        | Home      |
        | End       |
        | PgUp      |
        | PgDown    |

    Scenario: A vertical slider steps with Up and Down
      When the method "forceActiveFocus" of "down" is called
      And the user presses "Up"
      Then the property "value" of "down" is within 0.0001 of 0.35
      When the user presses "Right"
      Then the property "value" of "down" is within 0.0001 of 0.35
      When the user presses "Down"
      Then the property "value" of "down" is within 0.0001 of 0.25
