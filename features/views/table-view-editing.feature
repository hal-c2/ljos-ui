# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#editing-cells
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Editing the cells of a TableView
  A delegate's TableView.editDelegate edits its cell. edit(index) makes
  the cell current and an editor of it a child of the cell's delegate,
  with the focus; the delegate's editing is true and it rises above the
  others. Return or Enter in the editor sends the editor's attached
  commit and closes it; Escape or closeEditor() closes it without. Tab
  commits and edits the next cell; editing another cell or a tap outside
  the editor commits the one open. editTriggers say which taps and keys open an editor too:
  Return, Enter and F2 are the edit keys, and the key that opens an
  editor by AnyKeyPressed is typed into it.

  Background:
    Given the file "Sheet.qml":
      """qml
      import QtQuick; import QtQuick.Controls; import QtQml.Models; import Qt.labs.qmlmodels
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property string heard: ""; property var ed: null; property alias tv: tv
        TableView {
          id: tv; objectName: "tv"; width: 300; height: 200; focus: true
          property string out: ""
          selectionModel: ItemSelectionModel {}
          model: TableModel { id: tm; TableModelColumn { display: "a" } TableModelColumn { display: "b" }
            rows: [ { a: "x0", b: "y0" }, { a: "x1", b: "y1" }, { a: "x2", b: "y2" } ] }
          delegate: Rectangle {
            required property bool editing; required property string display
            required property int row; required property int column
            implicitWidth: 100; implicitHeight: 30
            onEditingChanged: root.heard += (editing ? "+" : "-") + row + column + " "
            TableView.editDelegate: TextField { text: display
              Component.onCompleted: root.ed = this; Component.onDestruction: root.ed = null
              TableView.onCommit: { root.heard += "commit:" + text + " "; display = text } } }
          function at(r, c) { return tm.index(r, c) }
          function go() { var b = []; for (var r = 0; r < 3; ++r) b.push(tm.getRow(r).b)
            out = heard + "| " + currentRow + " " + currentColumn + " " + b.join(",") + " "
              + (root.ed ? root.ed.text + " " + root.ed.activeFocus : "none " + activeFocus) }
          function where() { var c = itemAtCell(Qt.point(1, 1))
            out = [root.ed.parent === c, c.z, c.editing, c.visible, selectionModel.hasSelection].join(" ") } } }
      """

  Scenario: edit() opens an editor of the cell
    Given the QML:
      """qml
      import QtQuick
      Sheet { Component.onCompleted: tv.edit(tv.at(1, 1)) }
      """
    When 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "+11 | 1 1 y0,y1,y2 y1 true"
    When the method "where" of "tv" is called
    Then the property "out" of "tv" is "true 2 true true false"

  Scenario Outline: <key> in the editor <does>
    Given the QML:
      """qml
      import QtQuick
      Sheet { Component.onCompleted: tv.edit(tv.at(1, 1)) }
      """
    When 50 milliseconds pass
    And the user types "a"
    And the user presses "<key>"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "<out>"

    Examples:
      | key    | does                                  | out                                          |
      | Return | commits                               | +11 commit:y1a -11 \| 1 1 y0,y1a,y2 none true |
      | Enter  | commits                               | +11 commit:y1a -11 \| 1 1 y0,y1a,y2 none true |
      | Escape | closes it without commit              | +11 -11 \| 1 1 y0,y1,y2 none true            |
      | Tab    | commits and edits the next cell       | +11 commit:y1a -11 +20 \| 2 0 y0,y1a,y2 x2 true |

  Scenario: closeEditor() closes the editor without commit
    Given the QML:
      """qml
      import QtQuick
      Sheet { Component.onCompleted: tv.edit(tv.at(1, 1)) }
      """
    When 50 milliseconds pass
    And the user types "a"
    And the method "closeEditor" of "tv" is called
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "+11 -11 | 1 1 y0,y1,y2 none true"

  Scenario: Editing another cell commits the one open
    Given the QML:
      """qml
      import QtQuick
      Sheet {
        Component.onCompleted: tv.edit(tv.at(0, 1))
        function next() { tv.edit(tv.at(2, 0)) }
      }
      """
    When 50 milliseconds pass
    And the user types "d"
    And the method "next" of "root" is called
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "+01 commit:y0d -01 +20 | 2 0 y0d,y1,y2 x2 true"

  Scenario Outline: <key> on the current cell opens its editor
    Given the QML:
      """qml
      import QtQuick
      Sheet { Component.onCompleted: { tv.editTriggers = <triggers>; tv.selectionModel.setCurrentIndex(tv.at(2, 1), ItemSelectionModel.NoUpdate) } }
      """
    When 50 milliseconds pass
    And the user presses "<key>"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "<out>"

    Examples:
      | key    | triggers                     | out                          |
      | Return | TableView.EditKeyPressed     | +21 \| 2 1 y0,y1,y2 y2 true  |
      | Enter  | TableView.EditKeyPressed     | +21 \| 2 1 y0,y1,y2 y2 true  |
      | F2     | TableView.EditKeyPressed     | +21 \| 2 1 y0,y1,y2 y2 true  |
      | q      | TableView.EditKeyPressed     | \| 2 1 y0,y1,y2 none true    |
      | q      | TableView.AnyKeyPressed      | +21 \| 2 1 y0,y1,y2 y2q true |
      | Return | TableView.NoEditTriggers     | \| 2 1 y0,y1,y2 none true    |

  Scenario: edit() opens an editor whatever the triggers, and keeps the selection
    Given the QML:
      """qml
      import QtQuick
      Sheet {
        Component.onCompleted: {
          tv.editTriggers = TableView.NoEditTriggers
          tv.selectionModel.select(tv.at(1, 1), ItemSelectionModel.ClearAndSelect)
          tv.edit(tv.at(1, 1))
        }
      }
      """
    When 50 milliseconds pass
    And the method "where" of "tv" is called
    Then the property "out" of "tv" is "true 2 true true true"

  Scenario: A tap opens the editor with SingleTapped, and a tap elsewhere commits it to edit there
    Given the QML:
      """qml
      import QtQuick
      Sheet { Component.onCompleted: tv.editTriggers = TableView.SingleTapped }
      """
    When 50 milliseconds pass
    And the user presses the pointer at 150, 75
    And the user releases the pointer at 150, 75
    And 50 milliseconds pass
    And the user presses the pointer at 50, 15
    And the user releases the pointer at 50, 15
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "+21 commit:y2 -21 +00 | 0 0 y0,y1,y2 x0 true"

  Scenario: A tap outside the editor commits it
    Given the QML:
      """qml
      import QtQuick
      Sheet { Component.onCompleted: tv.edit(tv.at(2, 1)) }
      """
    When 50 milliseconds pass
    And the user types "a"
    And the user presses the pointer at 50, 15
    And the user releases the pointer at 50, 15
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "+21 commit:y2a -21 | 0 0 y0,y1,y2a none true"

  Scenario: A tap on a selected cell opens its editor with SelectedTapped
    Given the QML:
      """qml
      import QtQuick
      Sheet {
        Component.onCompleted: {
          tv.editTriggers = TableView.SelectedTapped
          tv.selectionModel.select(tv.at(1, 0), ItemSelectionModel.ClearAndSelect)
        }
      }
      """
    When 50 milliseconds pass
    And the user presses the pointer at 50, 45
    And the user releases the pointer at 50, 45
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "+10 | 1 0 y0,y1,y2 x1 true"

  Scenario: A double tap opens the editor with DoubleTapped
    Given the QML:
      """qml
      import QtQuick
      Sheet {}
      """
    When 50 milliseconds pass
    And the user double-clicks at 150, 45
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "+11 | 1 1 y0,y1,y2 y1 true"

  Scenario: A model given anew closes the editor without commit
    Given the QML:
      """qml
      import QtQuick
      Sheet {
        Component.onCompleted: tv.edit(tv.at(1, 1))
        function drop() { tv.model = null }
        function seen() { tv.out = heard + "| " + (ed === null) }
      }
      """
    When 50 milliseconds pass
    And the method "drop" of "root" is called
    And 50 milliseconds pass
    And the method "seen" of "root" is called
    Then the property "out" of "tv" is "+11 -11 | true"

  Scenario Outline: edit() <what> warns and opens nothing
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      TableView {
        objectName: "tv"; width: 100; height: 100
        property string out: ""
        model: TableModel { id: tm; TableModelColumn { display: "a" } rows: [ { a: 1 } ] }
        delegate: Item { implicitWidth: 50; implicitHeight: 20 }
        function go() { edit(tm.index(<row>, 0)); out = currentRow + " " + currentColumn }
      }
      """
    When 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the log has a warning matching "<warning>"
    And the property "out" of "tv" is "-1 -1"

    Examples:
      | what                          | row | warning                                        |
      | of a cell that is not there   | 5   | cannot edit: index is not valid!$  |
      | of a cell with no editor      | 0   | cannot edit: no TableView.editDelegate set!$ |

  Scenario: Return in an editor that is not a text field commits
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      TableView {
        id: tv; objectName: "tv"; width: 100; height: 100
        property string out: ""
        model: TableModel { id: tm; TableModelColumn { display: "a" } rows: [ { a: 1 }, { a: 2 } ] }
        delegate: Item {
          implicitWidth: 50; implicitHeight: 20
          TableView.editDelegate: Item { focus: true; TableView.onCommit: tv.out += "commit " }
        }
        Component.onCompleted: edit(tm.index(1, 0))
      }
      """
    When 50 milliseconds pass
    And the user presses "Return"
    And 50 milliseconds pass
    Then the property "out" of "tv" is "commit "
