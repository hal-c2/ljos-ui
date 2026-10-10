# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-enumerations.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#transformOrigin-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Enumerations of the built-in types
  An enum key is written Type.Key and is a number.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool flip: false
        property int origin: Item.Center
        property int inherited: Rectangle.BottomRight
        property int elide: Text.ElideRight
        property int wrap: Text.WordWrap
        property int fixed: Text.FixedHeight
        property int policy: TapHandler.WithinBounds
        property int ready: Loader.Ready
        property int direction: Qt.RightToLeft
        property int chosen: flip ? Text.AlignRight : Text.AlignLeft
        Text {
          objectName: "label"
          horizontalAlignment: Text.AlignHCenter
          verticalAlignment: root.flip ? Text.AlignBottom : Text.AlignVCenter
          transformOrigin: Item.TopLeft
        }
      }
      """

  Scenario Outline: A key has the number Qt documents
    Then the property "<property>" of "root" is <value>

    Examples:
      | property  | value |
      | origin    | 4     |
      | inherited | 8     |
      | elide     | 1     |
      | wrap      | 1     |
      | fixed     | 1     |
      | policy    | 1     |
      | ready     | 1     |
      | direction | 1     |
      | chosen    | 1     |

  Scenario Outline: An enum property reads as its key's number
    Then the property "<property>" of "label" is <value>

    Examples:
      | property            | value |
      | horizontalAlignment | 4     |
      | verticalAlignment   | 128   |
      | transformOrigin     | 0     |

  Scenario: A key inside an expression follows the expression
    When the property "flip" of "root" is set to true
    Then the property "chosen" of "root" is 2
    And the property "verticalAlignment" of "label" is 64
