# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#grouped-properties
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#attached-properties-and-attached-signal-handlers
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Dotted names among the properties given to Component.createObject
  A name among the initial properties of createObject may be a path, as it may be in the
  declaration: `"font.pixelSize"` is a member of a value, `"border.width"` a property of a group,
  and `"Accessible.name"` or `"Layout.fillWidth"` a property of what a type attaches to the new
  object.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      import QtQuick.Layouts
      Item {
        id: root
        objectName: "root"
        property var got; property var other
        Component { id: text; Text { text: "x"; font.italic: true } }
        Component { id: box; Rectangle { } }
        Component { id: button; Button { text: "b" } }
        Component { id: plain; Item { } }
        ColumnLayout { id: column }
        function sized() {
          const o = text.createObject(root, { "font.pixelSize": 31 })
          got = o.font.pixelSize
          other = o.font.italic
        }
        function bordered() { got = box.createObject(root, { "border.width": 4 }).border.width }
        function named() {
          got = button.createObject(root, { "Accessible.name": "Open panel" }).Accessible.name
        }
        function filling() {
          got = plain.createObject(column, { "Layout.fillWidth": true }).Layout.fillWidth
        }
      }
      """

  Rule: A dotted name reaches what the path names

    Scenario: A member of a value property
      When the method "sized" of "root" is called
      Then the property "got" of "root" is 31
      And the property "other" of "root" is true

    Scenario: A property of a group
      When the method "bordered" of "root" is called
      Then the property "got" of "root" is 4

    Scenario: A property attached by Accessible
      When the method "named" of "root" is called
      Then the property "got" of "root" is "Open panel"

    Scenario: A property attached by Layout
      When the method "filling" of "root" is called
      Then the property "got" of "root" is true
