# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-basics.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A semicolon where QML's grammar has no place for one
  A semicolon ends a binding to a value, a property declaration and a signal declaration. An
  object's members are not a list of statements, so a semicolon after an object, a binding to
  an object, an object list, a function or an enumeration is a mistake, and so is one on its
  own. The file does not load, and the error is at the semicolon.

  Scenario Outline: A semicolon after <what> stops the file loading
    Given the file "Bad.qml":
      """qml
      import QtQuick
      Item {
        property Item a
        <member>
      }
      """
    Then loading "Bad.qml" is rejected

    Examples:
      | what                        | member                |
      | a binding to an object      | a: Item {};           |
      | a binding to an object list | data: [Item {}];      |
      | a child object              | Item {}; x: 1         |
      | a function                  | function f() {};      |
      | an enumeration              | enum E { A };         |
      | another semicolon           | x: 1;;                |
      | a script block              | x: { return 1 };      |

  Scenario: The error is at the semicolon
    Given the file "Bad.qml":
      """qml
      import QtQuick
      ListView {
        model: 2
        delegate: Item {};
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item { Loader { objectName: "l"; source: "Bad.qml" } }
      """
    Then the property "status" of "l" is 3
    And the log has a warning matching "Bad\.qml:4:\d+: Unexpected token"

  Scenario: Declarations and bindings to values take a semicolon
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property Item a: Item { objectName: "a" };
        property list<Item> b: [Item {}];
        signal s;
        x: 1; y: 2;
        property string made: a.objectName + b.length + x + y
      }
      """
    Then the property "made" of "root" is "a112"
