# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-horizontalheaderview.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-verticalheaderview.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-headerviewdelegate.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: HorizontalHeaderView and VerticalHeaderView
  A header view is a TableView of one row (horizontal) or one column
  (vertical) of sections, synced to the view it heads: a horizontal one
  has a section per column of its syncView, as wide as the column, and
  scrolls across with it; a vertical one a section per row, as tall as
  the row, and scrolls down with it. With no model of its own, every
  role of a section is its number counted from one; its textRole names
  the role the section shows. It is as wide (horizontal) or as tall
  (vertical) as its syncView.

  Background:
    Given the file "Headed.qml":
      """qml
      import QtQuick; import QtQuick.Controls; import Qt.labs.qmlmodels
      Item {
        id: root; objectName: "root"; width: 500; height: 400
        property alias tv: tv; property alias header: header; property alias side: side; property string out: ""
        property real tableHeight: 100; property alias headerDelegate: header.delegate
        TableView {
          id: tv; x: 60; y: 40; width: 200; height: root.tableHeight; columnSpacing: 2; rowSpacing: 1
          model: TableModel { TableModelColumn { display: "a" } TableModelColumn { display: "b" } TableModelColumn { display: "c" }
            rows: [ { a: 0, b: 0, c: 0 }, { a: 0, b: 0, c: 0 }, { a: 0, b: 0, c: 0 }, { a: 0, b: 0, c: 0 }, { a: 0, b: 0, c: 0 }, { a: 0, b: 0, c: 0 } ] }
          delegate: Rectangle { required property int column; required property int row; implicitWidth: 80 + column * 20; implicitHeight: 20 + row * 3 }
        }
        HorizontalHeaderView { id: header; x: 60; syncView: tv }
        VerticalHeaderView { id: side; y: 40; syncView: tv }
        function cells(v) { var s = []; for (var r = 0; r < v.rows; ++r) for (var c = 0; c < v.columns; ++c) {
            var i = v.itemAtCell(Qt.point(c, r)); if (i) s.push(i.contentItem.text + "@" + i.x + "," + i.y + ":" + (v === side && i.width === v.contentWidth ? "W" : i.width) + "x" + i.height) }
          return v.rows + " " + v.columns + " " + s.join(" ") }
        function across() { out = cells(header) } function down() { out = cells(side) }
        function sizes() { out = [header.width, side.height, header.height === header.contentHeight, side.width === side.contentWidth].join(" ") }
        function scroll() { tv.contentX = 30; tv.contentY = 25 }
        function where() { out = [header.contentX, header.contentY, side.contentX, side.contentY].join(" ") }
      }
      """

  Scenario: A horizontal header has a section per column of the view it heads
    Given the QML:
      """qml
      import QtQuick
      Headed {}
      """
    When 50 milliseconds pass
    And the method "across" of "root" is called
    Then the property "out" of "root" is "1 3 1@0,0:80x30 2@82,0:100x30 3@184,0:120x30"

  Scenario: A vertical header has a section per row of the view it heads
    Given the QML:
      """qml
      import QtQuick
      Headed { tableHeight: 60 }
      """
    When 50 milliseconds pass
    And the method "down" of "root" is called
    Then the property "out" of "root" is "6 1 1@0,0:Wx20 2@0,21:Wx23 3@0,45:Wx26"

  Scenario: A header is as long as the view it heads, and as deep as its sections
    Given the QML:
      """qml
      import QtQuick
      Headed {}
      """
    When 50 milliseconds pass
    And the method "sizes" of "root" is called
    Then the property "out" of "root" is "200 100 true true"

  Scenario: Scrolling the view scrolls a header the way it heads
    Given the QML:
      """qml
      import QtQuick
      Headed {}
      """
    When 50 milliseconds pass
    And the method "scroll" of "root" is called
    And 50 milliseconds pass
    And the method "where" of "root" is called
    Then the property "out" of "root" is "30 0 0 25"

  Scenario Outline: A <which> header view starts as Qt's
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      <type> {
        objectName: "h"
        property string out: ["" + syncDirection, "" + flickableDirection, "" + boundsBehavior, clip,
                              resizableColumns, resizableRows, <movable>].join(" ")
      }
      """
    Then the property "out" of "h" is "<out>"

    Examples:
      | which      | type                 | movable        | out                              |
      | horizontal | HorizontalHeaderView | movableColumns | 1 1 3 false true false false |
      | vertical   | VerticalHeaderView   | movableRows    | 2 2 3 false false true false |

  Scenario: Every role of a section is its number, so a list's header shows its role by textRole
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        objectName: "root"; width: 300; height: 200
        property string out: ""
        TableView {
          id: tv; y: 40; width: 200; height: 100
          model: ListModel { ListElement { name: "a" } ListElement { name: "b" } }
          delegate: Rectangle { implicitWidth: 50; implicitHeight: 20 }
        }
        HorizontalHeaderView { id: h; syncView: tv }
        VerticalHeaderView { id: v; x: 220; syncView: tv; textRole: "name" }
        function go() { var c = h.itemAtCell(Qt.point(0, 0))
          out = [h.textRole, v.textRole, h.columns, c.contentItem.text === "", c.model.name, v.itemAtCell(Qt.point(0, 1)).contentItem.text].join(" ") }
      }
      """
    When 50 milliseconds pass
    And the method "go" of "root" is called
    Then the property "out" of "root" is "display name 1 true 1 2"

  Scenario: A header delegate is given its section's model, column and row
    Given the QML:
      """qml
      import QtQuick
      Headed {
        headerDelegate: Text { required property var model; required property int row; required property int column; required property bool current
          text: "c" + row + column + ":" + model.display + ":" + current }
        function names() { var s = []; for (var c = 0; c < 3; ++c) s.push(header.itemAtCell(Qt.point(c, 0)).text); out = s.join(" ") }
      }
      """
    When 50 milliseconds pass
    And the method "names" of "root" is called
    Then the property "out" of "root" is "c00:1:false c01:2:false c02:3:false"
