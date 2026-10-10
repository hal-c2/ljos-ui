# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#antialiasing-prop
#   https://doc.qt.io/qt-6/qml-qtquick-rectangle.html#antialiasing-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Antialiasing defaults
  Items are not antialiased unless asked, except Text and rounded
  Rectangles.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Item { objectName: "item" }
        Rectangle { objectName: "square"; width: 20; height: 20 }
        Rectangle { objectName: "round"; width: 20; height: 20; radius: 4 }
        Text { objectName: "label"; text: "Aa" }
        Rectangle { objectName: "forced"; radius: 4; antialiasing: false }
      }
      """

  Scenario Outline: The default depends on the type
    Then the property "antialiasing" of "<object>" is <value>

    Examples:
      | object | value |
      | item   | false |
      | square | false |
      | round  | true  |
      | label  | true  |
      | forced | false |

  Scenario: A Rectangle's default follows its radius
    When the property "radius" of "square" is set to 6
    Then the property "antialiasing" of "square" is true
    When the property "radius" of "square" is set to 0
    Then the property "antialiasing" of "square" is false

  Scenario: An assigned value stays when the radius changes
    When the property "antialiasing" of "round" is set to false
    And the property "radius" of "round" is set to 9
    Then the property "antialiasing" of "round" is false
