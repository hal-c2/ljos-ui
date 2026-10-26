# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-functionlist.html
#   https://tc39.es/ecma262/#sec-math.hypot
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (connections, bindings), buckets value:step:text and value:load:text-rounding:
# Math.hypot an ulp from Qt's, which a number written out shows.
# Of three numbers or more, Qt's is still an ulp from Ljós's now and then (10 of 254 tried);
# no order of the sum found matches each.

@qt-parity
Feature: Math.hypot
  Math.hypot divides each number by the largest and sums the squares in order, so a number
  written out is the one Qt writes.

  Scenario Outline: Math.hypot(<args>) is <hypot>
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen: String(Math.hypot(<args>))
      }
      """
    Then the property "seen" of "root" is "<hypot>"

    Examples:
      | args                    | hypot              |
      | 0.1, 100                | 100.00004999998751 |
      | 0.001, 7                | 7.0000000714285715 |
      | 3, 4                    | 5                  |
      | -3, 4                   | 5                  |
      | 1.5, 2.5, 3.5           | 4.55521678957215   |
      | 1e200, 1e200            | 1.414213562373095e+200 |
      | 1e-200, 3e-200          | 3.1622776601683794e-200 |
      | 0, -0                   | 0                  |
      |                         | 0                  |
      | -7                      | 7                  |
      | NaN, 1                  | NaN                |
      | NaN, -Infinity          | Infinity           |
      | "3", [4]                | 5                  |

  Scenario: Math.hypot takes two numbers by its length, and reads each it is given
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string log
        property string seen: {
          var h = Math.hypot({ valueOf: function () { log += "a"; return NaN } },
                             { valueOf: function () { log += "b"; return Infinity } })
          return Math.hypot.length + "," + h + "," + log
        }
      }
      """
    Then the property "seen" of "root" is "2,Infinity,ab"
