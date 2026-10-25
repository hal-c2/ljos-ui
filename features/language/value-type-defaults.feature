# Sources:
#   https://doc.qt.io/qt-6/qml-size.html
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings), buckets value:load-member-alias:nan-qt and value:step:int-range.

@qt-parity
Feature: The value a value-type property starts with
  A value-type property declared without a value holds its type's empty value. For a size
  that is an invalid size, -1 by -1, not 0 by 0.

  Scenario: A size declared without a value is -1 by -1
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property size s
        property string seen: s.width + " " + s.height
      }
      """
    Then the property "seen" of "root" is "-1 -1"
