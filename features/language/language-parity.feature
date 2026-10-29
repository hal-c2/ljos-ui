# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#default-properties
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#data-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#children-prop
#   https://doc.qt.io/qt-6/qml-list.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
#   https://doc.qt.io/qt-6/qml-qtqml-connections.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-aliases
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Language parity

  Scenario: Overriding the default property in a derived component keeps base children in base data
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        component Frame : Item {
          default property QtObject contentSlot
          Item { objectName: "chrome" }
        }
        Frame {
          id: frame; objectName: "frame"
          QtObject { objectName: "payload" }
        }
        property int frameChildren: frame.children.length
        property string chromeName: frame.children[0].objectName
        property string slotName: frame.contentSlot.objectName
      }
      """
    Then the property "frameChildren" of "root" is 1
    And the property "chromeName" of "root" is "chrome"
    And the property "slotName" of "root" is "payload"
