# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html
#   https://doc.qt.io/qt-6/qml-qt-labs-qmlmodels-tablemodel.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: TableView shows a model in rows and columns
  A TableView made in QML shows a TableModel's cells, a delegate for
  each, which reads the cell's roles, its row and its column. A model
  of one column (a list, a number or a ListModel) shows as one column,
  its delegates reading modelData and index as a ListView's do. The
  view follows the model's changes, and a delegate may write a role.

  Background:
    Given the file "Table.qml":
      """qml
      import QtQuick
      TableView {
        id: tv; objectName: "tv"; width: 300; height: 200
        property string out: ""
        function cell(c, r) { var i = itemAtCell(Qt.point(c, r)); return i ? i.t : "-" }
        function go() {
          out = rows + "x" + columns + " " + [cell(0, 0), cell(1, 0), cell(0, 1), cell(1, 1)].join(" ")
                + " " + contentWidth + "," + contentHeight
        }
      }
      """

  Scenario: The cells of a TableModel
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      Table {
        model: TableModel { TableModelColumn { display: "a" } TableModelColumn { display: "b" } rows: [ { a: 1, b: "x" }, { a: 2, b: "y" } ] }
        delegate: Item { property string t: display + "/" + row + "/" + column + "/" + (typeof model) + "/" + model.display; implicitWidth: 50; implicitHeight: 20 }
      }
      """
    When the method "go" of "tv" is called
    Then the property "out" of "tv" is "2x2 1/0/0/object/1 x/0/1/object/x 2/1/0/object/2 y/1/1/object/y 100,40"

  Scenario: A delegate with required properties
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      Table {
        model: TableModel { TableModelColumn { display: "a" } TableModelColumn { display: "b" } rows: [ { a: 1, b: "x" }, { a: 2, b: "y" } ] }
        delegate: Item { required property var display; required property int row; required property int column; property string t: display + "/" + row + "/" + column; implicitWidth: 50; implicitHeight: 20 }
      }
      """
    When the method "go" of "tv" is called
    Then the property "out" of "tv" is "2x2 1/0/0 x/0/1 2/1/0 y/1/1 100,40"

  Scenario: An array is one column
    Given the QML:
      """qml
      import QtQuick
      Table {
        model: ["p", "q", "r"]
        delegate: Item { property string t: modelData + "/" + row + "/" + column + "/" + (typeof display) + "/" + index; implicitWidth: 50; implicitHeight: 20 }
      }
      """
    When the method "go" of "tv" is called
    Then the property "out" of "tv" is "3x1 p/0/0/undefined/0 - q/1/0/undefined/1 - 50,60"

  Scenario: A number is one column of that many rows
    Given the QML:
      """qml
      import QtQuick
      Table {
        model: 3
        delegate: Item { property string t: modelData + "/" + row + "/" + column + "/" + index; implicitWidth: 50; implicitHeight: 20 }
      }
      """
    When the method "go" of "tv" is called
    Then the property "out" of "tv" is "3x1 0/0/0/0 - 1/1/0/1 - 50,60"

  Scenario: A ListModel is one column, read by role
    Given the QML:
      """qml
      import QtQuick
      Table {
        model: ListModel { ListElement { name: "a"; v: 1 } ListElement { name: "b"; v: 2 } }
        delegate: Item { property string t: name + "/" + v + "/" + row + "/" + column + "/" + model.name; implicitWidth: 50; implicitHeight: 20 }
      }
      """
    When the method "go" of "tv" is called
    Then the property "out" of "tv" is "2x1 a/1/0/0/a - b/2/1/0/b - 50,40"

  Scenario: An array of objects gives each delegate its object
    Given the QML:
      """qml
      import QtQuick
      Table {
        model: [ { name: "a" }, { name: "b" } ]
        delegate: Item { property string t: (typeof modelData) + "/" + modelData.name + "/" + row; implicitWidth: 50; implicitHeight: 20 }
      }
      """
    When the method "go" of "tv" is called
    Then the property "out" of "tv" is "2x1 object/a/0 - object/b/1 - 50,40"

  Scenario: Rows appended and data set later are shown
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      Table {
        model: TableModel { id: tm; TableModelColumn { display: "a" } rows: [ { a: 1 } ] }
        delegate: Item { property string t: display; implicitWidth: 50; implicitHeight: 20 }
        function grow() { tm.appendRow({ a: 2 }); tm.setData(tm.index(0, 0), 9, "display") }
      }
      """
    When the method "grow" of "tv" is called
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "2x1 9 - 2 - 50,40"

  Scenario: A delegate writes its role into the model
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      Table {
        model: TableModel { id: tm; TableModelColumn { display: "a" } rows: [ { a: 1 } ] }
        delegate: Item { property string t: display; implicitWidth: 50; implicitHeight: 20; Component.onCompleted: model.display = 5 }
        function written() { out = tm.rows[0].a + " " + cell(0, 0) }
      }
      """
    When the method "written" of "tv" is called
    Then the property "out" of "tv" is "5 5"

  Scenario: The defaults of a TableView without a model
    Given the QML:
      """qml
      import QtQuick
      Table {
        function defaults() {
          out = [rows, columns, reuseItems, interactive, clip, keyNavigationEnabled, pointerNavigationEnabled, selectionBehavior,
                 selectionMode, editTriggers, alternatingRows, resizableColumns, flickableDirection, boundsBehavior,
                 currentRow, currentColumn, leftColumn, topRow, syncDirection].join(",")
        }
      }
      """
    When the method "defaults" of "tv" is called
    Then the property "out" of "tv" is "0,1,true,true,false,true,true,1,2,10,true,false,0,3,-1,-1,-1,-1,3"

  Scenario Outline: A view with <what> lays out no cell and is <size> large
    Given the QML:
      """qml
      import QtQuick
      TableView {
        objectName: "tv"; width: <width>; height: 100; model: <model>
        delegate: Rectangle { implicitWidth: 30; implicitHeight: 20 }
        property string out: ""
        function go() { out = [itemAtCell(0, 0) !== null, contentWidth, contentHeight, topRow, leftColumn].join(" ") }
      }
      """
    When 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "<out>"

    Examples:
      | what                  | width | model | size     | out                 |
      | cells but no room     | 0     | 3     | 50 by 50 | false 50 50 -1 -1   |
      | no cells              | 100   | 0     | 0 by 0   | false 0 0 -1 -1     |

  Scenario Outline: A delegate with no implicit <what> makes a cell 50 <what>
    Given the QML:
      """qml
      import QtQuick
      TableView {
        objectName: "tv"; width: 300; height: 200; model: 3
        delegate: Item { <size> }
        property string out: ""
        function go() { out = [columnWidth(0), rowHeight(0), contentWidth, contentHeight, bottomRow].join(" ") }
      }
      """
    When 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "<out>"

    Examples:
      | what            | size               | out            |
      | width or height |                    | 50 50 50 150 2 |
      | width           | implicitHeight: 10 | 50 10 50 30 2  |
