# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#incrementCurrentIndex-method
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#decrementCurrentIndex-method
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#keyNavigationWraps-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ListView behavior matches Qt

  Rule: ListView incrementCurrentIndex and decrementCurrentIndex when currentIndex is -1 or model is empty
    When currentIndex is -1 on a non-empty ListView, incrementCurrentIndex selects index 0 whether
    or not keyNavigationWraps is true, while decrementCurrentIndex wraps to count - 1 only when
    keyNavigationWraps is true and stays at -1 otherwise. On an empty model both leave
    currentIndex at -1.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 100; height: 100
          property int idx: lv.currentIndex
          property string cur: lv.currentItem ? lv.currentItem.objectName : "null"
          ListView {
            id: lv; objectName: "lv"; width: 100; height: 100; model: 3
            delegate: Item {
              required property int index
              objectName: "d-" + index; width: 100; height: 20
            }
          }
          function incFromNone(wraps) {
            lv.keyNavigationWraps = wraps; lv.currentIndex = -1; lv.incrementCurrentIndex()
          }
          function decFromNone(wraps) {
            lv.keyNavigationWraps = wraps; lv.currentIndex = -1; lv.decrementCurrentIndex()
          }
          function stepOnEmpty() {
            lv.model = 0; lv.forceLayout()
            lv.keyNavigationWraps = true; lv.incrementCurrentIndex(); lv.decrementCurrentIndex()
          }
        }
        """

    Scenario: incrementCurrentIndex from -1 moves to row 0 with or without wrapping
      When the method "incFromNone" of "root" is called with false
      Then the property "idx" of "root" is 0
      And the property "cur" of "root" is "d-0"
      When the method "incFromNone" of "root" is called with true
      Then the property "idx" of "root" is 0

    Scenario: decrementCurrentIndex from -1 stays at -1 without wrapping and selects the last row with wrapping
      When the method "decFromNone" of "root" is called with false
      Then the property "idx" of "root" is -1
      And the property "cur" of "root" is "null"
      When the method "decFromNone" of "root" is called with true
      Then the property "idx" of "root" is 2
      And the property "cur" of "root" is "d-2"

    Scenario: Incrementing and decrementing on an empty model leaves currentIndex at -1
      When the method "stepOnEmpty" of "root" is called
      Then the property "idx" of "root" is -1
      And the property "cur" of "root" is "null"
