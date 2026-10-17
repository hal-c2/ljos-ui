# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-models-itemselectionmodel.html
#   https://doc.qt.io/qt-6/qitemselectionmodel.html
#   https://doc.qt.io/qt-6/qitemselection.html
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ItemSelectionModel follows its model and keeps ranges
  The selection is kept as ranges of items, and the current item as an
  index. When the model gains, loses or moves rows, the ranges and the
  current item go with the rows they name. A command with Current
  changes the selection being made, which the next command without
  Current makes part of the selection.

  Background:
    Given the file "Selection.qml":
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        id: root
        property string out: ""; property string heard: ""
        property alias lm: lm
        property alias sm: sm
        ListModel { id: lm; Component.onCompleted: { for (var i = 0; i < 5; ++i) append({ n: i }) } }
        ItemSelectionModel {
          id: sm; model: lm
          onCurrentChanged: (current, previous) => root.heard += previous.row + ">" + current.row + " "
          onSelectionChanged: (s, d) => root.heard += "+" + ranges(s) + " -" + ranges(d) + " "
        }
        function ranges(sel) {
          var o = []
          for (var i = 0; i < sel.length; ++i) o.push(sel[i].top + "-" + sel[i].bottom)
          return o.join(",")
        }
        function state() {
          var s = [], all = sm.selectedIndexes
          for (var i = 0; i < all.length; ++i) s.push(all[i].row)
          return "[" + s.join(",") + "] " + ranges(sm.selection) + " " + sm.currentIndex.row
        }
      }
      """

  Scenario: Rows inserted above the selection move it and the current item
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.select(lm.index(1, 0), ItemSelectionModel.Select)
          sm.setCurrentIndex(lm.index(2, 0), ItemSelectionModel.NoUpdate)
          heard = ""
          lm.insert(0, { n: 9 })
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[2] 2-2 3"
    And the property "heard" of "root" is "+ - "

  Scenario: Rows added below the selection are not told
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.select(lm.index(1, 0), ItemSelectionModel.Select)
          heard = ""
          lm.insert(2, { n: 9 })
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[1] 1-1 -1"
    And the property "heard" of "root" is ""

  Scenario: A row inserted inside a selected range is not selected
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.select(lm.index(0, 0), ItemSelectionModel.Select | ItemSelectionModel.Columns)
          heard = ""
          lm.insert(2, { n: 9 })
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[0,1,3,4,5] 0-1,3-5 -1"
    And the property "heard" of "root" is ""

  Scenario: A selected row removed leaves the selection, and is told
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.select(lm.index(1, 0), ItemSelectionModel.Select)
          sm.select(lm.index(3, 0), ItemSelectionModel.Select)
          heard = ""
          lm.remove(1)
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[2] 2-2 -1"
    And the property "heard" of "root" is "+ -1-1 "

  Scenario: A row removed inside a selected range splits it
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.select(lm.index(0, 0), ItemSelectionModel.Select | ItemSelectionModel.Columns)
          heard = ""
          lm.remove(2)
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[0,1,2,3] 0-1,2-3 -1"
    And the property "heard" of "root" is "+ -2-2 "

  Scenario: When the current row is removed the row above it becomes current
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.setCurrentIndex(lm.index(3, 0), ItemSelectionModel.NoUpdate)
          heard = ""
          lm.remove(2, 2)
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[]  1"
    And the property "heard" of "root" is "3>1 "

  Scenario: When the first row is current and removed the row below becomes current
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.setCurrentIndex(lm.index(1, 0), ItemSelectionModel.NoUpdate)
          heard = ""
          lm.remove(0, 2)
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[]  0"
    And the property "heard" of "root" is "1>2 "

  Scenario: When every row is removed nothing is current
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.setCurrentIndex(lm.index(2, 0), ItemSelectionModel.SelectCurrent)
          heard = ""
          lm.clear()
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[]  -1"
    And the property "heard" of "root" is "2>-1 + -2-2 "

  Scenario: Moved rows take their selection and current item along, untold
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.setCurrentIndex(lm.index(0, 0), ItemSelectionModel.SelectCurrent)
          heard = ""
          lm.move(0, 3, 1)
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[3] 3-3 3"
    And the property "heard" of "root" is ""

  Scenario: Items selected one at a time are ranges of their own
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          for (var i = 0; i < 3; ++i) sm.select(lm.index(i, 0), ItemSelectionModel.Select)
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[0,1,2] 0-0,1-1,2-2 -1"

  Scenario: A command with Current replaces the selection being made
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.select(lm.index(1, 0), ItemSelectionModel.Select)
          sm.select(lm.index(3, 0), ItemSelectionModel.Select)
          heard = ""
          sm.select(lm.index(2, 0), ItemSelectionModel.Select | ItemSelectionModel.Current)
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[1,2] 1-1,2-2 -1"
    And the property "heard" of "root" is "+2-2 -3-3 "

  Scenario: Toggle with Current undoes what is being made, and keeps the rest
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.select(lm.index(1, 0), ItemSelectionModel.Select)
          sm.select(lm.index(0, 0), ItemSelectionModel.Select)
          sm.select(lm.index(1, 0), ItemSelectionModel.ToggleCurrent)
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[]  -1"

  Scenario: Making an item current without a selection command keeps the selection being made
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.select(lm.index(1, 0), ItemSelectionModel.SelectCurrent)
          sm.setCurrentIndex(lm.index(4, 0), ItemSelectionModel.NoUpdate)
          sm.select(lm.index(0, 0), ItemSelectionModel.SelectCurrent)
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[0] 0-0 4"

  Scenario: Selecting inside a range splits it
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.select(lm.index(0, 0), ItemSelectionModel.Select | ItemSelectionModel.Columns)
          sm.select(lm.index(2, 0), ItemSelectionModel.Select)
          sm.select(lm.index(0, 0), ItemSelectionModel.Select)
          sm.select(lm.index(3, 0), ItemSelectionModel.Deselect)
          out = state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[2,1,0,4] 2-2,1-1,0-0,4-4 -1"

  Scenario: A selection of another selection model is selected or deselected
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        ItemSelectionModel { id: other; model: lm }
        function go() {
          other.select(lm.index(1, 0), ItemSelectionModel.Select | ItemSelectionModel.Columns)
          sm.select(other.selection, ItemSelectionModel.Select)
          var both = state()
          sm.select(other.selection, ItemSelectionModel.Deselect)
          out = both + " / " + state()
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "[0,1,2,3,4] 0-4 -1 / []  -1"

  Scenario: A range tells its rows and columns and what it contains
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Selection {
        objectName: "root"
        function go() {
          sm.select(lm.index(0, 0), ItemSelectionModel.Select | ItemSelectionModel.Columns)
          var r = sm.selection[0]
          out = [r.top, r.bottom, r.left, r.right, r.width, r.height, r.valid, r.empty,
                 r.contains(lm.index(2, 0)), r.topLeft.row, r.bottomRight.row, r.model === lm].join(" ")
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "0 4 0 0 1 5 true false true 0 4 true"
