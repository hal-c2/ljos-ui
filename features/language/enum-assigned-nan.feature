# Sources:
#   https://doc.qt.io/qt-6/qml-enumeration.html
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#horizontalAlignment-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# Written while triaging ljos-fuzz (bindings) buckets
# warning:assign(qt-only):Cannot_assign_double_to_QQuickText::HAlignment and …::WrapMode, where the
# Error is thrown from a handler that Ljós does not run (see crates/ljos_fuzz/known/bindings.txt).

@qt-parity
Feature: A script giving an enumeration NaN
  A script that assigns NaN to a property that holds an enumeration is thrown an Error that
  says it cannot assign a double to the enumeration, and the property keeps its value. A
  fraction is rounded.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string seen
        Text { id: label; horizontalAlignment: Text.AlignRight }
        function give(v) {
          try { label.horizontalAlignment = v; seen = "taken " + label.horizontalAlignment }
          catch (e) { seen = e.name + " " + /^Cannot assign double to \S*HAlignment$/.test(e.message) + " " + label.horizontalAlignment }
        }
        function giveNaN() { give(Number("Hello")) }
      }
      """

  Scenario: NaN is thrown an Error
    When the method "giveNaN" of "root" is called
    Then the property "seen" of "root" is "Error true 2"

  Scenario: A fraction is rounded
    When the method "give" of "root" is called with 0.6
    Then the property "seen" of "root" is "taken 1"
