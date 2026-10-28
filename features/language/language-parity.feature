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

  Scenario: Multi-argument push and splice preserve order and update length
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property QtObject a: QtObject { objectName: "a" }
        property QtObject b: QtObject { objectName: "b" }
        property QtObject c: QtObject { objectName: "c" }
        property QtObject d: QtObject { objectName: "d" }
        property list<QtObject> items: [a]
        property int changes: 0
        property string names: ""
        onItemsChanged: changes++
        function dump() {
          var s = []
          for (var i = 0; i < items.length; ++i) s.push(items[i] ? items[i].objectName : "null")
          names = s.join(",")
        }
        function pushTwo() { items.push(b, c); dump() }
        function spliceMiddle() { items.splice(1, 1, d); dump() }
      }
      """
    When the method "pushTwo" of "root" is called
    Then the property "names" of "root" is "a,b,c"
    And the property "changes" of "root" is 3
    When the method "spliceMiddle" of "root" is called
    Then the property "names" of "root" is "a,d,c"

  Scenario: Expanding and truncating a list property's length pads with null and drops tail entries
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property QtObject a: QtObject { objectName: "a" }
        property QtObject b: QtObject { objectName: "b" }
        property list<QtObject> items: [a, b]
        property string summary: ""
        function resizeAndTruncate() {
          items.length = 4
          var expanded = items.length + ":" + (items[2] === null) + ":" + (items[3] === null)
          items.length = 1
          summary = expanded + "|after=" + items.length + ":" + items[0].objectName
        }
      }
      """
    When the method "resizeAndTruncate" of "root" is called
    Then the property "summary" of "root" is "4:true:true|after=1:a"
