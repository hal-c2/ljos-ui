# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-models-itemselectionmodel.html
#   https://doc.qt.io/qt-6/qitemselectionmodel.html
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ItemSelectionModel
  An ItemSelectionModel keeps which items of a model are selected and
  which one is current: what a view that selects (TableView, TreeView)
  is given as its selectionModel. An item is named by its index in the
  model, which the model gives with index(row, column).

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        id: root; objectName: "root"
        property string out: ""
        property int currents: 0
        property int selections: 0
        property string heard: ""
        ListModel {
          id: lm; objectName: "lm"
          ListElement { name: "a" }
          ListElement { name: "b" }
          ListElement { name: "c" }
        }
        ItemSelectionModel {
          id: sm; objectName: "sm"; model: lm
          onCurrentChanged: (current, previous) => { root.currents++; root.heard = previous.row + ">" + current.row }
          onSelectionChanged: root.selections++
        }
        function rows() {
          var s = []
          var all = sm.selectedIndexes
          for (var i = 0; i < all.length; ++i) s.push(all[i].row)
          return s.sort().join(",")
        }
        function fresh() {
          out = [sm.hasSelection, sm.currentIndex.valid, sm.currentIndex.row, sm.selectedIndexes.length, sm.model === lm].join(" ")
        }
        function indexes() {
          var i = lm.index(1, 0), bad = lm.index(5, 0)
          out = [i.valid, i.row, i.column, i.parent.valid, i.model === lm, bad.valid, bad.row].join(" ")
        }
        function current() {
          sm.setCurrentIndex(lm.index(1, 0), ItemSelectionModel.SelectCurrent)
          out = [sm.currentIndex.row, sm.hasSelection, sm.isSelected(lm.index(1, 0)), sm.isSelected(lm.index(0, 0)), rows()].join(" ")
        }
        function currentOnly() {
          sm.setCurrentIndex(lm.index(2, 0), ItemSelectionModel.NoUpdate)
          out = [sm.currentIndex.row, sm.hasSelection].join(" ")
        }
        function several() {
          sm.select(lm.index(0, 0), ItemSelectionModel.Select)
          sm.select(lm.index(2, 0), ItemSelectionModel.Select)
          var both = rows()
          sm.select(lm.index(1, 0), ItemSelectionModel.ClearAndSelect)
          out = both + " " + rows()
        }
        function toggled() {
          sm.select(lm.index(0, 0), ItemSelectionModel.Toggle)
          var once = rows()
          sm.select(lm.index(0, 0), ItemSelectionModel.Toggle)
          sm.select(lm.index(2, 0), ItemSelectionModel.Select)
          sm.select(lm.index(2, 0), ItemSelectionModel.Deselect)
          out = once + " [" + rows() + "] " + sm.hasSelection
        }
        function rowsSelected() {
          sm.select(lm.index(1, 0), ItemSelectionModel.Select | ItemSelectionModel.Rows)
          out = [sm.isRowSelected(1), sm.isRowSelected(0), sm.selectedRows().length, sm.selectedRows()[0].row].join(" ")
        }
        function cleared() {
          sm.setCurrentIndex(lm.index(1, 0), ItemSelectionModel.SelectCurrent)
          sm.clear()
          out = [sm.hasSelection, sm.currentIndex.valid, rows()].join(" ")
        }
        function unchanged() {
          sm.select(lm.index(0, 0), ItemSelectionModel.Select)
          sm.select(lm.index(0, 0), ItemSelectionModel.Select)
          sm.setCurrentIndex(lm.index(0, 0), ItemSelectionModel.NoUpdate)
          sm.setCurrentIndex(lm.index(0, 0), ItemSelectionModel.NoUpdate)
          out = "x"
        }
        function invalid() {
          sm.select(lm.index(7, 0), ItemSelectionModel.Select)
          out = [sm.hasSelection, rows()].join(" ")
        }
      }
      """

  Scenario: A new selection model has nothing selected and no current item
    When the method "fresh" of "root" is called
    Then the property "out" of "root" is "false false -1 0 true"

  Scenario: A model names its items by index
    When the method "indexes" of "root" is called
    Then the property "out" of "root" is "true 1 0 false true false -1"

  Scenario: Making an item current can select it
    When the method "current" of "root" is called
    Then the property "out" of "root" is "1 true true false 1"
    And the property "currents" of "root" is 1
    And the property "selections" of "root" is 1
    And the property "heard" of "root" is "-1>1"

  Scenario: An item can be made current without selecting it
    When the method "currentOnly" of "root" is called
    Then the property "out" of "root" is "2 false"
    And the property "selections" of "root" is 0

  Scenario: Items are added to the selection, or replace it
    When the method "several" of "root" is called
    Then the property "out" of "root" is "0,2 1"
    And the property "selections" of "root" is 3

  Scenario: Toggle and Deselect take items out of the selection
    When the method "toggled" of "root" is called
    Then the property "out" of "root" is "0 [] false"

  Scenario: A whole row can be selected
    When the method "rowsSelected" of "root" is called
    Then the property "out" of "root" is "true false 1 1"

  Scenario: clear() empties the selection and forgets the current item
    When the method "cleared" of "root" is called
    Then the property "out" of "root" is "false false "
    And the property "currents" of "root" is 2

  Scenario: Nothing is told when nothing changes
    When the method "unchanged" of "root" is called
    Then the property "selections" of "root" is 1
    And the property "currents" of "root" is 1

  Scenario: An index the model does not have selects nothing
    When the method "invalid" of "root" is called
    Then the property "out" of "root" is "false "
    And the property "selections" of "root" is 0
