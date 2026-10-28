# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html#offset-prop
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html#currentIndex-prop
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html#onPath-attached-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: View and model behavior matches Qt

  Rule: PathView offset modulo normalization and currentIndex tracking on model insertion and removal
    Setting PathView.offset outside [0, count) wraps it into [0, count) and updates currentIndex
    to match. Inserting or removing a row before the current row in the model adjusts currentIndex
    so the same delegate stays current.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 100; height: 100
          property int idx: pv.currentIndex; property int off: Math.round(pv.offset * 100)
          property string cur: pv.currentItem ? pv.currentItem.objectName : "null"
          ListModel {
            id: lm; objectName: "lm"
            ListElement { name: "a" }
            ListElement { name: "b" }
            ListElement { name: "c" }
            ListElement { name: "d" }
          }
          PathView {
            id: pv; objectName: "pv"; width: 100; height: 100; model: lm; highlightMoveDuration: 0
            path: Path { startX: 0; startY: 50; PathLine { x: 100; y: 50 } }
            delegate: Item {
              required property string name; objectName: "p-" + name; width: 10; height: 10
            }
          }
          function setNegativeOffset() { pv.offset = -1 }
          function setOverflowOffset() { pv.offset = 5 }
          function insertBeforeCurrent() { pv.currentIndex = 2; lm.insert(0, { name: "z" }) }
          function removeBeforeCurrent() { pv.currentIndex = 2; lm.remove(0, 1) }
        }
        """

    Scenario: Negative and overflow offsets wrap into the model count and update currentIndex
      When the method "setNegativeOffset" of "root" is called
      Then the property "off" of "root" is 300
      And the property "idx" of "root" is 1
      When the method "setOverflowOffset" of "root" is called
      Then the property "off" of "root" is 100
      And the property "idx" of "root" is 3

    Scenario: Inserting or removing a row before currentIndex keeps the same current delegate
      When the method "insertBeforeCurrent" of "root" is called
      Then the property "idx" of "root" is 3
      And the property "cur" of "root" is "p-c"
      When the method "removeBeforeCurrent" of "root" is called
      Then the property "idx" of "root" is 1
      And the property "cur" of "root" is "p-b"
