# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Hiding a parent with many children
  Each child of a hidden item reads not visible. Telling the children is
  one change after another, never one inside the next, so it works for any
  number of children.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool open: true
        Column {
          objectName: "panel"
          visible: root.open
          Repeater {
            model: 2000
            Text { objectName: "row" + index; text: "Row " + (index + 1) }
          }
        }
      }
      """

  Scenario: Every child of a hidden parent reads not visible
    When the property "open" of "root" is set to false
    Then the property "visible" of "panel/row1999" is false
    And the property "text" of "panel/row1999" is "Row 2000"
    And the log has no warning matching "call stack"
