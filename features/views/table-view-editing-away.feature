# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#editing-cells
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#reusing-items
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An editor whose cell leaves sight
  The delegate of a cell with an open editor is not pooled or destroyed
  when the cell is scrolled out of sight: it stays, still editing, out
  of the way, and the editor keeps its text and the focus, while
  itemAtCell has no delegate for the cell. Back in sight, the cell has
  that delegate again. Closing the editor while its cell is out of
  sight destroys the delegate, pooled views too. Tab there moves the
  current cell as it does on the view and leaves the editor open.

  Background:
    Given the file "Away.qml":
      """qml
      import QtQuick; import QtQuick.Controls; import QtQml.Models; import Qt.labs.qmlmodels
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property string heard: ""; property var ed: null; property var cell: null; property string out: ""
        property alias tv: tv; property alias reuse: tv.reuseItems
        TableView {
          id: tv; objectName: "tv"; width: 300; height: 100; focus: true
          selectionModel: ItemSelectionModel {}
          model: TableModel { id: tm; TableModelColumn { display: "a" } TableModelColumn { display: "b" }
            rows: { var r = []; for (var i = 0; i < 30; ++i) r.push({ a: "x" + i, b: "y" + i }); return r } }
          delegate: Rectangle {
            required property bool editing; required property string display; required property int row; required property int column
            implicitWidth: 100; implicitHeight: 30
            function hear(s) { if (row === 1 && column === 1) root.heard += s + " " }
            onEditingChanged: hear(editing ? "+" : "-"); TableView.onPooled: hear("pooled"); TableView.onReused: hear("reused")
            Component.onDestruction: hear("gone")
            TableView.editDelegate: TextField { text: display
              Component.onCompleted: { root.ed = this; root.cell = parent } Component.onDestruction: root.ed = null
              TableView.onCommit: { root.heard += "commit:" + text + " "; display = text } } } }
        function open() { tv.edit(tm.index(1, 1)) } function away() { tv.contentY = 600 } function back() { tv.contentY = 0 }
        function go() { var c = tv.itemAtCell(Qt.point(1, 1))
          out = heard + "| " + (ed ? [ed.text, ed.activeFocus, ed.parent === cell, cell.editing, cell.x + cell.width <= tv.contentX].join(" ") : "none")
            + " | " + (c === null ? "null" : c === cell) + " " + tv.currentRow + " " + tv.currentColumn + " " + tm.getRow(1).b }
      }
      """

  Scenario Outline: Out of sight, an editor stays open in its delegate (reuseItems <reuse>)
    Given the QML:
      """qml
      import QtQuick
      Away { reuse: <reuse>; Component.onCompleted: open() }
      """
    When 50 milliseconds pass
    And the user types "a"
    And the method "away" of "root" is called
    And 50 milliseconds pass
    And the user types "b"
    And the method "go" of "root" is called
    Then the property "out" of "root" is "+ | y1ab true true true true | null 1 1 y1"

    Examples:
      | reuse |
      | true  |
      | false |

  Scenario: Back in sight, the cell has the delegate with the editor again
    Given the QML:
      """qml
      import QtQuick
      Away { reuse: true; Component.onCompleted: open() }
      """
    When 50 milliseconds pass
    And the method "away" of "root" is called
    And 50 milliseconds pass
    And the method "back" of "root" is called
    And 50 milliseconds pass
    And the user types "a"
    And the method "go" of "root" is called
    Then the property "out" of "root" is "+ | y1a true true true false | true 1 1 y1"
    When the user presses "Return"
    And the method "go" of "root" is called
    Then the property "out" of "root" is "+ commit:y1a - | none | true 1 1 y1a"

  Scenario Outline: Closed out of sight by <how>, the editor's delegate is destroyed (reuseItems <reuse>)
    Given the QML:
      """qml
      import QtQuick
      Away { reuse: <reuse>; Component.onCompleted: open() }
      """
    When 50 milliseconds pass
    And the method "away" of "root" is called
    And 50 milliseconds pass
    And <step>
    And the method "go" of "root" is called
    Then the property "out" of "root" is "<out>"

    Examples:
      | how           | reuse | step                                    | out                                    |
      | Return        | true  | the user presses "Return"               | + commit:y1 - gone \| none \| null 1 1 y1 |
      | Return        | false | the user presses "Return"               | + commit:y1 - gone \| none \| null 1 1 y1 |
      | closeEditor() | true  | the method "closeEditor" of "tv" is called | + - gone \| none \| null 1 1 y1      |

  Scenario: Tab with the edited cell out of sight moves the current cell and leaves the editor open
    Given the QML:
      """qml
      import QtQuick
      Away { reuse: true; Component.onCompleted: open() }
      """
    When 50 milliseconds pass
    And the method "away" of "root" is called
    And 50 milliseconds pass
    And the user presses "Tab"
    And 50 milliseconds pass
    And the method "go" of "root" is called
    Then the property "out" of "root" is "+ | y1 true true true true | null 2 0 y1"
