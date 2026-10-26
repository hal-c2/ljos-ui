# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#currentIndex-prop
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#forceLayout-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The current row's number follows rows added or taken before it at layout
  Rows a script adds or takes away before the current row move it on or back, but
  `currentIndex` says so when the rows are laid out, before `count` does: until then it
  keeps its old number, and `currentItem` is the same row. `forceLayout()` and the
  methods that move the current row tell it at once, and a number a script gives
  `currentIndex` is the one it keeps.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property string seen; property string told
        ListModel { id: m; ListElement { n: "a" } ListElement { n: "b" } ListElement { n: "c" } }
        ListView {
          id: v; objectName: "list"; width: 100; height: 100; model: m; currentIndex: 1
          delegate: Rectangle { width: 100; height: 20; property string n: model.n }
          onCurrentIndexChanged: root.told += "index " + currentIndex + ";"
          onCountChanged: root.told += "count " + count + ";"
        }
        function look() { seen = [v.currentIndex, v.currentItem.n, v.count].join(",") }
        function add() { told = ""; m.insert(0, { n: "z" }); look() }
        function take() { told = ""; m.remove(0); look() }
        function forced() { told = ""; m.insert(0, { n: "z" }); v.forceLayout(); look() }
        function stepped() { told = ""; m.insert(0, { n: "z" }); v.incrementCurrentIndex(); look() }
        function chosen() { told = ""; m.insert(0, { n: "z" }); v.currentIndex = 3; look() }
      }
      """

  Scenario: A row added before the current one moves it on when the rows are laid out
    When the method "add" of "root" is called
    Then the property "seen" of "root" is "1,b,4"
    When 20 milliseconds pass
    Then the property "currentIndex" of "list" is 2
    And the property "told" of "root" is "index 2;count 4;"

  Scenario: A row taken before the current one moves it back when the rows are laid out
    When the method "take" of "root" is called
    Then the property "seen" of "root" is "1,b,2"
    When 20 milliseconds pass
    Then the property "currentIndex" of "list" is 0
    And the property "told" of "root" is "index 0;count 2;"

  Scenario: forceLayout tells the current row's new number at once
    When the method "forced" of "root" is called
    Then the property "seen" of "root" is "2,b,4"
    And the property "told" of "root" is "index 2;count 4;"

  Scenario: incrementCurrentIndex lays the rows out before it moves from where it was
    When the method "stepped" of "root" is called
    Then the property "seen" of "root" is "2,b,4"
    And the property "told" of "root" is "index 2;count 4;"

  # Ljós tells "index 3;count 4;": the rows are laid out after the number is
  # stored, as a setter may not write its own property, and laying them out
  # before it is what qml6 does.
  @backlog
  Scenario: A number a script gives currentIndex after a change is told after the layout
    When the method "chosen" of "root" is called
    Then the property "seen" of "root" is "3,c,4"
    And the property "told" of "root" is "index 2;count 4;index 3;"
    When 20 milliseconds pass
    Then the property "currentIndex" of "list" is 3
