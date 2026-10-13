# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-splitview.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-splithandle.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-container.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: SplitView
  A split view lays its items out side by side with a handle between each
  two. Each item is as long as it prefers, within its minimum and its
  maximum, and one item fills what is left: the one that says so, or the
  last. Dragging a handle makes the item on its side away from the one
  that fills longer or shorter.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property string seen: ""; property var saved
        SplitView {
          id: sv; objectName: "sv"; anchors.fill: parent
          Rectangle { id: a; objectName: "a"; implicitWidth: 100; SplitView.minimumWidth: 50 }
          Rectangle { id: b; objectName: "b"; implicitWidth: 80; SplitView.fillWidth: true }
          Rectangle { id: c; objectName: "c"; SplitView.preferredWidth: 70; SplitView.maximumWidth: 120 }
        }
        function each(f) { seen = sv.children.filter(h => h !== sv.contentItem).map(f).join(" ") }
        function handles() { each(h => h.x + "," + h.y + " " + h.width + "x" + h.height) }
        function pressed() { each(h => h.SplitHandle.pressed) }
        function hovered() { each(h => h.SplitHandle.hovered) }
        function attached() { var v = b.SplitView; seen = [v.minimumWidth, v.preferredWidth, v.maximumWidth, v.fillWidth, v.fillHeight, v.view === sv].join(" ") }
        function preferA(w) { a.SplitView.preferredWidth = w }
        function preferC(w) { c.SplitView.preferredWidth = w }
        function forgetA() { a.SplitView.preferredWidth = undefined; seen = a.SplitView.preferredWidth }
        function noFill() { b.SplitView.fillWidth = false }
        function vertical() { sv.orientation = Qt.Vertical; b.SplitView.fillHeight = true }
        function save() { saved = sv.saveState() }
        function restore(junk) { seen = sv.restoreState(junk ? "junk" : saved) + " " + c.SplitView.preferredWidth }
        function dropC() { sv.removeItem(c) }
      }
      """

  @api-QtQuick.Controls.SplitView.orientation @api-QtQuick.Controls.SplitView.resizing @api-QtQuick.Controls.SplitView.handle
  Scenario: A split view is horizontal and not being resized
    When 100 milliseconds pass
    Then the property "orientation" of "sv" is 1
    And the property "resizing" of "sv" is false
    And the property "focusPolicy" of "sv" is 0
    And the property "count" of "sv" is 3
    And the property "spacing" of "sv" is 0
    And the property "implicitWidth" of "sv" is 0
    And the property "implicitHeight" of "sv" is 0

  @api-QtQuick.Controls.SplitView.fillWidth @api-QtQuick.Controls.SplitView.preferredWidth @api-QtQuick.Controls.SplitView.maximumWidth
  Scenario: The item that fills takes what the others leave
    When 100 milliseconds pass
    Then the property "x" of "a" is 0
    And the property "width" of "a" is 100
    And the property "x" of "b" is 106
    And the property "width" of "b" is 218
    And the property "x" of "c" is 330
    And the property "width" of "c" is 70
    And the property "height" of "c" is 300
    And the property "y" of "c" is 0

  Scenario: There is a handle after every item but the last
    When 100 milliseconds pass
    And the method "handles" of "root" is called
    Then the property "seen" of "root" is "100,0 6x300 324,0 6x300"

  @api-QtQuick.Controls.SplitView.view @api-QtQuick.Controls.SplitView.minimumWidth @api-QtQuick.Controls.SplitView.fillHeight
  Scenario: An item reads its view and its sizes
    When 100 milliseconds pass
    And the method "attached" of "root" is called
    Then the property "seen" of "root" is "0 -1 Infinity true false true"

  Scenario: A preferred width is the item's width
    When 100 milliseconds pass
    And the method "preferA" of "root" is called with 150
    And 100 milliseconds pass
    Then the property "width" of "a" is 150
    And the property "x" of "b" is 156
    And the property "width" of "b" is 168

  @api-QtQuick.Controls.SplitView.minimumWidth
  Scenario: A preferred width is kept above the minimum
    When 100 milliseconds pass
    And the method "preferA" of "root" is called with 10
    And 100 milliseconds pass
    Then the property "width" of "a" is 50
    And the property "x" of "b" is 56
    And the property "width" of "b" is 268

  @api-QtQuick.Controls.SplitView.maximumWidth
  Scenario: A preferred width is kept below the maximum
    When 100 milliseconds pass
    And the method "preferC" of "root" is called with 300
    And 100 milliseconds pass
    Then the property "width" of "c" is 120
    And the property "x" of "c" is 280
    And the property "width" of "b" is 168

  Scenario: A preferred width taken back is none
    When 100 milliseconds pass
    And the method "preferA" of "root" is called with 150
    And 100 milliseconds pass
    And the method "forgetA" of "root" is called
    And 100 milliseconds pass
    Then the property "seen" of "root" is "-1"
    And the property "width" of "a" is 100

  Scenario: Without an item that fills the last one does, within its maximum
    When 100 milliseconds pass
    And the method "noFill" of "root" is called
    And 100 milliseconds pass
    Then the property "width" of "b" is 80
    And the property "x" of "c" is 192
    And the property "width" of "c" is 120

  Scenario: A wider view gives the item that fills more
    When 100 milliseconds pass
    And the property "width" of "root" is set to 600
    And 100 milliseconds pass
    Then the property "width" of "b" is 418
    And the property "x" of "c" is 530

  @api-QtQuick.Controls.SplitView.orientation
  Scenario: A vertical split view lays its items out from the top
    When 100 milliseconds pass
    And the method "vertical" of "root" is called
    And 100 milliseconds pass
    Then the property "y" of "a" is 0
    And the property "width" of "a" is 400
    And the property "y" of "b" is 6
    And the property "height" of "b" is 288
    And the property "x" of "b" is 0

  @api-QtQuick.Controls.SplitView.saveState @api-QtQuick.Controls.SplitView.restoreState
  Scenario: A saved state puts back the preferred widths that were set
    When 100 milliseconds pass
    And the method "save" of "root" is called
    And the method "preferC" of "root" is called with 100
    And 100 milliseconds pass
    And the method "restore" of "root" is called with false
    And 100 milliseconds pass
    Then the property "seen" of "root" is "true 70"
    And the property "width" of "c" is 70

  @api-QtQuick.Controls.SplitView.restoreState
  Scenario: Something that is no saved state is not restored
    When 100 milliseconds pass
    And the method "restore" of "root" is called with true
    Then the property "seen" of "root" is "false 70"
    And the property "width" of "c" is 70

  @api-QtQuick.Controls.SplitView.removeItem
  Scenario: An item taken out leaves one handle fewer
    When 100 milliseconds pass
    And the method "dropC" of "root" is called
    And 100 milliseconds pass
    And the method "handles" of "root" is called
    Then the property "count" of "sv" is 2
    And the property "seen" of "root" is "100,0 6x300"
    And the property "width" of "b" is 294

  @api-QtQuick.Controls.SplitHandle.pressed @api-QtQuick.Controls.SplitView.resizing
  Scenario: Dragging the handle after an item before the one that fills widens it
    When 100 milliseconds pass
    And the user presses the pointer at 103, 150
    And the user moves the pointer to 120, 150
    And the user moves the pointer to 143, 150
    And 100 milliseconds pass
    Then the property "resizing" of "sv" is true
    And the property "width" of "a" is 140
    And the property "x" of "b" is 146
    When the method "pressed" of "root" is called
    Then the property "seen" of "root" is "true false"
    When the user releases the pointer at 143, 150
    And 100 milliseconds pass
    Then the property "resizing" of "sv" is false
    And the property "width" of "a" is 140

  Scenario: Dragging the handle before an item after the one that fills narrows it
    When 100 milliseconds pass
    And the user drags from 327, 150 to 297, 150
    And 100 milliseconds pass
    Then the property "width" of "c" is 100
    And the property "x" of "c" is 300
    And the property "width" of "b" is 188

  Scenario: A handle is not dragged past the item's minimum
    When 100 milliseconds pass
    And the user drags from 103, 150 to 10, 150
    And 100 milliseconds pass
    Then the property "width" of "a" is 50

  @api-QtQuick.Controls.SplitHandle.hovered
  Scenario: The handle under the pointer is hovered
    When 100 milliseconds pass
    And the user moves the pointer to 50, 150
    And the user moves the pointer to 326, 150
    And the method "hovered" of "root" is called
    Then the property "seen" of "root" is "false true"
    When the user moves the pointer to 200, 150
    And the method "hovered" of "root" is called
    Then the property "seen" of "root" is "false false"
