# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-selectionrectangle.html
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#selecting-items
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: SelectionRectangle
  A SelectionRectangle selects cells of its target, a TableView or a
  TreeView with a selectionModel, with the pointer. By Drag, a drag
  across the cells selects the block from the cell pressed to the cell
  under the pointer and makes that one current, and the view does not
  flick; by PressAndHold, a long press selects the cell pressed; Auto
  is PressAndHold on a view that flicks and Drag on one that does not.
  The view's selectionBehavior and selectionMode say what the block
  is. While it selects, the rectangle is active, and dragging while
  the pointer drags; a selection it did not make (a tap, a key, the
  model cleared) makes it inactive. An active rectangle shows a handle
  at the top-left and one at the bottom-right corner of its block, in
  the view's content above the cells; dragging one moves that corner.
  Each handle's SelectionRectangle.control is the rectangle.

  Background:
    Given the file "Sel.qml":
      """qml
      import QtQuick; import QtQuick.Controls; import QtQml.Models; import Qt.labs.qmlmodels
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property alias tv: tv; property alias sr: sr; property string out: ""
        property alias mode: sr.selectionMode; property alias interactive: tv.interactive
        property alias behavior: tv.selectionBehavior; property alias selection: tv.selectionMode
        TableView {
          id: tv; width: 300; height: 200
          selectionModel: ItemSelectionModel {}
          model: TableModel { TableModelColumn { display: "a" } TableModelColumn { display: "b" } TableModelColumn { display: "c" } TableModelColumn { display: "d" }
            rows: { var r = []; for (var i = 0; i < 20; ++i) r.push({ a: 0, b: 0, c: 0, d: 0 }); return r } }
          delegate: Rectangle { property bool cell: true; implicitWidth: 100; implicitHeight: 30 }
        }
        SelectionRectangle { id: sr; target: tv }
        function sel() { var s = []; var l = tv.selectionModel ? tv.selectionModel.selectedIndexes : []
          for (var i = 0; i < l.length; ++i) s.push(l[i].row + "," + l[i].column); s.sort(); return s.join(" ") }
        function handles() { var h = []; var k = tv.contentItem.children
          for (var i = 0; i < k.length; ++i) if (k[i].cell === undefined) h.push(k[i])
          h.sort(function (a, b) { return a.x - b.x }); return h }
        function go() { out = [sr.active, sr.dragging, sel(), tv.currentRow, tv.currentColumn, tv.contentY].join(" ") }
        function look() { out = handles().map(function (h) { return [h.x, h.y, h.width, h.height, h.visible, h.z].join(",") }).join(" ") }
        function clear() { tv.selectionModel.clearSelection() }
      }
      """

  Scenario: A SelectionRectangle starts as Qt's
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Sel {
        Component.onCompleted: out = [SelectionRectangle.Drag, SelectionRectangle.PressAndHold, SelectionRectangle.Auto,
                                      sr.selectionMode, sr.active, sr.dragging, sr.target === tv, handles().length].join(" ")
      }
      """
    Then the property "out" of "root" is "0 1 2 2 false false true 0"

  Scenario Outline: By Drag, <how> selects the block dragged across, and the view does not flick
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Sel { <setup> }
      """
    When 50 milliseconds pass
    And the user presses the pointer at 25, 15
    And the user moves the pointer to 60, 40
    And the user moves the pointer to 125, 75
    And the method "go" of "root" is called
    Then the property "out" of "root" is "true true 0,0 0,1 1,0 1,1 2,0 2,1 2 1 0"
    When the user releases the pointer at 125, 75
    And the method "go" of "root" is called
    Then the property "out" of "root" is "true false 0,0 0,1 1,0 1,1 2,0 2,1 2 1 0"

    Examples:
      | how                              | setup                     |
      | a drag on a view that flicks     | mode: SelectionRectangle.Drag |
      | Auto on a view that does not flick | interactive: false        |

  Scenario: Auto on a view that flicks flicks it by dragging, and selects nothing
    Given the QML:
      """qml
      import QtQuick
      Sel { function flicked() { out = [sr.active, sr.dragging, "[" + sel() + "]", tv.contentY > 0].join(" ") } }
      """
    When 50 milliseconds pass
    And the user presses the pointer at 25, 115
    And the user moves the pointer to 30, 90
    And the user moves the pointer to 35, 60
    And the user moves the pointer to 40, 30
    And the user releases the pointer at 40, 30
    And 1000 milliseconds pass
    And the method "flicked" of "root" is called
    Then the property "out" of "root" is "false false [] true"

  Scenario: A new drag selects anew, and a tap takes the selection away
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Sel { mode: SelectionRectangle.Drag }
      """
    When 50 milliseconds pass
    And the user drags from 25, 15 to 125, 75
    And the user presses the pointer at 150, 75
    And the user moves the pointer to 170, 90
    And the user moves the pointer to 250, 100
    And the user releases the pointer at 250, 100
    And the method "go" of "root" is called
    Then the property "out" of "root" is "true false 2,1 2,2 3,1 3,2 3 2 0"
    When the user presses the pointer at 250, 160
    And the user releases the pointer at 250, 160
    And the method "go" of "root" is called
    Then the property "out" of "root" is "false false  5 2 0"

  Scenario Outline: <what> makes the rectangle inactive and hides its handles
    Given the application starts with the controls style "Basic"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Sel { mode: SelectionRectangle.Drag; tv.focus: true }
      """
    When 50 milliseconds pass
    And the user drags from 25, 15 to 125, 75
    And <step>
    And 50 milliseconds pass
    And the method "go" of "root" is called
    Then the property "out" of "root" is "<out>"
    When the method "look" of "root" is called
    Then the property "out" of "root" is "-14,-14,28,28,false,100 186,76,28,28,false,100"

    Examples:
      | what                      | step                                    | out                   |
      | Clearing the selection    | the method "clear" of "root" is called  | false false  2 1 0    |
      | A key that moves          | the user presses "Down"                 | false false  3 1 0    |

  Scenario: The Basic style's handles are circles at the corners of the block, above the cells
    Given the application starts with the controls style "Basic"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Sel {
        mode: SelectionRectangle.Drag
        function paint() { var h = handles(); out = [h.length, h[0].color, h[0].border.color, h[0].border.width, h[0].radius,
          h[0].parent === tv.contentItem, h[0].SelectionRectangle.control === sr, h[1].SelectionRectangle.control === sr].join(" ") }
      }
      """
    When 50 milliseconds pass
    And the user drags from 25, 15 to 125, 75
    And the method "look" of "root" is called
    Then the property "out" of "root" is "-14,-14,28,28,true,100 186,76,28,28,true,100"
    When the method "paint" of "root" is called
    Then the property "out" of "root" is "2 #ffffff #bdbdbd 1 14 true true true"

  Scenario: Dragging the bottom-right handle moves that corner and the current cell
    Given the application starts with the controls style "Basic"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Sel { mode: SelectionRectangle.Drag
        function held() { out = [sr.dragging, handles()[1].SelectionRectangle.dragging].join(" ") } }
      """
    When 50 milliseconds pass
    And the user drags from 25, 15 to 125, 75
    And the user presses the pointer at 200, 90
    And the user moves the pointer to 220, 110
    And the user moves the pointer to 250, 140
    And the method "held" of "root" is called
    Then the property "out" of "root" is "true true"
    When the user releases the pointer at 250, 140
    And the method "go" of "root" is called
    Then the property "out" of "root" is "true false 0,0 0,1 0,2 1,0 1,1 1,2 2,0 2,1 2,2 3,0 3,1 3,2 4,0 4,1 4,2 4 2 0"
    When the method "look" of "root" is called
    Then the property "out" of "root" is "-14,-14,28,28,true,100 286,136,28,28,true,100"

  Scenario: Dragging the top-left handle moves that corner, and its cell is current
    Given the application starts with the controls style "Basic"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Sel { mode: SelectionRectangle.Drag }
      """
    When 50 milliseconds pass
    And the user drags from 25, 15 to 125, 75
    And the user presses the pointer at 5, 5
    And the user moves the pointer to 50, 30
    And the user moves the pointer to 150, 45
    And the user releases the pointer at 150, 45
    And the method "go" of "root" is called
    Then the property "out" of "root" is "true false 1,1 2,1 1 1 0"

  Scenario Outline: With <behavior>, a drag selects <what>
    Given the application starts with the controls style "Basic"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Sel { mode: SelectionRectangle.Drag; <setup> }
      """
    When 50 milliseconds pass
    And the user drags from 25, 15 to 125, 75
    And the method "go" of "root" is called
    Then the property "out" of "root" is "<out>"
    When the method "look" of "root" is called
    Then the property "out" of "root" is "<handles>"

    Examples:
      | behavior        | what              | setup                                  | out                                                        | handles                                         |
      | SelectRows      | the rows          | behavior: TableView.SelectRows         | true false 0,0 0,1 0,2 0,3 1,0 1,1 1,2 1,3 2,0 2,1 2,2 2,3 2 1 0 | -14,-14,28,28,true,100 386,76,28,28,true,100 |
      | SingleSelection | the pressed cell  | selection: TableView.SingleSelection   | true false 0,0 0 0 0                                       | -14,-14,28,28,true,100 86,16,28,28,true,100  |

  Scenario: With SelectionDisabled a drag selects nothing, and says why
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Sel { mode: SelectionRectangle.Drag; behavior: TableView.SelectionDisabled }
      """
    When 50 milliseconds pass
    And the user drags from 25, 15 to 125, 75
    And the method "go" of "root" is called
    Then the property "out" of "root" is "false false  -1 -1 0"
    And the log has a warning matching "Cannot start selection: TableView.selectionBehavior == TableView.SelectionDisabled"

  Scenario: Without a selection model a drag selects nothing, and says why
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Sel { mode: SelectionRectangle.Drag; tv.selectionModel: null }
      """
    When 50 milliseconds pass
    And the user drags from 25, 15 to 125, 75
    And the method "go" of "root" is called
    Then the property "out" of "root" is "false false  -1 -1 0"
    And the log has a warning matching "Cannot start selection: no SelectionModel assigned!"

  Scenario: By PressAndHold, a long press selects the cell pressed, and moving does not drag
    Given the QML:
      """qml
      import QtQuick
      Sel {}
      """
    When 50 milliseconds pass
    And the user presses the pointer at 125, 45
    And 500 milliseconds pass
    And the method "go" of "root" is called
    Then the property "out" of "root" is "false false  -1 -1 0"
    When 400 milliseconds pass
    And the method "go" of "root" is called
    Then the property "out" of "root" is "true false 1,1 1 1 0"
    When the user moves the pointer to 160, 70
    And the user moves the pointer to 250, 100
    And the user releases the pointer at 250, 100
    And the method "go" of "root" is called
    Then the property "out" of "root" is "true false 1,1 1 1 0"
