# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#currentIndex-prop
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#count-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Rows that come to a view with none move no current row
  Rows added to a model with no rows push no current row on, whatever number
  `currentIndex` was given: there was no row it named. Once there are rows, rows
  added at or before the current one move it on. A view that chooses its current
  row itself takes the first row that arrives, and rows added before it later move it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property string seen
        component V: ListView { width: 100; height: 100; delegate: Rectangle { width: 100; height: 20 } }
        ListModel { id: ma }
        V { id: a; model: ma; currentIndex: 1 }
        ListModel { id: mb }
        V { id: b; model: mb; currentIndex: 0 }
        ListModel { id: mc }
        V { id: c; model: mc }
        ListModel { id: md; ListElement { k: 0 } ListElement { k: 1 } }
        V { id: d; model: md; currentIndex: 1 }
        Component.onCompleted: {
          ma.append({}); ma.insert(0, {}); ma.insert(0, {})
          mb.append({}); mb.append({}); mb.append({})
        }
        function later() { mc.append({}); mc.insert(0, {}); md.insert(0, {}) }
        function look() { seen = [a.currentIndex, b.currentIndex, c.currentIndex, d.currentIndex].join(",") }
      }
      """

  Scenario: The rows a view first fills with move only a current row that was there
    When 50 milliseconds pass
    And the method "look" of "root" is called
    Then the property "seen" of "root" is "3,0,-1,1"

  Scenario: Rows added later move the current row once there are rows before it
    When 50 milliseconds pass
    And the method "later" of "root" is called
    And 50 milliseconds pass
    And the method "look" of "root" is called
    Then the property "seen" of "root" is "3,0,1,2"
