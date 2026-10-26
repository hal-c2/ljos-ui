# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-expressions.html
#   https://doc.qt.io/qt-6/qml-date.html
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A function or a Date given to a property of another type
  Only a var holds a function: a binding that gives one to a property of any other type is
  warned about and leaves the property as it was, though a bool takes it as true. A Date is
  its time to a real, true to a bool and an invalid colour to a colour, without a word; an
  int, point, size, rect, vector, font or url refuses it by Qt's name for it, QDateTime, and
  keeps its empty value. A script's write is refused by the same names, and throws.

  Scenario Outline: A <type> bound to a function keeps what it had and is warned about
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        function pass(v) { return v }
        property bool on: false
        property <type> v: on ? pass(function () { return 1 }) : <first>
        property string seen: String(v)
      }
      """
    When the property "on" of "root" is set to true
    Then the property "seen" of "root" is "<seen>"
    And the log has a warning matching "Unable to assign a function to a property of any type other than var."

    Examples:
      | type   | first           | seen               |
      | real   | 7               | 7                  |
      | int    | 7               | 7                  |
      | string | "a"             | a                  |
      | color  | "red"           | #ff0000            |
      | point  | Qt.point(1, 2)  | QPointF(1, 2)      |
      | rect   | Qt.rect(1, 2, 3, 4) | QRectF(1, 2, 3, 4) |

  Scenario: A bool bound to a function is true
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        function pass(v) { return v }
        property bool v: pass(function () { return 0 })
      }
      """
    Then the property "v" of "root" is true
    And the log has no warning matching "assign"

  Scenario Outline: A <type> given a Date reads <seen>
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        function pass(v) { return v }
        property <type> v: pass(new Date(5000))
        property string seen: String(<read>)
      }
      """
    Then the property "seen" of "root" is "<seen>"
    And the log has no warning matching "assign"

    Examples:
      | type       | read    | seen  |
      | real       | v       | 5000  |
      | bool       | v       | true  |
      | color      | v.valid | false |
      | list<real> | v       | 0     |

  Scenario Outline: A <type> given a Date is its empty value and warned about
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        function pass(v) { return v }
        property <type> v: pass(new Date(5000))
        property string seen: String(<read>)
      }
      """
    Then the property "seen" of "root" is "<seen>"
    And the log has a warning matching "Unable to assign QDateTime to <qt>"

    Examples:
      | type     | read   | seen               | qt        |
      | int      | v      | 0                  | int       |
      | point    | v      | QPointF(0, 0)      | QPointF   |
      | size     | v      | QSizeF(-1, -1)     | QSizeF    |
      | rect     | v      | QRectF(0, 0, 0, 0) | QRectF    |
      | vector3d | v      | QVector3D(0, 0, 0) | QVector3D |
      | font     | v.bold | false              | QFont     |
      | url      | v      |                    | QUrl      |

  Scenario: A script's write of a function or a Date is refused by the same names
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int i: 3
        property real r
        property point p: Qt.point(1, 2)
        property string seen
        function t(write) { try { write(); return "ok" } catch (e) { return e.message } }
        function write() {
          seen = [t(() => i = new Date(5000)), t(() => p = new Date(5000)), t(() => r = new Date(5000)),
                  t(() => p = function () {}), t(() => r = function () {}), i, p.x, r].join(";")
        }
      }
      """
    When the method "write" of "root" is called
    Then the property "seen" of "root" is "Cannot assign QDateTime to int;Cannot assign QDateTime to QPointF;ok;Cannot assign JavaScript function to QPointF;Cannot assign JavaScript function to double;3;1;5000"
