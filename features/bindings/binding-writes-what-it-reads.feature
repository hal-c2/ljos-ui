# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-javascript-expressions.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A binding that writes a property it reads
  A binding may write, as it runs, a property it also reads. The write would run the binding
  again, and that run would write again. The binding runs once for each change, the write it
  made stays, and the loop is reported for the binding's property.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int n: 1
        property int m: { root.n = root.n + 1; return root.n }
        property int k: 0
        property int other: { root.k = root.n * 3; return root.n }
        property int same: { root.n = root.n; return root.n }
      }
      """

  Scenario: The binding runs once and keeps what it wrote
    Then the property "n" of "root" is 2
    And the property "m" of "root" is 2
    And the log has a warning matching "Binding loop detected for property .m."

  Scenario: A later change runs it once more
    When the property "n" of "root" is set to 10
    Then the property "n" of "root" is 11
    And the property "m" of "root" is 11
    And the log has a warning matching "Binding loop detected for property .m."

  Scenario: A write of a property the binding does not read is no loop
    Then the property "k" of "root" is 6
    And the property "other" of "root" is 2
    And the log has no warning matching "Binding loop detected for property .other."

  Scenario: A write of the value the property holds is no loop
    Then the property "same" of "root" is 2
    And the log has no warning matching "Binding loop detected for property .same."
