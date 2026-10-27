# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
#   https://doc.qt.io/qt-6/qml-point.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#resolvedUrl-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A value held in a variable in a file that resolves addresses
  A file that resolves an address against itself, or makes a component or object from one, still
  writes a member of a value it read into a variable back to where the value was read from.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property point spot: Qt.point(1, 2)
        property string out
        function point() { const p = spot; p.x = 5; out = p + "," + spot + "," + Qt.resolvedUrl("a.qml").toString().endsWith("/a.qml") }
      }
      """

  Scenario: Writing a member of a point held in a constant writes the property
    When the method "point" of "root" is called
    Then the property "out" of "root" is "QPointF(5, 2),QPointF(5, 2),true"
