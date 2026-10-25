# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html#property-change-signal-handlers
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings), bucket warning:TypeError(qt-only):Cannot_call_method_'_'_of_undefined.

@qt-parity
Feature: A binding that gives the default value while the object is made
  A binding made with the object runs its property's change handler only if it changes the
  value, except for a font or a colour: those tell the change even when the binding gives the
  value they had.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        property font f1
        property font f2: f1
        onF2Changed: log += "font;"
        property color c1
        property color c2: c1
        onC2Changed: log += "color;"
        property int i1
        property int i2: i1
        onI2Changed: log += "int;"
        property point q1
        property point q2: q1
        onQ2Changed: log += "point;"
        property size s1
        property size s2: s1
        onS2Changed: log += "size;"
        property rect r2: Qt.rect(0, 0, 0, 0)
        onR2Changed: log += "rect;"
        property string t2: ""
        onT2Changed: log += "string;"
      }
      """

  @backlog
  Scenario: A font or a colour bound to the value it had is told as changed
    Then the property "log" of "root" is "font;color;"
