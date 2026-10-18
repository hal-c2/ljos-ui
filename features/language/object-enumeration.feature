# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   qml6, run as a black box (for…in over a Timer)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Listing an object's members
  `for…in` over an object of a QML type lists what its type has: the
  properties first, those of the base type before those the type adds,
  then its signals, again a base's first.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Timer { id: t; property int extra: 1; function hello() {} }
        function names(o) { var k = []; for (var n in o) k.push(n); return k }
        function before(k, a, b) { return k.indexOf(a) >= 0 && k.indexOf(a) < k.indexOf(b) }
        property var listed: names(t)
        property string first: listed[0]
        property bool baseFirst: before(listed, "interval", "extra")
        property bool propsFirst: before(listed, "extra", "triggered")
        property bool signalsBaseFirst: before(listed, "triggered", "extraChanged")
      }
      """

  Scenario: A base type's properties are listed before the type's own
    Then the property "first" of "root" is "objectName"
    And the property "baseFirst" of "root" is true

  Scenario: The properties are listed before the signals
    Then the property "propsFirst" of "root" is true
    And the property "signalsBaseFirst" of "root" is true
