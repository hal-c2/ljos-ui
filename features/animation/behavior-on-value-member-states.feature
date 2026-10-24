# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-behaviors.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A Behavior on a member of a value follows a change of state
  A state that gives a member of a value (`font.pixelSize`, `p.x` of a `point`) a value hands
  it to the member's Behavior, as it does for a property: the animation takes the member
  there, and back when the state is left. The other members stay as they are.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real size: t.font.pixelSize
        property bool bold: t.font.bold
        property real px: r.p.x
        property alias running: anim.running
        Text {
          id: t; text: "x"; font.pixelSize: 10; font.bold: true
          Behavior on font.pixelSize { NumberAnimation { id: anim; duration: 1000 } }
        }
        Item {
          id: r; property point p: Qt.point(0, 7)
          Behavior on p.x { NumberAnimation { duration: 1000 } }
        }
        states: State {
          name: "big"
          PropertyChanges { t.font.pixelSize: 30; r.p.x: 100 }
        }
      }
      """

  Scenario: The state's value is held back
    When the property "state" of "root" is set to "big"
    Then the property "size" of "root" is 10
    And the property "running" of "root" is true

  Scenario: The animation takes the member to the state's value
    When the property "state" of "root" is set to "big"
    And 500 milliseconds pass
    Then the property "size" of "root" is 20
    And the property "bold" of "root" is true
    When 1000 milliseconds pass
    Then the property "size" of "root" is 30
    And the property "running" of "root" is false

  Scenario: Leaving the state animates the member back
    When the property "state" of "root" is set to "big"
    And 1500 milliseconds pass
    And the property "state" of "root" is set to ""
    Then the property "size" of "root" is 30
    When 500 milliseconds pass
    Then the property "size" of "root" is 20
    When 1000 milliseconds pass
    Then the property "size" of "root" is 10
    And the property "bold" of "root" is true

  Scenario: A member of a point follows the state
    When the property "state" of "root" is set to "big"
    Then the property "px" of "root" is 0
    When 500 milliseconds pass
    Then the property "px" of "root" is within 5 of 50
    When 1000 milliseconds pass
    Then the property "px" of "root" is 100
