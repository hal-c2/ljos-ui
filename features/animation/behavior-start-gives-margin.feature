# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html#leftMargin-attached-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A Behavior's start gives a layout margin a value of its own
  A side margin of a cell that was never given a value is `Layout.margins`, and moves with it.
  A Behavior on the side starts its animation at the margin the side has, and that start gives
  the side that margin as its own: from then on `Layout.margins` no longer moves it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"
        property string seen
        RowLayout {
          Rectangle {
            id: r; implicitWidth: 10; implicitHeight: 10
            Layout.margins: 5
            Behavior on Layout.leftMargin { NumberAnimation { duration: 1000 } }
          }
          Rectangle { id: s; implicitWidth: 10; implicitHeight: 10; Layout.margins: 5 }
        }
        function give() {
          seen = r.Layout.leftMargin + " " + s.Layout.leftMargin
          r.Layout.leftMargin = 100
          r.Layout.margins = 8
          s.Layout.margins = 8
          seen += " " + r.Layout.leftMargin + " " + s.Layout.leftMargin
        }
      }
      """

  Scenario: The side keeps the margin its animation started from
    When the method "give" of "root" is called
    Then the property "seen" of "root" is "5 5 5 8"
