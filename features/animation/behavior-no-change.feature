# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qml-qtquick-propertyanimation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A Behavior given a value that changes nothing
  A Behavior that is given the value its property already holds starts no animation. One
  given the value its running animation is going to carries on as it was. A value that goes
  and comes back before the property has moved starts an animation all the same: the
  animation runs its whole duration and writes no new value.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log
        property int moves
        property alias gliding: move.running
        Rectangle {
          id: box; objectName: "box"
          x: 100; color: "#000000"
          onXChanged: root.moves++
          Behavior on x {
            NumberAnimation {
              id: move; duration: 1000
              onRunningChanged: root.log += running ? "on " : "off "
            }
          }
          Behavior on color { ColorAnimation { id: tint; duration: 1000 } }
        }
        property alias tinting: tint.running
        function there() { box.x = 200; box.x = 100 }
        function tintThere() { box.color = "#ff0000"; box.color = "#000000" }
      }
      """

  Scenario: The value the property holds starts no animation
    When the property "x" of "box" is set to 100
    Then the property "gliding" of "root" is false
    And the property "log" of "root" is ""

  Scenario: A value that goes and comes back still runs the animation
    When the method "there" of "root" is called
    Then the property "gliding" of "root" is true
    And the property "x" of "box" is 100

  Scenario: The animation of a value that came back moves nothing
    When the method "there" of "root" is called
    And 500 milliseconds pass
    Then the property "x" of "box" is 100
    And the property "moves" of "root" is 0
    And the property "gliding" of "root" is true

  Scenario: The animation of a value that came back runs its whole duration
    When the method "there" of "root" is called
    And 1500 milliseconds pass
    Then the property "gliding" of "root" is false
    And the property "log" of "root" is "on off "
    And the property "moves" of "root" is 0

  Scenario: The value the animation is going to does not start it again
    When the property "x" of "box" is set to 200
    And 600 milliseconds pass
    And the property "x" of "box" is set to 200
    And 600 milliseconds pass
    Then the property "gliding" of "root" is false
    And the property "x" of "box" is 200
    And the property "log" of "root" is "on off "

  Scenario: A colour that goes and comes back still runs the animation
    When the method "tintThere" of "root" is called
    Then the property "tinting" of "root" is true
    And the property "color" of "box" is "#000000"

  Scenario: The colour animation of a colour that came back ends on time
    When the method "tintThere" of "root" is called
    And 1500 milliseconds pass
    Then the property "tinting" of "root" is false
    And the property "color" of "box" is "#000000"
