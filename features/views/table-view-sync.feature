# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#syncView-prop
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#syncDirection-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A TableView follows the view it is synced to
  A TableView whose syncView is another follows it in the directions
  syncDirection gives, both of them unless told otherwise: across, its
  columns are as wide as the other's, with its column spacing, and it
  scrolls with it; down, its rows are as tall, with its row spacing.
  Its content is as wide or as tall as the other's, whatever its own
  model has. Scrolling either one in a synced direction scrolls both.

  Background:
    Given the file "Pair.qml":
      """qml
      import QtQuick; import Qt.labs.qmlmodels
      Item {
        id: root; objectName: "root"; width: 500; height: 300
        property alias main: main; property alias other: other; property string out: ""
        property int direction: Qt.Horizontal | Qt.Vertical
        TableView {
          id: main; width: 200; height: 100; columnSpacing: 2; rowSpacing: 1
          model: TableModel { TableModelColumn { display: "a" } TableModelColumn { display: "b" } TableModelColumn { display: "c" }
            rows: [ { a: 0, b: 0, c: 0 }, { a: 0, b: 0, c: 0 }, { a: 0, b: 0, c: 0 }, { a: 0, b: 0, c: 0 }, { a: 0, b: 0, c: 0 }, { a: 0, b: 0, c: 0 } ] }
          delegate: Rectangle { required property int column; required property int row; implicitWidth: 60 + column * 20; implicitHeight: 20 + row * 3 }
        }
        TableView {
          id: other; x: 250; width: 200; height: 100; syncView: main; syncDirection: root.direction
          model: TableModel { TableModelColumn { display: "a" } TableModelColumn { display: "b" } rows: [ { a: 0, b: 0 }, { a: 0, b: 0 } ] }
          delegate: Rectangle { implicitWidth: 33; implicitHeight: 7 }
        }
        function sizes() {
          var c = other.itemAtCell(Qt.point(1, 1))
          out = [other.columnWidth(0), other.columnWidth(1), other.rowHeight(0), other.rowHeight(1),
                 c.x, c.y, c.width, c.height, other.contentWidth, other.contentHeight].join(" ")
        }
        function spacing() { main.columnSpacing = 4; main.rowSpacing = 6 } function spaced() { out = other.columnSpacing + " " + other.rowSpacing }
        function scrollMain() { main.contentX = 30; main.contentY = 25 } function scrollOther() { other.contentX = 10; other.contentY = 5 }
        function where() { out = [main.contentX, main.contentY, other.contentX, other.contentY].join(" ") }
      }
      """

  Scenario Outline: Synced <how>, a view takes the other's sizes that way
    Given the QML:
      """qml
      import QtQuick
      Pair { direction: <direction> }
      """
    When 50 milliseconds pass
    And the method "sizes" of "root" is called
    Then the property "out" of "root" is "<out>"

    Examples:
      | how          | direction                   | out                           |
      | both ways    | Qt.Horizontal \| Qt.Vertical | 60 80 20 23 62 21 80 23 244 152 |
      | across       | Qt.Horizontal               | 60 80 7 7 62 7 80 7 244 14      |
      | down         | Qt.Vertical                 | 33 33 20 23 33 21 33 23 66 152  |

  Scenario: Both ways is what a view is synced by unless told
    Given the QML:
      """qml
      import QtQuick
      TableView { objectName: "tv"; property string out: "" + syncDirection }
      """
    Then the property "out" of "tv" is "3"

  Scenario Outline: Synced <how>, a view takes the other's spacing that way
    Given the QML:
      """qml
      import QtQuick
      Pair { direction: <direction> }
      """
    When 50 milliseconds pass
    And the method "spacing" of "root" is called
    And 50 milliseconds pass
    And the method "spaced" of "root" is called
    Then the property "out" of "root" is "<out>"

    Examples:
      | how       | direction                   | out |
      | both ways | Qt.Horizontal \| Qt.Vertical | 4 6 |
      | across    | Qt.Horizontal               | 4 0 |
      | down      | Qt.Vertical                 | 0 6 |

  Scenario Outline: Synced <how>, scrolling <which> scrolls the other that way
    Given the QML:
      """qml
      import QtQuick
      Pair { direction: <direction> }
      """
    When 50 milliseconds pass
    And the method "<method>" of "root" is called
    And 50 milliseconds pass
    And the method "where" of "root" is called
    Then the property "out" of "root" is "<out>"

    Examples:
      | how       | which            | direction                   | method      | out          |
      | both ways | the one followed | Qt.Horizontal \| Qt.Vertical | scrollMain  | 30 25 30 25  |
      | across    | the one followed | Qt.Horizontal               | scrollMain  | 30 25 30 0   |
      | down      | the one followed | Qt.Vertical                 | scrollMain  | 30 25 0 25   |
      | both ways | the follower     | Qt.Horizontal \| Qt.Vertical | scrollOther | 10 5 10 5    |
      | across    | the follower     | Qt.Horizontal               | scrollOther | 10 0 10 5    |
