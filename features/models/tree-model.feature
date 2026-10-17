# Sources:
#   https://doc.qt.io/qt-6/qml-qt-labs-qmlmodels-treemodel.html
#   https://doc.qt.io/qt-6/qml-qt-labs-qmlmodels-tablemodelcolumn.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: TreeModel
  A TreeModel keeps rows of JavaScript objects, each of which may have
  rows of its own under the key "rows". Its columns are TableModelColumns,
  each naming the key of a row a role reads. An item is named by its path
  from the top: tm.index([0, 1], column) is the second row under the first.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      Item {
        id: root; objectName: "root"
        property string out: ""
        TreeModel {
          id: tm; objectName: "tm"
          TableModelColumn { display: "name" }
          TableModelColumn { display: "size" }
          rows: [ { name: "docs", size: 2, rows: [ { name: "a.txt", size: 1 }, { name: "b.txt", size: 3 } ] }, { name: "bin", size: 0 } ]
        }
        function at(path) { return tm.index(path, 0) }
        function shape() { out = [tm.columnCount, tm.rowCount(), tm.rowCount(at([0])), tm.rowCount(at([1])), tm.rowCount(at([0, 1]))].join(" ") }
        function read() { var i = tm.index([0, 1], 1); out = [i.valid, i.row, i.column, i.parent.row, tm.data(i), tm.data(i, "display"), tm.data(at([0, 1]))].join(" ") }
        function bad() { var i = at([0, 7]); out = [i.valid, i.row].join(" ") }
        function row() { var r = tm.getRow(at([0])); out = [r.name, r.size, r.rows.length, r.rows[1].name].join(" ") }
        function append() { tm.appendRow(at([1]), { name: "ls", size: 5 }); out = [tm.rowCount(at([1])), tm.data(at([1, 0])), tm.getRow(at([1])).rows.length].join(" ") }
        function appendTop() { tm.appendRow({ name: "etc", size: 4 }); out = [tm.rowCount(), tm.data(at([2]))].join(" ") }
        function remove() { tm.removeRow(at([0, 0])); var one = tm.rowCount(at([0])); tm.removeRow(at([0, 0])); out = [one, tm.rowCount(at([0])), "rows" in tm.getRow(at([0]))].join(" ") }
        function write() { var ok = tm.setData(at([0, 0]), "c.txt", "display"); out = [ok, tm.data(at([0, 0])), tm.getRow(at([0, 0])).name].join(" ") }
        function replace() { tm.setRow(at([1]), { name: "lib", size: 9 }); out = [tm.data(at([1])), tm.data(tm.index([1], 1))].join(" ") }
        function cleared() { tm.clear(); out = [tm.rowCount(), tm.columnCount, tm.rows.length].join(" ") }
      }
      """

  Scenario: A tree model counts its columns and the rows under each row
    When the method "shape" of "root" is called
    Then the property "out" of "root" is "2 2 2 0 0"

  Scenario: A row is named by its path from the top and read by column
    When the method "read" of "root" is called
    Then the property "out" of "root" is "true 1 1 0 3 3 b.txt"

  Scenario: A path that names no row gives an index that is not valid
    When the method "bad" of "root" is called
    Then the property "out" of "root" is "false -1"
    And the log has a warning matching "could not find any node at the specified index"

  Scenario: getRow gives a row with the rows under it
    When the method "row" of "root" is called
    Then the property "out" of "root" is "docs 2 2 b.txt"

  Scenario: appendRow adds a row under the row its index names
    When the method "append" of "root" is called
    Then the property "out" of "root" is "1 ls 1"

  Scenario: appendRow without a parent adds the row at the top
    When the method "appendTop" of "root" is called
    Then the property "out" of "root" is "3 etc"
    And the log has a warning matching "will be appended to root"

  Scenario: removeRow takes a row out, and a row left with none loses its rows
    When the method "remove" of "root" is called
    Then the property "out" of "root" is "1 0 false"

  Scenario: setData writes the key the column's role reads
    When the method "write" of "root" is called
    Then the property "out" of "root" is "true c.txt c.txt"

  Scenario: setRow puts another row in a row's place
    When the method "replace" of "root" is called
    Then the property "out" of "root" is "lib 9"

  Scenario: clear takes every row out and keeps the columns
    When the method "cleared" of "root" is called
    Then the property "out" of "root" is "0 2 0"
