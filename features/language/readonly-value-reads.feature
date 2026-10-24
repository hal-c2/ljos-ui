# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#read-only-properties
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A binding reads a read-only property's value
  A read-only property given a value where it is declared keeps that value, and a
  binding that reads it has it. createObject can give it another value as the object
  is made; the object's bindings have that one.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        readonly property int r: 3
        readonly property color c: "red"
        property int twice: r * 2
        property string shade: c.toString()
        property int made: -1
        Component { id: kid; Item { objectName: "kid"; readonly property int r: 2; property int d: r * 10 } }
        function create(props) { made = kid.createObject(root, props).d }
      }
      """

  Scenario: A binding has the value it was declared with
    Then the property "twice" of "root" is 6
    And the property "shade" of "root" is "#ff0000"

  Scenario: A binding of an object createObject makes has the value it was given
    When the method "create" of "root" is called with {"r": 7}
    Then the property "made" of "root" is 70
    And the property "d" of "root/kid" is 70

  Scenario: Without a value given the binding has the declared one
    When the method "create" of "root" is called with {}
    Then the property "d" of "root/kid" is 20
