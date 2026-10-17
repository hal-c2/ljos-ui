# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#incubateObject-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#binding-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Qt.binding for a member of a value in the initial properties
  The initial properties of createObject and incubateObject may name a member of a value type
  property, as "font.pixelSize" or "p.x", and give it a Qt.binding. Neither refuses it.
  createObject sets the member to what the binding gives when the object is made, and it does
  not follow afterwards; incubateObject makes it a binding that follows, until a script writes
  that member.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int size: 15
        Component { id: c; Text { objectName: "made"; property point p } }
        function bound(f) { return Qt.binding(function () { return f() }) }
        function created() {
          c.createObject(root, { "font.pixelSize": bound(() => size), "p.x": bound(() => size + 1) })
        }
        function incubated() {
          made = c.incubateObject(root, { "font.pixelSize": bound(() => size), "p.x": bound(() => size + 1) },
            Qt.Synchronous).object
        }
        property var made
        function writeX() { made.p.x = 3 }
      }
      """

  Scenario Outline: <how> sets the members from the bindings
    When the method "<method>" of "root" is called
    Then the property "font.pixelSize" of "root/made" is 15
    And the property "p.x" of "root/made" is 16
    And the log has no warnings

    Examples:
      | how            | method    |
      | createObject   | created   |
      | incubateObject | incubated |

  Scenario Outline: <how> <does> what the bindings read
    When the method "<method>" of "root" is called
    And the property "size" of "root" is set to 20
    Then the property "font.pixelSize" of "root/made" is <pixelSize>
    And the property "p.x" of "root/made" is <x>

    Examples:
      | how            | does               | method    | pixelSize | x  |
      | createObject   | does not follow    | created   | 15        | 16 |
      | incubateObject | follows            | incubated | 20        | 21 |

  Scenario: Writing one member ends its binding and not the others
    When the method "incubated" of "root" is called
    And the method "writeX" of "root" is called
    And the property "size" of "root" is set to 20
    Then the property "font.pixelSize" of "root/made" is 20
    And the property "p.x" of "root/made" is 3
