# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html#itemAt-method
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html#count-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A binding that reads a Repeater's items through itemAt
  A binding that walks a Repeater's items with itemAt, up to count, runs again when the Repeater
  makes or drops its items, and when a property it read through one of them changes. Here each
  item reads the binding as it is made, so the binding first runs while count already counts the
  items but itemAt has none of them yet.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Row {
        id: root
        objectName: "root"
        property int grow: 0
        readonly property real tallest: {
          let h = 0
          for (let c = 0; c < cells.count; ++c) {
            const it = cells.itemAt(c)
            if (it) h = Math.max(h, it.height)
          }
          return h
        }
        Repeater {
          id: cells
          objectName: "cells"
          model: 3
          Item {
            objectName: "cell" + index
            property real seen: root.tallest
            width: 10
            height: 10 + index * 5 + (index === 1 ? root.grow : 0)
          }
        }
      }
      """

  Scenario: The binding sees the items the Repeater made
    Then the property "tallest" of "root" is 20
    And the property "seen" of "cell0" is 20

  Scenario: A property read through itemAt changes
    When the property "grow" of "root" is set to 30
    Then the property "tallest" of "root" is 45

  Scenario: The Repeater drops some of its items
    When the property "model" of "cells" is set to 1
    Then the property "tallest" of "root" is 10

  Scenario: The Repeater drops all of its items
    When the property "model" of "cells" is set to 0
    Then the property "tallest" of "root" is 0

  Scenario: The Repeater makes more items
    When the property "model" of "cells" is set to 5
    Then the property "tallest" of "root" is 30
