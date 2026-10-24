# Sources:
#   https://doc.qt.io/qt-6/qtquicklayouts-overview.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A layout settles when a child's size follows the room it gets
  A child whose implicit size grows with the size the layout gives it (a
  Column whose last item fills what is left of it) asks for more each time
  it is laid out. The layout lays it out again until the sizes agree,
  within the frame and without a warning.

  @api-QtQuick.Layouts.Layout.fillHeight @api-QtQuick.Item.implicitHeight
  Scenario: A child that asks for the height it was given takes the room left
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      ColumnLayout {
        width: 100; height: 400; spacing: 0
        Item { objectName: "fixed"; Layout.fillHeight: true; Layout.preferredHeight: 100 }
        Item { objectName: "follows"; Layout.fillHeight: true; implicitHeight: height }
      }
      """
    When 100 milliseconds pass
    Then the property "height" of "fixed" is 100
    And the property "height" of "follows" is 300
    And the log has no warnings
