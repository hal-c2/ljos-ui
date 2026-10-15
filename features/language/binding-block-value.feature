# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-javascript-expressions.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#gc-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The value of a binding written as a block
  A binding written between braces without `return` takes the value of the
  last expression statement it evaluated, and `gc()` is a global function.

  Scenario Outline: An if chain gives the statement it took
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int kind: <kind>
        property string name: {
          if (kind == 0) {
            "red";
          } else if (kind == 1) {
            "blue";
          } else {
            "yellow";
          }
        }
      }
      """
    Then the property "name" of "root" is "<name>"

    Examples:
      | kind | name   |
      | 0    | red    |
      | 1    | blue   |
      | 2    | yellow |

  Scenario: A block that ends in a plain statement gives it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: 3
        property int doubled: {
          var m = n * 2;
          m;
        }
      }
      """
    Then the property "doubled" of "root" is 6

  Scenario: gc() can be called and is a function
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool ok: { gc(); typeof gc === "function" }
      }
      """
    Then the property "ok" of "root" is true
