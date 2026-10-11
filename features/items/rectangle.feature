# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-rectangle.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Rectangle
  A Rectangle paints a filled, optionally rounded and bordered box.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 100; height: 100
        Rectangle { objectName: "box"; width: 40; height: 30 }
      }
      """

  Scenario Outline: Defaults
    Then the property "<property>" of "box" is <value>

    Examples:
      | property     | value     |
      | color        | "#ffffff" |
      | radius       | 0         |
      | border.width | 1         |
      | border.color | "#000000" |

  Scenario: The border is a grouped property
    When the property "border.width" of "box" is set to 3
    And the property "border.color" of "box" is set to "#204080"
    Then the property "border.width" of "box" is 3
    And the property "border.color" of "box" is "#204080"

  Scenario: The radius can be larger than the box
    When the property "radius" of "box" is set to 100
    Then the property "radius" of "box" is 100

  Scenario: A gradient replaces the colour
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "g"
        width: 40; height: 30
        property int stops: gradient ? gradient.stops.length : 0
        gradient: Gradient {
          GradientStop { position: 0; color: "red" }
          GradientStop { position: 1; color: "blue" }
        }
      }
      """
    Then the property "stops" of "g" is 2
