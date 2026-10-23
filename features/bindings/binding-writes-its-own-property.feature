# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A binding that writes its own property
  A binding that gives its own property a value as it runs is replaced by that value, as by any
  other write: the property ends at the value written, not at the binding's result, and
  nothing is reported.

  Scenario: The property ends at the value the binding wrote
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: { n = 5; return 3 }
      }
      """
    Then the property "n" of "root" is 5
    And the log has no warning matching "Binding loop"
