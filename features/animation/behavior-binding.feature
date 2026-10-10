# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-behaviors.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A Behavior on a bound property starts where the binding changes
  When the binding of a property with a Behavior gives a new value, the
  animation is running, and targetValue is the new value, by the time the
  statement that changed the binding's input has finished.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real goal: 30; property real size: 4
        property string log; property alias smooth: glide.enabled
        property var stillAim: still.targetValue; property var bareAim: bare.targetValue
        property var boundAim: bound.targetValue
        function push(v) {
          goal = v
          log += move.running + " " + glide.targetValue + " " + box.y + " "
        }
        Rectangle {
          id: box; objectName: "box"
          x: 5; y: root.goal; width: root.size; height: 4
          Behavior on x { id: still; enabled: false; NumberAnimation { } }
          Behavior on height { id: bare } Behavior on width { id: bound }
          Behavior on y {
            id: glide
            NumberAnimation {
              id: move; duration: 2000
              onRunningChanged: log += running ? "on " : "off "
            }
          }
        }
      }
      """

  Scenario: The animation runs before the next statement
    When the method "push" of "root" is called with 80
    Then the property "log" of "root" is "on true 80 30 "

  Scenario: A second value on the way is the target at once
    When the method "push" of "root" is called with 80
    And the method "push" of "root" is called with 60
    Then the property "log" of "root" is "on true 80 30 true 60 30 "

  Scenario: A value that is let through is there before the next statement
    When the property "smooth" of "root" is set to false
    And the method "push" of "root" is called with 50
    Then the property "log" of "root" is "false 50 50 "

  Scenario: A value let through stops the animation that was running at once
    When the method "push" of "root" is called with 80
    And the property "smooth" of "root" is set to false
    And the method "push" of "root" is called with 90
    Then the property "log" of "root" is "on true 80 30 off false 90 90 "

  Scenario: The animation still takes the property to the last value
    When the method "push" of "root" is called with 80
    And the method "push" of "root" is called with 60
    And 3 seconds pass
    Then the property "y" of "box" is 60
    And the property "log" of "root" is "on true 80 30 true 60 30 off "

  Scenario: targetValue is the value a disabled Behavior let through
    When the property "x" of "box" is set to 9
    Then the property "stillAim" of "root" is 9
    And the property "x" of "box" is 9

  Scenario: targetValue is the value a Behavior without an animation let through
    When the property "height" of "box" is set to 8
    Then the property "bareAim" of "root" is 8

  Scenario: targetValue is the value a binding gave a Behavior without an animation
    When the property "size" of "root" is set to 6
    Then the property "boundAim" of "root" is 6
    And the property "width" of "box" is 6
