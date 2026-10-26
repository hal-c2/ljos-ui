# Sources:
#   https://doc.qt.io/qt-6/qml-int.html
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings), buckets value:int:url-string,
# warning:assign(qt-only):url-string, warning:assign(qt-only):color-string,
# warning:assign(qt-only):stringify-array and throws:Error/none:step-set(reduce); and by
# ljos-fuzz (states), seeds 101 and 202, bucket warning:assign(ljos-only):Unable_to_assign_QString_to_int.

@qt-parity
Feature: A string given to an int property
  An `int` property does not read a number out of a string. A binding that gives it a string
  is reported and leaves the property at 0; a script that assigns a string is thrown an error,
  unless the string is a whole number. A `nan` or `inf`, as a double cast to an int, is the least
  int.

  Scenario Outline: An int bound to the string <value> says it cannot take it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: String(<value>)
      }
      """
    Then the property "n" of "root" is 0
    And the log has a warning matching "Scenario.qml:4:\d+: Unable to assign QString to int$"

    Examples:
      | value      |
      | "0x1F"     |
      | "#000000"  |
      | "a.png"    |
      | "Infinity" |
      | "-NaN"     |
      | "1e400"    |

  Scenario Outline: An int bound to the string <value> is the least int, unwarned
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: String(<value>)
      }
      """
    Then the property "n" of "root" is -2147483648
    And the log has no warnings

    Examples:
      | value   |
      | "NaN"   |
      | "nan"   |
      | " NaN " |
      | "inf"   |
      | "-INF"  |

  Scenario: A script that gives an int a string that is not a number is thrown an error
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: 3
        property string seen: ""
        function give() {
          try { n = ["a", "b"].reduce(function (a, b) { return a + b }, 0) } catch (e) { seen = e.message }
        }
      }
      """
    When the method "give" of "root" is called
    Then the property "seen" of "root" is "Cannot assign QString to int"
    And the property "n" of "root" is 3

  Scenario: A script that gives an int a whole number as a string is obeyed
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: 3
        function give() { n = "7" }
      }
      """
    When the method "give" of "root" is called
    Then the property "n" of "root" is 7

  # Found by ljos-fuzz (bindings), bucket value:int:reduce.
  Scenario Outline: A number with a half given to an int as a string is rounded up
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: (<whole> + ".5")
      }
      """
    Then the property "n" of "root" is <n>
    And the log has no warning matching "(?i)assign"

    Examples:
      | whole | n |
      | "3"   | 4 |
      | "03"  | 4 |
      | "2"   | 3 |

  # Found by ljos-fuzz (bindings), bucket value:step-readonly:text-int-range.
  Scenario Outline: A script that gives an int the string <value>, which is out of its range, gives the least int
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: 3
        function give() { n = <value> }
      }
      """
    When the method "give" of "root" is called
    Then the property "n" of "root" is -2147483648

    Examples:
      | value         |
      | "3000000000"  |
      | "-3000000000" |
