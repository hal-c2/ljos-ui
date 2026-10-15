# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
#   https://doc.qt.io/qt-6/qml-font.html
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#sourceSize-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: PropertyChanges of one member of a value
  A change may name one member of a property that holds a value, such as `font.pixelSize` or
  `sourceSize.height`. The other members keep what they had, and leaving the state puts the
  member back.

  Background:
    Given a 16 by 16 image file "red.png" filled with "#ff0000"
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property int n: 20
        Image { id: pic; objectName: "pic"; source: "red.png"; sourceSize.width: 8; sourceSize.height: 50 }
        Text { id: label; objectName: "label"; text: "abc"; font.pixelSize: 12; font.italic: true }
        states: [
          State { name: "one"
            PropertyChanges { target: pic; sourceSize.height: 18 }
            PropertyChanges { target: label; font.pixelSize: 30 } },
          State { name: "two"
            PropertyChanges { target: label; font.pixelSize: 31; font.bold: true } },
          State { name: "bound"
            PropertyChanges { target: label; font.pixelSize: root.n } },
          State { name: "id"
            PropertyChanges { pic.sourceSize.height: 19 } }
        ]
      }
      """

  Scenario Outline: A member of a value is changed in the state
    When the property "state" of "root" is set to "one"
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property          | value |
      | pic    | sourceSize.height | 18    |
      | pic    | sourceSize.width  | 8     |
      | label  | font.pixelSize    | 30    |
      | label  | font.italic       | true  |

  Scenario: Two members of one value are both changed
    When the property "state" of "root" is set to "two"
    Then the property "font.pixelSize" of "label" is 31
    And the property "font.bold" of "label" is true
    And the property "font.italic" of "label" is true

  Scenario: A member named through an id is changed
    When the property "state" of "root" is set to "id"
    Then the property "sourceSize.height" of "pic" is 19

  Scenario: A member that is a binding follows what it reads
    When the property "state" of "root" is set to "bound"
    And the property "n" of "root" is set to 24
    Then the property "font.pixelSize" of "label" is 24

  Scenario Outline: Leaving the state puts the member back
    When the property "state" of "root" is set to "one"
    And the property "state" of "root" is set to ""
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property          | value |
      | pic    | sourceSize.height | 50    |
      | label  | font.pixelSize    | 12    |
