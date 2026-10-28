# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#incrementCurrentIndex-method
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#decrementCurrentIndex-method
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#keyNavigationWraps-prop
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#highlight-prop
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#highlightItem-prop
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#highlightFollowsCurrentItem-prop
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

  Rule: ListView highlightItem lifecycle when toggling highlightFollowsCurrentItem, clearing currentIndex, and replacing highlight
    With highlightFollowsCurrentItem false, changing currentIndex leaves highlightItem where it is
    until highlightFollowsCurrentItem turns true. Setting currentIndex to -1 with
    highlightFollowsCurrentItem true removes highlightItem, and replacing the highlight Component
    destroys the old highlightItem and creates the new one at the current row.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 100; height: 100
          property string log: ""; property real hy: lv.highlightItem ? lv.highlightItem.y : -1
          property string hname: lv.highlightItem ? lv.highlightItem.objectName : "null"
          Component {
            id: h1
            Item { objectName: "hl-1"; width: 100; height: 20; Component.onDestruction: root.log += "~hl-1;" }
          }
          Component {
            id: h2
            Item { objectName: "hl-2"; width: 100; height: 20; Component.onCompleted: root.log += "+hl-2;" }
          }
          ListView {
            id: lv; objectName: "lv"; width: 100; height: 100; model: 4
            highlight: h1; highlightFollowsCurrentItem: false; highlightMoveDuration: 0
            delegate: Item { width: 100; height: 20 }
          }
          function moveThenFollow() { lv.currentIndex = 2; lv.highlightFollowsCurrentItem = true }
          function clearAndRestore() {
            lv.highlightFollowsCurrentItem = true; lv.currentIndex = -1
            log += "neg=" + (lv.highlightItem ? lv.highlightItem.objectName : "null") + ";"; lv.currentIndex = 3
          }
          function swapHighlight() { lv.highlightFollowsCurrentItem = true; lv.currentIndex = 2; log = ""; lv.highlight = h2 }
        }
        """

    Scenario: Turning highlightFollowsCurrentItem on snaps highlightItem to the current row
      Then the property "hy" of "root" is 0
      When the method "moveThenFollow" of "root" is called
      Then the property "hy" of "root" is 40

    Scenario: Setting currentIndex to -1 and back to a row restores highlightItem at that row
      When the method "clearAndRestore" of "root" is called
      Then the property "hy" of "root" is 60
      And the property "hname" of "root" is "hl-1"

    Scenario: Replacing the highlight component destroys the old item and places the new one at currentItem
      When the method "swapHighlight" of "root" is called
      Then the property "hname" of "root" is "hl-2"
      And the property "hy" of "root" is 40
      And the property "log" of "root" is "+hl-2;"
