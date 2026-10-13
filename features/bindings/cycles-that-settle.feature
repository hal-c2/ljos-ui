# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Cycles of bindings that settle
  Bindings may read each other round a cycle. A change that goes round
  and comes back as the value the binding already has is no loop; only a
  change that comes back changed is reported, for the binding it came
  back to.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int k: 0
        property int a: Math.max(b, k)
        property int b: Math.min(a, 0)
        property int c: Math.max(d, k)
        property int d: Math.min(c, 2)
      }
      """

  Scenario: A change that comes back unchanged is no loop
    When the property "k" of "root" is set to 3
    Then the property "a" of "root" is 3
    And the property "b" of "root" is 0
    And the log has no warning matching "Binding loop detected for property .[ab]."

  Scenario: A change that comes back changed is a loop, reported where it started
    When the property "k" of "root" is set to 3
    Then the property "c" of "root" is 3
    And the property "d" of "root" is 2
    And the log has a warning matching "Binding loop detected for property .c."
    And the log has no warning matching "Binding loop detected for property .d."

  Scenario: Nothing is reported while the cycles are made
    Then the property "a" of "root" is 0
    And the property "d" of "root" is 0
    And the log has no warnings
