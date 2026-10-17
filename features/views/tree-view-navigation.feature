# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-treeview.html
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Moving through a TreeView and selecting in it
  With a selectionModel the arrows move the current row; Right expands
  the current row and Left collapses it. Shift extends the selection
  from where it began, as selectionBehavior and selectionMode say. A
  tap makes a cell current and takes the selection away; a double tap
  expands or collapses the row.

  Background:
    Given the file "Tree.qml":
      """qml
      import QtQuick
      import QtQml.Models
      import Qt.labs.qmlmodels
      TreeView {
        id: tv; objectName: "tv"; width: 200; height: 300; focus: true
        property string out: ""
        property var items: [ { name: "docs", rows: [ { name: "a" }, { name: "b", rows: [ { name: "x" } ] } ] },
                              { name: "bin" }, { name: "lib", rows: [ { name: "c" } ] } ]
        selectionModel: ItemSelectionModel {}
        model: TreeModel { TableModelColumn { display: "name" } rows: tv.items }
        delegate: Item { implicitWidth: 60; implicitHeight: 20 }
        function go() {
          var s = [], all = selectionModel.selectedIndexes, e = []
          for (var i = 0; i < all.length; ++i) s.push(rowAtIndex(all[i]))
          for (var r = 0; r < rows; ++r) if (isExpanded(r)) e.push(r)
          out = currentRow + " " + currentColumn + " [" + s.join(",") + "] e" + e.join(",")
        }
      }
      """

  Scenario: Down with nothing current makes the first row current
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the user presses "Down"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "0 0 [] e"

  Scenario: Up and Down move the current row and stop at the ends
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the user presses "Down"
    And the user presses "Down"
    And the user presses "Down"
    And the user presses "Down"
    And the user presses "Up"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "1 0 [] e"

  Scenario: Right expands the current row and Left collapses it
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the user presses "Down"
    And the user presses "Right"
    And 50 milliseconds pass
    And the user presses "Right"
    And 50 milliseconds pass
    And the user presses "Left"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "0 0 [] e"

  Scenario: Right on a row without rows under it does nothing
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the user presses "Down"
    And the user presses "Down"
    And the user presses "Right"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "1 0 [] e"

  Scenario: Left on a row under another does not go up to it
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the user presses "Down"
    And the user presses "Right"
    And 50 milliseconds pass
    And the user presses "Down"
    And the user presses "Left"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "1 0 [] e0"

  Scenario: The arrows go through the rows of expanded rows
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the user presses "Down"
    And the user presses "Right"
    And 50 milliseconds pass
    And the user presses "Down"
    And the user presses "Down"
    And the user presses "Right"
    And 50 milliseconds pass
    And the user presses "Down"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "3 0 [] e0,2"

  Scenario: Left or Right with nothing current does nothing
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the user presses "Right"
    And the user presses "Left"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "-1 -1 [] e"

  Scenario: Shift with the arrows selects the rows from where it began
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the user presses "Down"
    And the user presses "Shift+Down"
    And the user presses "Shift+Down"
    And the user presses "Shift+Up"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "1 0 [0,1] e"

  Scenario: An arrow without Shift takes the selection away
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the user presses "Down"
    And the user presses "Shift+Down"
    And the user presses "Down"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "2 0 [] e"

  Scenario: With SingleSelection Shift selects only the row it began on
    Given the QML:
      """qml
      import QtQuick
      Tree {
        selectionMode: TableView.SingleSelection
      }
      """
    When the user presses "Down"
    And the user presses "Shift+Down"
    And the user presses "Shift+Down"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "2 0 [0] e"

  Scenario: With selection disabled Shift only moves
    Given the QML:
      """qml
      import QtQuick
      Tree {
        selectionBehavior: TableView.SelectionDisabled
      }
      """
    When the user presses "Down"
    And the user presses "Shift+Down"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "1 0 [] e"

  Scenario: Without keyNavigationEnabled the keys do nothing
    Given the QML:
      """qml
      import QtQuick
      Tree {
        keyNavigationEnabled: false
      }
      """
    When the user presses "Down"
    And the user presses "Down"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "-1 -1 [] e"

  Scenario: Page Down and Page Up go by the rows in sight
    Given the QML:
      """qml
      import QtQuick
      Tree {
        height: 100
        items: { var r = []; for (var i = 0; i < 13; ++i) r.push({ name: "r" + i }); return r }
      }
      """
    When the user presses "Down"
    And the user presses "PgDown"
    And the user presses "PgDown"
    And the user presses "PgUp"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "4 0 [] e"

  Scenario: A tap makes its row current and takes the selection away
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the user presses "Down"
    And the user presses "Shift+Down"
    And the user touches 30, 50
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "2 0 [] e"

  Scenario: Without pointerNavigationEnabled a tap makes nothing current
    Given the QML:
      """qml
      import QtQuick
      Tree {
        pointerNavigationEnabled: false
      }
      """
    When the user touches 30, 30
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "-1 -1 [] e"

  Scenario: A double tap expands a row and makes it current
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the user double-clicks at 30, 10
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "0 0 [] e0"

  Scenario: A tap on a TreeViewDelegate makes its row current
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Tree {
        delegate: TreeViewDelegate {}
      }
      """
    When the user touches 40, 30
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "1 0 [] e"

  Scenario: A double tap on a TreeViewDelegate expands its row
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Tree {
        delegate: TreeViewDelegate {}
      }
      """
    When the user double-clicks at 40, 10
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "0 0 [] e0"
