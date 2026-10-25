# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#incubateObject-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
#   https://doc.qt.io/qt-6/qml-color.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What incubateObject makes of its initial properties
  incubateObject converts an initial property as a script's assignment does, not as
  createObject does: a bool is what the value is as a condition, a number is cut to an
  int, a real is what the value is as a number, and what is no colour is an invalid one.
  A text for an int is rounded, and one that is no number is told as an error of the call
  and leaves the property as it was. A list for a text is a character for each item.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int n: 3
        property string seen: ""
        Component {
          id: typed
          QtObject {
            property bool b: true
            property int i: 7
            property real r: root.n * 2
            property string s: "x"
            property color c: "red"
          }
        }
        function give(props) {
          const o = typed.incubateObject(root, props, Qt.Synchronous).object
          seen = [o.b, o.i, o.r, o.s, o.c.valid].join(",")
          n = 4
          seen += ";" + o.r
        }
      }
      """

  Scenario Outline: An initial <type> given <given> is converted
    When the method "give" of "root" is called with {"<prop>": <given>}
    Then the property "seen" of "root" is "<seen>"
    And the log has no warning matching "Cannot assign"

    Examples:
      | type   | prop | given         | seen                        |
      | bool   | b    | "0"           | true,7,6,x,true;8           |
      | bool   | b    | "false"       | true,7,6,x,true;8           |
      | bool   | b    | ""            | false,7,6,x,true;8          |
      | bool   | b    | 0.1           | true,7,6,x,true;8           |
      | bool   | b    | null          | false,7,6,x,true;8          |
      | int    | i    | 2.5           | true,2,6,x,true;8           |
      | int    | i    | -2.5          | true,-2,6,x,true;8          |
      | int    | i    | 1e20          | true,1661992960,6,x,true;8  |
      | int    | i    | "2.5"         | true,3,6,x,true;8           |
      | int    | i    | "-2.5"        | true,-3,6,x,true;8          |
      | int    | i    | " 1e3 "       | true,1000,6,x,true;8        |
      | int    | i    | "99999999999" | true,-2147483648,6,x,true;8 |
      | int    | i    | true          | true,1,6,x,true;8           |
      | real   | r    | "abc"         | true,7,NaN,x,true;NaN       |
      | real   | r    | ""            | true,7,0,x,true;0           |
      | real   | r    | "0x10"        | true,7,16,x,true;16         |
      | real   | r    | null          | true,7,0,x,true;0           |
      | real   | r    | {}            | true,7,NaN,x,true;NaN       |
      | real   | r    | [5]           | true,7,5,x,true;5           |
      | text   | s    | [72, 105]     | true,7,6,Hi,true;8          |
      | text   | s    | ["ab", "c"]   | true,7,6,ac,true;8          |
      | colour | c    | 3             | true,7,6,x,false;8          |
      | colour | c    | true          | true,7,6,x,false;8          |
      | colour | c    | null          | true,7,6,x,false;8          |
      | colour | c    | {}            | true,7,6,x,false;8          |
      | colour | c    | "nope"        | true,7,6,x,false;8          |

  Scenario Outline: An initial <type> that cannot be converted is an error of the call
    When the method "give" of "root" is called with {"<prop>": <given>}
    Then the property "seen" of "root" is "<seen>"
    And the log has a warning matching "Error: Cannot assign <what> to <to>"

    Examples:
      | type | prop | given  | seen              | what           | to      |
      | int  | i    | "x"    | true,7,6,x,true;8 | QString        | int     |
      | int  | i    | "0x10" | true,7,6,x,true;8 | QString        | int     |
      | int  | i    | null   | true,7,6,x,true;8 | std::nullptr_t | int     |
      | text | s    | {}     | true,7,6,x,true;8 | QJSValue       | QString |
