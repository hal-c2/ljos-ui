# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-animationcontroller.html
#   https://doc.qt.io/qt-6/qml-qtquick-numberanimation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: AnimationController
  An AnimationController moves an animation by hand: progress 0 is its start, 1 its end.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int whole: 0
        property int ended: 0
        Rectangle { id: box; objectName: "box"; x: 5 }
        NumberAnimation {
          id: move; objectName: "move"
          target: box; property: "x"; from: 100; to: 200; duration: 1000
          onFinished: ended++
        }
        NumberAnimation {
          id: count; target: root; property: "whole"; from: 0; to: 10; duration: 1000
        }
        AnimationController { id: ctl; objectName: "ctl"; animation: move }
        function counting(p) { ctl.animation = count; ctl.progress = p }
      }
      """

  Scenario: The controller puts its animation at the start
    Then the property "x" of "box" is 100
    And the property "progress" of "ctl" is 0

  Scenario: The controlled animation is not running
    Then the property "running" of "move" is false

  Scenario Outline: Progress <progress> shows the value at that point
    When the property "progress" of "ctl" is set to <progress>
    Then the property "x" of "box" is <x>

    Examples:
      | progress | x   |
      | 0.25     | 125 |
      | 0.5      | 150 |
      | 1        | 200 |

  Scenario: Progress is kept between 0 and 1
    When the property "progress" of "ctl" is set to 1.5
    Then the property "progress" of "ctl" is 1
    And the property "x" of "box" is 200

  Scenario: Reaching the end by hand does not finish the animation
    When the property "progress" of "ctl" is set to 1
    Then the property "ended" of "root" is 0

  Scenario: A change to the animation is used after reload()
    When the property "to" of "move" is set to 300
    And the method "reload" of "ctl" is called
    And the property "progress" of "ctl" is set to 0.5
    Then the property "x" of "box" is 200

  Scenario Outline: A whole-number property takes the nearest whole number at <progress>
    When the method "counting" of "root" is called with <progress>
    Then the property "whole" of "root" is <value>

    Examples:
      | progress | value |
      | 0.24     | 2     |
      | 0.25     | 3     |
      | 0.34     | 3     |
      | 0.35     | 4     |
      | 0.96     | 10    |
