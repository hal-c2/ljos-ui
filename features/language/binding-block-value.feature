# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-javascript-expressions.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#gc-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The value of a binding written as a block
  A binding written between braces without `return` takes the value of the
  last expression statement it evaluated, and `gc()` is a global function.
  Braces are a block unless they start as JSON does, with a string or a
  number and a colon: then they are an object, and every key is a string or
  a number. So `{}` is an empty block, and `{ k: 1 }` a statement with a label.

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

  Scenario Outline: Braces <value> are a block
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var p: <value>
        property string seen: typeof p + " " + String(JSON.stringify(p)).replace(/"/g, "'")
      }
      """
    Then the property "seen" of "root" is "<seen>"

    Examples:
      | value           | seen                |
      | {}              | undefined undefined |
      | { k: 1 }        | number 1            |
      | { k: 1; }       | number 1            |
      | { k: "s" }      | string 's'          |
      | { k: {} }       | undefined undefined |
      | { k: { j: 2 } } | number 2            |

  Scenario Outline: Braces that start with a string or a number and a colon are an object
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var p: <value>
        property string seen: JSON.stringify(p).replace(/"/g, "'")
      }
      """
    Then the property "seen" of "root" is "<seen>"

    Examples:
      | value                  | seen              |
      | { "k": 1 }             | {'k':1}           |
      | { 1: 2 }               | {'1':2}           |
      | { "k": 1, 2: 3, }      | {'2':3,'k':1}     |
      | { "k": [1, { b: 2 }] } | {'k':[1,{'b':2}]} |

  Scenario Outline: Braces <value> are neither, and the file does not load
    Given the file "Bad.qml":
      """qml
      import QtQuick
      Item {
        property var p: <value>
      }
      """
    Then loading "Bad.qml" is rejected

    # A colon after `k: 1, j`; a second key that is a name; anything after the object.
    Examples:
      | value              |
      | { k: 1, j: 2 }     |
      | { "k": 1, j: 2 }   |
      | { "k": 1 }.k       |
