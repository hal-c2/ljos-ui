# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-javascript-expressions.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Binding loops
  Two bindings that read each other are reported instead of looping forever.

  Scenario: A binding loop is reported in the log
    Given the file "Loop.qml":
      """qml
      import QtQuick
      Item {
        property int p: q + 1
        property int q: p + 1
      }
      """
    When "Loop.qml" is loaded
    Then the log has a warning matching "Binding loop detected"

  Scenario: An acyclic binding chain produces no warning
    Given the file "Chain.qml":
      """qml
      import QtQuick
      Item { property int p: 1; property int q: p + 1; property int r: q + 1 }
      """
    When "Chain.qml" is loaded
    Then the log has no warnings

  Scenario: A file with a syntax error is rejected
    Given the file "Broken.qml":
      """qml
      import QtQuick
      Item { property int p: }
      """
    Then loading "Broken.qml" is rejected
