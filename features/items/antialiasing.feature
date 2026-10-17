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

  Scenario: A Rectangle's default follows a corner given later
    When the property "topLeftRadius" of "square" is set to 5
    Then the property "antialiasing" of "square" is true
    When the property "topLeftRadius" of "square" is set to 0
    Then the property "antialiasing" of "square" is false

  Scenario: A corner given nothing follows a radius given later
    When the property "radius" of "square" is set to 6
    Then the property "topLeftRadius" of "square" is 6
    And the property "bottomRightRadius" of "square" is 6

  Scenario: A corner given a value keeps it when a radius comes later
    When the property "topRightRadius" of "square" is set to 2
    And the property "radius" of "square" is set to 6
    Then the property "topRightRadius" of "square" is 2
    And the property "topLeftRadius" of "square" is 6
