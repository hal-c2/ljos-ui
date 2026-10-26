# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html#property-change-signal-handlers
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (states), buckets warning:binding-loop(qt-only):load (a state change of a
# NaN that reads a binding of itself) and value:onChanged:text, seed 101 (a NaN bound again
# through Qt.binding) (see crates/ljos_fuzz/known/states.txt).
# Found again by ljos-fuzz (connections), seed 303, bucket value:onChanged:text (a NaN real
# given itself).

@qt-parity
Feature: NaN given to a real that is NaN
  NaN is not equal to itself, so a real that is NaN and is given NaN again is told as a change,
  and a binding that gives it NaN through itself is a loop.

  Scenario: Each NaN given to a NaN is told as a change
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real a: NaN
        property real c
        property string log
        onAChanged: log += "a;"
        onCChanged: log += "c;"
        function run() { log = ""; a = NaN; c = NaN; log += "|"; c = NaN }
      }
      """
    When the method "run" of "root" is called
    Then the property "log" of "root" is "a;c;|c;"

  Scenario: A state change that gives a NaN NaN through a binding of it is a loop
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real a: NaN
        property real b: a
        states: [State { name: "s0"; PropertyChanges { target: root; a: root.b - 1 } }]
        function run() { state = "s0" }
      }
      """
    When the method "run" of "root" is called
    Then the log has a warning matching "Binding loop detected for property .a."

  Scenario: A NaN bound again through Qt.binding is told as a change
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property real p0: parseInt("", 16)
        property real p1: root.p0
        onP1Changed: log += "p1=" + p1 + ";"
        function run() { log += "|"; p1 = Qt.binding(() => root.p0) }
      }
      """
    When the method "run" of "root" is called
    Then the property "log" of "root" is "p1=NaN;|p1=NaN;"
