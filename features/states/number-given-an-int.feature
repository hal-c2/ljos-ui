# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-binding.html#value-prop
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
#   https://doc.qt.io/qt-6/qml-int.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (connections), seed 303, bucket value:load:number.

@qt-parity
Feature: A number a Binding or a state's plain value gives an int
  A Binding's value and a state's plain value are held as variants, and a number held so is
  rounded half away from 0 when an int takes it, where an assignment or a binding truncates.
  Past the range of a 64-bit int it is the nearest such, cut to its low 32 bits; NaN gives
  nothing, and the int keeps what it had.

  Scenario: Each way of giving an int 0.5
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real h: 0.5
        property int a; property int b; property int c; property int d; property int e
        property int f; property int g: h; property int i
        property var hv: 2.5
        property string seen
        Binding { target: root; property: "a"; value: root.h }
        Binding on b { value: 0.5 }
        Binding { root.c: root.h }
        states: State {
          name: "s"
          PropertyChanges { target: root; d: 0.5; e: root.h }
          PropertyChanges { target: root; explicit: true; i: root.h }
        }
        function run() { state = "s"; f = hv; seen = [a, b, c, d, e, f, g, i].join(",") }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "1,1,0,1,0,2,0,1"

  Scenario Outline: A Binding's value of <n> gives an int <int>
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real h: 7
        property int a
        Binding { target: root; property: "a"; value: root.h }
        function run() { h = <n> }
      }
      """
    When the method "run" of "root" is called
    Then the property "a" of "root" is <int>
    And the log has no warnings

    Examples:
      | n                   | int         |
      | -0.5                | -1          |
      | 2.5                 | 3           |
      | -5.5                | -6          |
      | 0.49999999999999994 | 0           |
      | 2147483647.5        | -2147483648 |
      | -2147483648.5       | 2147483647  |
      | 1e10                | 1410065408  |
      | 9.2e18              | -1650982912 |
      | 9.3e18              | -1          |
      | Infinity            | -1          |
      | -Infinity           | 0           |
      | NaN                 | 7           |

  Scenario: A state's plain number is rounded, and its NaN, a binding, gives 0
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int a: 4
        property int b: 4
        states: State { name: "s"; PropertyChanges { target: root; a: NaN; b: -2.5 } }
      }
      """
    When the property "state" of "root" is set to "s"
    Then the property "a" of "root" is 0
    And the property "b" of "root" is -3
    And the log has no warnings
