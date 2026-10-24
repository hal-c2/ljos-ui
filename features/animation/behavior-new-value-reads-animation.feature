# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qml-qtquick-propertyanimation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A new value on the way reads the animation as it is then
  A Behavior that gets a new value while its animation runs starts the animation
  again from where the property is. The animation goes there with the duration,
  easing and ends it has at that moment, not those it had when it began.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property alias dur: move.duration
        property alias curve: move.easing.type
        property real px: box.x
        Rectangle {
          id: box; objectName: "box"; x: 0
          Behavior on x { NumberAnimation { id: move; duration: 200 } }
        }
      }
      """

  Scenario: The same animation again takes the whole duration
    When the property "x" of "box" is set to 100
    And 100 milliseconds pass
    And the property "x" of "box" is set to 150
    And 100 milliseconds pass
    Then the property "px" of "root" is within 15 of 100

  Scenario: A duration changed on the way is taken by the next value
    When the property "x" of "box" is set to 100
    And 100 milliseconds pass
    And the property "dur" of "root" is set to 1000
    And the property "x" of "box" is set to 150
    And 500 milliseconds pass
    Then the property "px" of "root" is within 15 of 100

  Scenario: An easing changed on the way is taken by the next value
    When the property "x" of "box" is set to 100
    And 100 milliseconds pass
    And the property "dur" of "root" is set to 1000
    And the property "x" of "box" is set to 150
    And 100 milliseconds pass
    And the property "curve" of "root" is set to 1
    And the property "x" of "box" is set to 160
    And 500 milliseconds pass
    Then the property "px" of "root" is within 12 of 85
