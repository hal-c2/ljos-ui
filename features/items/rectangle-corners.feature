# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-rectangle.html#radius-prop
#   https://doc.qt.io/qt-6/qml-qtquick-rectangle.html#topLeftRadius-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Rectangle corners that follow the radius
  A corner given no radius of its own has the rectangle's radius, and so
  does what reads it or hears it change, also when the radius is bound.

  Scenario: A corner given nothing tells of a bound radius changing
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property real r: 2
        property real seen: 0
        property real followed: box.bottomLeftRadius
        Rectangle {
          id: box
          objectName: "box"
          radius: root.r
          onTopLeftRadiusChanged: root.seen = topLeftRadius
        }
      }
      """
    When the property "r" of "root" is set to 6
    Then the property "seen" of "root" is 6
    And the property "followed" of "root" is 6
    And the property "topRightRadius" of "box" is 6
    And the property "antialiasing" of "box" is true
    When the property "r" of "root" is set to 0
    Then the property "antialiasing" of "box" is false
    And the property "followed" of "root" is 0
