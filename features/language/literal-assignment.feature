# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-attributes
#   https://doc.qt.io/qt-6/qml-list.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A value written in the file is of its property's type
  A number, a string, `true`, `false` or `null` written as a property's
  value must be one the property can hold. One that cannot be is a mistake
  in the file, found when the file is read: the file does not load, and
  what is said names the place of the value and the kind it should be. A
  number written with a minus is written as it is; anything else, `(5)` or
  `+"3"`, is an expression, and what it gives is only known when it runs.

  Rule: A value of the wrong kind stops the file loading, and says where

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
      And the log has a warning matching "Bad\.qml:3:<column>: <said>"

      Examples:
        | written                    | column | said                                           |
        | height: "tall"             | 19     | Invalid property assignment: number expected   |
        | opacity: true              | 20     | Invalid property assignment: number expected   |
        | anchors.margins: "4"       | 28     | Invalid property assignment: number expected   |
        | property real r: null      | 28     | Invalid property assignment: number expected   |
        | property int n: "3"        | 27     | Invalid property assignment: int expected      |
        | property int n: 2.5        | 27     | Invalid property assignment: int expected      |
        | property int n: 2147483648 | 27     | Invalid property assignment: int expected      |
        | visible: "true"            | 20     | Invalid property assignment: boolean expected  |
        | property bool b: 1         | 28     | Invalid property assignment: boolean expected  |
        | objectName: 5              | 23     | Invalid property assignment: string expected   |
        | property string s: -5      | 30     | Invalid property assignment: string expected   |
        | property url u: true       | 27     | Invalid property assignment: url expected      |
        | property color c: 5        | 29     | Invalid property assignment: color expected    |
        | property point p: "1,2,3"  | 29     | Invalid property assignment: point expected    |
        | property size s: "1,2"     | 28     | Invalid property assignment: size expected     |
        | property rect r: 5         | 28     | Invalid property assignment: point expected    |
        | property date d: true      | 28     | Invalid property assignment: datetime expected |
        | property list<int> l: 5    | 33     | Cannot assign primitives to lists              |
        | children: "x"              | 21     | Cannot assign primitives to lists              |

    @qt-table
    Scenario Outline: <written> is refused: an object is expected
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
      And the log has a warning matching "Bad\.qml:3:<column>: Invalid property assignment: unsupported type .<class>\*."

      Examples:
        | written                  | column | class      |
        | property Item i: 5       | 28     | QQuickItem |
        | property QtObject o: "x" | 32     | QObject    |

  Rule: A value of the right kind, or one only known when it runs, loads

    Scenario Outline: <written> gives <read> as <value>
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "o"
          <written>
          property var got: <read>
        }
        """
      Then the property "got" of "o" is <value>

      Examples:
        | written                      | read                    | value       |
        | property int n: 1e2          | n                       | 100         |
        | property int n: -2147483648  | n                       | -2147483648 |
        | property int n: +"3"         | n                       | 3           |
        | property string s: +5        | s                       | "5"         |
        | property string s: `plain`   | s                       | "plain"     |
        | property url u: ""           | String(u)               | ""          |
        | property point p: " 1 , 2 "  | p.x + p.y               | 3           |
        | property size v: "1 x 2"     | v.width * 10 + v.height | 12          |
        | property rect r: "1,2,3x4.5" | r.height                | 4.5         |
        | property Item i: null        | i === null              | true        |
        | property list<int> l: [1, 2] | l.length                | 2           |
        | property bool b: undefined   | b                       | false       |
