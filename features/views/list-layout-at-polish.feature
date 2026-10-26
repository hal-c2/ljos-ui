# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#forceLayout-method
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#positionViewAtEnd-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A ListView lays out the rows its model gave it once, before the frame
  Rows a script adds or takes away are laid out when the view is next polished, once for
  all of them, and not at each change: until then `count` is the model's, and the length
  of the content, the delegates, the current row and what is at a point are what the last
  layout made. `forceLayout()`, `positionViewAtIndex()` and `positionViewAtEnd()` lay them
  out at once.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property string seen; property string told
        ListModel { id: m; objectName: "m" }
        ListView {
          id: v; objectName: "list"; width: 100; height: 100; model: m
          delegate: Rectangle {
            width: 100; height: 20
            Component.onCompleted: root.told += "made " + index + ";"
          }
          onCountChanged: root.told += "count " + count + ";"
          onContentHeightChanged: root.told += "height " + contentHeight + ";"
        }
        function rows(n) { told = ""; for (var i = 0; i < n; i++) m.append({}) }
        function look() {
          seen = [v.count, v.contentHeight, v.contentY, v.itemAtIndex(0) !== null,
                  v.currentIndex, v.currentItem !== null, v.indexAt(5, v.contentY + 5)].join(",")
        }
        function add() { rows(3); look() }
        function take() { m.remove(0); look() }
        function forced() { rows(3); v.forceLayout(); look() }
        function atEnd() { rows(12); v.positionViewAtEnd(); look() }
        function atIndex() { rows(12); v.positionViewAtIndex(5, ListView.Beginning); look() }
      }
      """

  Scenario: Right after rows are added a script sees the model's count and the old layout
    When the method "add" of "root" is called
    Then the property "seen" of "root" is "3,0,0,false,-1,false,-1"
    When 20 milliseconds pass
    Then the property "contentHeight" of "list" is 60
    And the property "currentIndex" of "list" is 0
    And the property "count" of "list" is 3

  Scenario: Rows added together are laid out once, their delegates made before the view tells
    When the method "add" of "root" is called
    And 20 milliseconds pass
    Then the property "told" of "root" is "made 0;made 1;made 2;count 3;height 60;"

  Scenario: Right after a row is taken away a script sees the model's count
    When the method "add" of "root" is called
    And 20 milliseconds pass
    And the method "take" of "root" is called
    Then the property "seen" of "root" is "2,60,0,true,0,true,0"
    When 20 milliseconds pass
    Then the property "contentHeight" of "list" is 40

  Scenario: forceLayout lays the rows out at once
    When the method "forced" of "root" is called
    Then the property "seen" of "root" is "3,60,0,true,0,true,0"

  Scenario: positionViewAtEnd lays the rows out and goes to the last
    When the method "atEnd" of "root" is called
    Then the property "seen" of "root" is "12,240,140,true,0,true,7"
    And the property "contentY" of "list" is 140

  Scenario: positionViewAtIndex lays the rows out and goes to the row
    When the method "atIndex" of "root" is called
    Then the property "seen" of "root" is "12,240,100,true,0,true,5"
