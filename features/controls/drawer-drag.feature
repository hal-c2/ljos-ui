# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-drawer.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#parent-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-overlay.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Dragging a drawer in and out
  A drawer is placed by the edges of its window, so its parent is the
  window's overlay. A drag that starts at its edge, within dragMargin,
  brings it in, and a drag on an open drawer takes it back out. The
  press reaches what is under it first; the drawer takes the pointer
  only once it has moved more than 20 pixels along the drawer's axis.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300; property string log: ""; property string seen: ""
        Button {
          x: 0; y: 100; width: 200; height: 100
          onClicked: root.log += "clicked "; onPressedChanged: root.log += "pressed" + pressed + " "
        }
        Drawer {
          id: d; objectName: "d"; width: 120; height: root.height
          onAboutToShow: root.log += "aboutToShow "; onOpened: root.log += "opened "
          onAboutToHide: root.log += "aboutToHide "; onClosed: root.log += "closed "
        }
        Drawer { id: r; objectName: "r"; edge: Qt.RightEdge; width: 120; height: root.height }
        function parents() { seen = (d.parent === d.Overlay.overlay) + " " + (r.parent === r.Overlay.overlay) + " " + d.Overlay.overlay.visible }
        function own() { d.parent = root; seen = "" + (d.parent === root); d.parent = undefined; seen += " " + (d.parent === root) }
        function openNow() { d.open(); log = "" }
      }
      """

  @api-QtQuick.Controls.Popup.parent @api-QtQuick.Controls.Overlay.overlay
  Scenario: A drawer's parent is the overlay, which is there while the window has a drawer
    When the method "parents" of "root" is called
    Then the property "seen" of "root" is "true true true"

  @api-QtQuick.Controls.Popup.parent
  Scenario: A drawer given a parent keeps it, and reset goes back to where it is declared
    When the method "own" of "root" is called
    Then the property "seen" of "root" is "true true"

  @api-QtQuick.Controls.Drawer.dragMargin
  Scenario: A drag from the edge brings the drawer in
    When the user drags from 4, 150 to 100, 150
    And 1 second passes
    Then the property "position" of "d" is 1
    And the property "opened" of "d" is true
    And the property "log" of "root" is "pressedtrue aboutToShow pressedfalse opened "

  Scenario: A drawer at the right comes in from the right edge
    When the user drags from 396, 150 to 300, 150
    And 1 second passes
    Then the property "position" of "r" is 1
    And the property "x" of "r" is 280

  Scenario: A press at the edge that is not dragged is the content's
    When the user presses the pointer at 4, 150
    And the user releases the pointer at 4, 150
    And 1 second passes
    Then the property "visible" of "d" is false
    And the property "log" of "root" is "pressedtrue pressedfalse clicked "

  @api-QtQuick.Controls.Drawer.dragMargin
  Scenario: A drag that starts beyond the margin is the content's
    When the user drags from 12, 150 to 90, 150
    And 1 second passes
    Then the property "visible" of "d" is false
    And the property "log" of "root" is "pressedtrue pressedfalse clicked "

  @api-QtQuick.Controls.Drawer.dragMargin
  Scenario: A drag margin of 0 turns dragging in off
    When the property "dragMargin" of "d" is set to 0
    And the user drags from 0, 150 to 90, 150
    And 1 second passes
    Then the property "visible" of "d" is false

  @api-QtQuick.Controls.Drawer.interactive
  Scenario: A drawer that is not interactive is not dragged in
    When the property "interactive" of "d" is set to false
    And the user drags from 4, 150 to 90, 150
    And 1 second passes
    Then the property "visible" of "d" is false
    And the property "log" of "root" is "pressedtrue pressedfalse clicked "

  Scenario: A drag on an open drawer takes it out
    When the method "openNow" of "root" is called
    And 1 second passes
    And the user drags from 100, 150 to 10, 150
    And 1 second passes
    Then the property "visible" of "d" is false
    And the property "position" of "d" is 0
    And the property "log" of "root" is "opened aboutToHide closed "

  Scenario: An open drawer dragged a little goes back in and says so again
    When the method "openNow" of "root" is called
    And 1 second passes
    And the user drags from 100, 150 to 70, 150
    And 1 second passes
    Then the property "position" of "d" is 1
    And the property "log" of "root" is "opened aboutToShow opened "

  Scenario: An open drawer is not dragged across its edge
    When the method "openNow" of "root" is called
    And 1 second passes
    And the user drags from 60, 150 to 60, 40
    And 1 second passes
    Then the property "position" of "d" is 1
    And the property "log" of "root" is "opened "
