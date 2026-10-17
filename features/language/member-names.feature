# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-attributes
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#signal-attributes
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#method-attributes
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The names a property, a signal or a method may have
  A property, signal or method an object declares is named with a lower
  case letter first, and not with a name of the script environment's own:
  `print`, `console`, `Math`, `undefined` and the like. Either mistake stops
  the file loading, and what is said is at the name. A method or a signal
  is told first of its capital letter; a property first of its name.

  Rule: A name that is not allowed stops the file loading, and says where

    Scenario Outline: <written> is refused: <said>
      Given the file "Bad.qml":
        """qml
        import QtQuick
        Item {
          <written>
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { objectName: "loader"; source: "Bad.qml" }
        }
        """
      Then the property "status" of "loader" is 3
      And the log has a warning matching "Bad\.qml:3:<column>: <said>$"

      Examples:
        | written                         | column | said                                                  |
        | function print() {}             | 20     | Illegal method name                                   |
        | function console() {}           | 20     | Illegal method name                                   |
        | function undefined() {}         | 20     | Illegal method name                                   |
        | function qsTr(s) { return s }   | 20     | Illegal method name                                   |
        | function parseInt() {}          | 20     | Illegal method name                                   |
        | function Foo() {}               | 20     | Method names cannot begin with an upper case letter   |
        | function Math() {}              | 20     | Method names cannot begin with an upper case letter   |
        | signal print()                  | 18     | Illegal signal name                                   |
        | signal gc(int n)                | 18     | Illegal signal name                                   |
        | signal Foo()                    | 18     | Signal names cannot begin with an upper case letter   |
        | signal Qt()                     | 18     | Signal names cannot begin with an upper case letter   |
        | property int print              | 24     | Illegal property name                                 |
        | property var Math               | 24     | Illegal property name                                 |
        | property int Qt                 | 24     | Illegal property name                                 |
        | property real NaN               | 25     | Illegal property name                                 |
        | property var XMLHttpRequest     | 24     | Illegal property name                                 |
        | readonly property int eval: 1   | 33     | Illegal property name                                 |
        | required property int isNaN     | 33     | Illegal property name                                 |
        | property int Foo                | 24     | Property names cannot begin with an upper case letter |
        | property var Component          | 24     | Property names cannot begin with an upper case letter |

  Rule: A name the environment does not have is any object's to use

    Scenario: Names close to the environment's own load
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int require: 1
          property int globalThis: 2
          property int _hidden: 3
          property int $cash: 4
          signal arguments()
          function toString() { return "root" }
          function prints() { return require + globalThis + _hidden + $cash }
          property int sum: prints()
        }
        """
      Then the property "sum" of "root" is 10
      And the log has no warnings
