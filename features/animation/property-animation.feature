# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertyanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-numberanimation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: PropertyAnimation and NumberAnimation
  A property animation takes one or more properties of one or more objects from a start value to
  an end value over its duration.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property real limit: 50
        property int ended: 0
        Rectangle { id: a; objectName: "a"; x: 5; width: limit }
        Rectangle { id: b; objectName: "b" }
        NumberAnimation { objectName: "plain" }
        NumberAnimation { objectName: "toOnly"; target: a; property: "x"; to: 40; duration: 100 }
        NumberAnimation { objectName: "fromOnly"; target: a; property: "x"; from: 90; duration: 100 }
        NumberAnimation { objectName: "instant"; target: a; property: "x"; to: 7; duration: 0 }
        PropertyAnimation {
          objectName: "many"; targets: [a, b]; properties: "x,y"; to: 30; duration: 100
        }
        NumberAnimation { objectName: "bound"; target: a; property: "width"; to: 200; duration: 100 }
        NumberAnimation { objectName: "lost"; target: a; property: "nope"; to: 1; duration: 100 }
        NumberAnimation { objectName: "free"; to: 1; duration: 100; onFinished: ended++ }
      }
      """

  Scenario Outline: The default of <property> is <value>
    Then the property "<property>" of "plain" is <value>

    Examples:
      | property         | value   |
      | duration         | 250     |
      | loops            | 1       |
      | running          | false   |
      | paused           | false   |
      | alwaysRunToEnd   | false   |
      | property         | ""      |
      | properties       | ""      |
      | easing.type      | 0       |
      | easing.amplitude | 1       |
      | easing.overshoot | 1.70158 |
      | easing.period    | 0.3     |

  Scenario: Without from, the animation starts at the value the property has
    When the method "start" of "toOnly" is called
    Then the property "x" of "a" is 5

  Scenario: The property ends at to
    When the method "start" of "toOnly" is called
    And 1 second passes
    Then the property "x" of "a" is 40

  Scenario: Without to, the property is left alone
    When the method "start" of "fromOnly" is called
    And 1 second passes
    Then the property "x" of "a" is 5
    And the property "running" of "fromOnly" is false

  Scenario: An animation of no duration is over as soon as it starts
    When the method "start" of "instant" is called
    Then the property "x" of "a" is 7
    And the property "running" of "instant" is false

  Scenario Outline: targets and properties animate "<property>" of "<object>"
    When the method "start" of "many" is called
    And 1 second passes
    Then the property "<property>" of "<object>" is 30

    Examples:
      | object | property |
      | a      | x        |
      | a      | y        |
      | b      | x        |
      | b      | y        |

  Scenario: Animating a bound property keeps its binding
    When the method "start" of "bound" is called
    And 1 second passes
    And the property "limit" of "root" is set to 80
    Then the property "width" of "a" is 80

  Scenario: The animation wins over the binding while it runs
    When the method "start" of "bound" is called
    And 1 second passes
    Then the property "width" of "a" is 200

  Scenario: A property the target does not have is reported
    When the method "start" of "lost" is called
    Then the log has a warning matching "NumberAnimation: Cannot animate non-existent property .nope."

  Scenario: An animation without a target still runs its duration
    When the method "start" of "free" is called
    And 1 second passes
    Then the property "ended" of "root" is 1
