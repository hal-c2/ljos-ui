# Sources:
#   https://doc.qt.io/qt-6/qml-qt-labs-qmlmodels-tablemodel.html
#   https://doc.qt.io/qt-6/qml-qt-labs-qmlmodels-tablemodelcolumn.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: TableModel
  A TableModel keeps rows of JavaScript objects. Its columns are
  TableModelColumns, each naming the key of a row a role reads. Rows are
  changed through its methods, which keep rows and rowCount up to date,
  or by giving rows anew. A row that lacks a key any role of its columns
  reads is refused with a warning. data() joined shows undefined as "".

  Background:
    Given the file "Tm.qml":
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      Item {
        id: root; objectName: "root"
        property string out: ""
        property string heard: ""
        property alias tm: tm
        TableModel {
          id: tm
          TableModelColumn { display: "a" }
          TableModelColumn { display: "b"; edit: "c" }
          rows: [ { a: 1, b: 2, c: 3 }, { a: 4, b: 5, c: 6 } ]
          onRowCountChanged: root.heard += "n"
          onRowsChanged: root.heard += "r"
        }
        function j(v) { return JSON.stringify(v).replace(/"/g, "") }
        function after(f) { heard = ""; f(); out = tm.rowCount + " " + j(tm.rows) + " " + heard }
      }
      """

  Scenario: Cells are read through the roles of their column
    Given the QML:
      """qml
      import QtQuick
      Tm {
        function go() {
          var i = tm.index(1, 1)
          out = [tm.rowCount, tm.columnCount, tm.data(i, "display"), tm.data(i, "edit"), tm.data(i),
                 tm.data(tm.index(0, 0), "toolTip"), tm.index(5, 0).valid, tm.index(0, 5).valid].join(" ")
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "2 2 5 6 5  false false"
    And the log has no warnings

  Scenario: A row read is a copy
    Given the QML:
      """qml
      import QtQuick
      Tm { function go() { var r = tm.getRow(0); r.a = 9; out = j(r) + " " + tm.getRow(0).a } }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "{a:9,b:2,c:3} 1"

  Scenario: Rows are appended, inserted and removed
    Given the QML:
      """qml
      import QtQuick
      Tm {
        function go() {
          var o = []
          after(() => tm.appendRow({ a: 7, b: 8, c: 9, d: 1 })); o.push(out)
          after(() => tm.insertRow(0, { a: 0, b: 0, c: 0 })); o.push(out)
          after(() => tm.removeRow(1, 2)); o.push(out)
          after(() => tm.removeRow(0)); o.push(out)
          out = o.join(" | ")
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "3 [{a:1,b:2,c:3},{a:4,b:5,c:6},{a:7,b:8,c:9,d:1}] nr | 4 [{a:0,b:0,c:0},{a:1,b:2,c:3},{a:4,b:5,c:6},{a:7,b:8,c:9,d:1}] nr | 2 [{a:0,b:0,c:0},{a:7,b:8,c:9,d:1}] nr | 1 [{a:7,b:8,c:9,d:1}] nr"

  Scenario: Rows are moved and set
    Given the QML:
      """qml
      import QtQuick
      Tm {
        function go() {
          var o = []
          tm.appendRow({ a: 7, b: 8, c: 0 })
          after(() => tm.moveRow(0, 2, 1)); o.push(out)
          after(() => tm.moveRow(2, 0)); o.push(out)
          after(() => tm.setRow(1, { a: 5, b: 5, c: 5 })); o.push(out)
          after(() => tm.setRow(3, { a: 6, b: 6, c: 6 })); o.push(out)
          out = o.join(" | ")
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "3 [{a:4,b:5,c:6},{a:7,b:8,c:0},{a:1,b:2,c:3}] r | 3 [{a:1,b:2,c:3},{a:4,b:5,c:6},{a:7,b:8,c:0}] r | 3 [{a:1,b:2,c:3},{a:5,b:5,c:5},{a:7,b:8,c:0}] r | 4 [{a:1,b:2,c:3},{a:5,b:5,c:5},{a:7,b:8,c:0},{a:6,b:6,c:6}] nr"

  Scenario: Data is set through the roles of its column
    Given the QML:
      """qml
      import QtQuick
      Tm {
        function go() {
          heard = ""
          var o = [tm.setData(tm.index(0, 1), 42, "edit"), tm.setData(tm.index(0, 0), 43, "display"),
                   tm.setData(tm.index(0, 0), 44, "edit"), tm.setData(tm.index(5, 0), 45, "display")]
          out = o.join(" ") + " " + j(tm.getRow(0)) + " " + heard
        }
      }
      """
    When the method "go" of "root" is called
    Then the log has a warning matching "setData..: no role named .edit. at column index 0. The available roles for that column are: QList..display..$"
    And the property "out" of "root" is "true true false false {a:43,b:2,c:42} rr"

  Scenario: Clearing takes the rows and keeps the columns; rows given anew are told
    Given the QML:
      """qml
      import QtQuick
      Tm {
        function go() {
          after(() => tm.clear()); var a = out + " " + tm.columnCount
          after(() => { tm.rows = [{ a: 1, b: 1, c: 1 }] })
          out = a + " | " + out
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "0 [] nr 2 | 1 [{a:1,b:1,c:1}] rn"

  Scenario Outline: A call out of range is refused with a warning
    Given the QML:
      """qml
      import QtQuick
      Tm { function go() { after(() => tm.<call>) } }
      """
    When the method "go" of "root" is called
    Then the log has a warning matching "QML TableModel: <warning>"
    And the property "out" of "root" is "2 [{a:1,b:2,c:3},{a:4,b:5,c:6}] "

    Examples:
      | call                    | warning                                                                         |
      | getRow(7)               | getRow..: .rowIndex. 7 is greater than or equal to rowCount.. of 2          |
      | getRow(-1)              | getRow..: .rowIndex. cannot be negative                                       |
      | insertRow(9, {a:1,b:1,c:1}) | insertRow..: .rowIndex. 9 is greater than rowCount.. of 2                    |
      | insertRow(-1, {a:1,b:1,c:1})| insertRow..: .rowIndex. cannot be negative                                    |
      | removeRow(5)            | removeRow..: .rowIndex. 5 is greater than or equal to rowCount.. of 2       |
      | removeRow(1, 5)         | removeRow..: .rows. 5 exceeds available rowCount.. of 2 when removing from .rowIndex. 1 |
      | removeRow(0, 0)         | removeRow..: .rows. is less than or equal to zero                            |
      | moveRow(-1, 0)          | moveRow..: .fromRowIndex. cannot be negative                                  |
      | moveRow(0, 5)           | moveRow..: .toRowIndex. 5 is greater than or equal to rowCount.. of 2       |
      | moveRow(0, 1, 0)        | moveRow..: .rows. is less than or equal to 0                                  |
      | moveRow(1, 1)           | moveRow..: .fromRowIndex. cannot be equal to .toRowIndex.                     |
      | setRow(5, {a:1,b:1,c:1})    | setRow..: .rowIndex. 5 is greater than rowCount.. of 2                      |
      | setRow(-1, {a:1,b:1,c:1})   | setRow..: .rowIndex. cannot be negative                                       |
      | appendRow({a:1})        | appendRow..: expected 2 columns, but only got 1                               |
      | appendRow({a:1,c:1})    | appendRow..: expected a property named .b. in row                             |
      | appendRow({a:1,b:1})    | appendRow..: expected a property named .c. in row                             |
      | setRow(0, {b:1})        | setRow..: expected 2 columns, but only got 1                                  |
      | insertRow(0, 3)         | insertRow..: expected .row. argument to be a QJSValue, but got int instead    |
