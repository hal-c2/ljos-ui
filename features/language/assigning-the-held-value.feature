# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: A script's assignment of the value a property holds
  A handler often sets what it set last time, as a clock's does each
  tick. The property's change signal is not emitted for a value it holds
  already; an assignment still ends a binding, whatever it writes.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int base: 3
        property int held: 0
        property int bound: base
        property int changes: 0
        onHeldChanged: changes++
        function run() { held = 7; held = 7; held = 7 }
        function same() { bound = 3 }
      }
      """

  @qt-parity
  Scenario: The value a property holds again is no change
    When the method "run" of "root" is called
    And the method "run" of "root" is called
    Then the property "held" of "root" is 7
    And the property "changes" of "root" is 1

  @qt-parity
  Scenario: The value a binding gives, assigned, still ends the binding
    When the method "same" of "root" is called
    And the property "base" of "root" is set to 4
    Then the property "bound" of "root" is 3
