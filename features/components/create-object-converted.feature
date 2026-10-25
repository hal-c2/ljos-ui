# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qml-color.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (creation), buckets value:load-handlers:bool, value:step:bool,
# value:step:number, value:step:text-number, value:load-handlers:invalid-qt,
# value:load-handlers:nan-ljos, value:onChanged+onCompleted:empty-qt and
# warning:other(qt-only):Could_not_set_initial_property_pN.

@qt-parity
Feature: What createObject makes of its initial properties
  An initial property is converted to the property's type as a value from outside QML is:
  a string "0" is false, a number is rounded to an int, a bool too, and what is no colour is an invalid
  one. One that cannot be converted at all is told, and the property loses its binding and
  keeps its type's default.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int n: 3
        property string seen: ""
        property string log: ""
        Component {
          id: typed
          QtObject {
            property bool b: true
            property int i
            property color c: "red"
            property real r: root.n * 2
            onRChanged: root.log += "r;"
          }
        }
        function give(props) {
          var o = typed.createObject(root, props)
          seen = [o.b, o.i, o.c.valid, o.r].join(",")
        }
        function giveNaN() { give({i: NaN}) }
      }
      """

  Scenario Outline: An initial <type> given <given> is converted
    When the method "give" of "root" is called with {"<prop>": <given>}
    Then the property "seen" of "root" is "<seen>"
    And the log has no warning matching "Could not set initial property"

    Examples:
      | type   | prop | given   | seen           |
      | bool   | b    | "0"     | false,0,true,6 |
      | bool   | b    | 0.1     | false,0,true,6 |
      | bool   | b    | "false" | false,0,true,6 |
      | int    | i    | "2.5"   | true,3,true,6  |
      | int    | i    | 2.5     | true,3,true,6  |
      | int    | i    | -2.5    | true,-3,true,6 |
      | colour | c    | false   | true,0,false,6 |
      | colour | c    | 3       | true,0,false,6 |

  Scenario: An initial int given NaN is told, and stays 0
    When the method "giveNaN" of "root" is called
    Then the property "seen" of "root" is "true,0,true,6"
    And the log has a warning matching "Could not set initial property i"

  Scenario: An initial property that cannot be converted leaves its type's default
    When the method "give" of "root" is called with {"r": ""}
    Then the property "seen" of "root" is "true,0,true,0"
    And the log has a warning matching "Could not set initial property r"
    When the property "n" of "root" is set to 4
    Then the property "log" of "root" is ""
