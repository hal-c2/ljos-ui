# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-drawer.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html
#   https://doc.qt.io/qt-6/qtquickcontrols-popups.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Drawer
  A drawer is a popup that slides in from an edge of the window. Its
  position goes from 0, out of sight past the edge, to 1, all of it in.
  It is modal: what is behind it is shaded, as much as the drawer is in,
  and Escape or a release beside it sends it back out.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300; property string log: ""; property string seen: ""
        Drawer {
          id: d; objectName: "d"; width: 120; height: root.height
          onAboutToShow: root.log += "aboutToShow "; onOpened: root.log += "opened "
          onAboutToHide: root.log += "aboutToHide "; onClosed: root.log += "closed "
          onPositionChanged: if (position === 0 || position === 1) root.log += "position" + position + " "
          Rectangle { implicitWidth: 50; implicitHeight: 40 }
        }
        Drawer { id: r; objectName: "r"; edge: Qt.RightEdge; width: 100; height: root.height }
        Drawer { id: t; objectName: "t"; edge: Qt.TopEdge; width: root.width; height: 80 }
        Drawer { id: b; objectName: "b"; edge: Qt.BottomEdge; width: root.width; height: 80 }
        Drawer {
          id: bare; objectName: "bare"; width: 100; height: root.height; enter: null; exit: null
          onOpened: root.log += "opened "
          onPositionChanged: if (position === 0 || position === 1) root.log += "position" + position + " "
        }
        function gapBelow() { seen = b.Overlay.overlay.height - b.y }
        function transitions() { seen = (d.enter !== null) + " " + (d.exit !== null) }
        function shown() { seen = d.Overlay.overlay.children.filter(k => k.visible).map(k => k.opacity).join(" ") }
      }
      """

  @api-QtQuick.Controls.Drawer.edge @api-QtQuick.Controls.Drawer.position @api-QtQuick.Controls.Drawer.dragMargin @api-QtQuick.Controls.Drawer.interactive
  Scenario: A drawer comes from the left and is modal
    Then the property "edge" of "d" is 2
    And the property "position" of "d" is 0
    And the property "dragMargin" of "d" is 8
    And the property "interactive" of "d" is true
    And the property "modal" of "d" is true
    And the property "dim" of "d" is true
    And the property "closePolicy" of "d" is 20
    And the property "focus" of "d" is true
    And the property "visible" of "d" is false
    And the property "opened" of "d" is false

  Scenario: The edges are Qt's
    Then the property "edge" of "r" is 4
    And the property "edge" of "t" is 1
    And the property "edge" of "b" is 8

  Scenario: A drawer that is not modal does not dim
    When the property "modal" of "d" is set to false
    Then the property "dim" of "d" is false

  Scenario: The style slides a drawer in and out
    When the method "transitions" of "root" is called
    Then the property "seen" of "root" is "true true"

  Scenario: The line on the side that faces the window is not content
    Then the property "leftPadding" of "d" is 0
    And the property "rightPadding" of "d" is 1
    And the property "implicitWidth" of "d" is 51
    And the property "implicitHeight" of "d" is 40
    And the property "leftPadding" of "r" is 1
    And the property "rightPadding" of "r" is 0
    And the property "bottomPadding" of "t" is 1
    And the property "topPadding" of "b" is 1

  @api-QtQuick.Controls.Drawer.position
  Scenario: A drawer that opens starts out of sight
    When the method "open" of "d" is called
    Then the property "visible" of "d" is true
    And the property "opened" of "d" is false
    And the property "position" of "d" is 0
    And the property "x" of "d" is -120
    And the property "log" of "root" is "aboutToShow "

  Scenario: A drawer is opened when it is all in
    When the method "open" of "d" is called
    And 1 second passes
    Then the property "position" of "d" is 1
    And the property "opened" of "d" is true
    And the property "x" of "d" is 0
    And the property "y" of "d" is 0
    And the property "log" of "root" is "aboutToShow position1 opened "

  Scenario: A drawer half in is half past its edge
    When the method "open" of "d" is called
    And 1 second passes
    And the property "position" of "d" is set to 0.5
    Then the property "x" of "d" is -60
    And the property "opened" of "d" is true

  Scenario: A drawer is closed when it is all out
    When the method "open" of "d" is called
    And 1 second passes
    And the method "close" of "d" is called
    And 1 second passes
    Then the property "position" of "d" is 0
    And the property "visible" of "d" is false
    And the property "opened" of "d" is false
    And the property "x" of "d" is -120
    And the property "log" of "root" is "aboutToShow position1 opened aboutToHide position0 closed "

  Scenario: A drawer from the right is at the right of the window
    When the method "open" of "r" is called
    And 1 second passes
    Then the property "x" of "r" is 300
    And the property "y" of "r" is 0
    And the property "position" of "r" is 1

  Scenario: A drawer from the top is at the top of the window
    When the method "open" of "t" is called
    And 1 second passes
    Then the property "x" of "t" is 0
    And the property "y" of "t" is 0
    When the property "position" of "t" is set to 0.5
    Then the property "y" of "t" is -40

  Scenario: A drawer from the bottom is at the bottom of the window
    When the method "open" of "b" is called
    And 1 second passes
    And the method "gapBelow" of "root" is called
    Then the property "x" of "b" is 0
    And the property "seen" of "root" is "80"

  Scenario: An open drawer given another edge goes there
    When the method "open" of "d" is called
    And 1 second passes
    And the property "edge" of "d" is set to 4
    Then the property "x" of "d" is 280
    And the property "position" of "d" is 1

  Scenario: A drawer without transitions is opened where its position is
    When the method "open" of "bare" is called
    Then the property "opened" of "bare" is true
    And the property "position" of "bare" is 0
    And the property "x" of "bare" is -100
    And the property "log" of "root" is "opened "
    When the method "close" of "bare" is called
    Then the property "visible" of "bare" is false
    And the property "opened" of "bare" is false

  @api-QtQuick.Controls.Popup.dim
  Scenario: The shade behind a drawer is as far in as the drawer
    When the method "open" of "d" is called
    And 1 second passes
    And the method "shown" of "root" is called
    Then the property "seen" of "root" is "1 1"
    When the property "position" of "d" is set to 0.5
    And the method "shown" of "root" is called
    Then the property "seen" of "root" is "0.5 1"

  @api-QtQuick.Controls.Popup.closePolicy
  Scenario: Escape sends a drawer out
    When the method "open" of "d" is called
    And 1 second passes
    And the user presses "Escape"
    And 1 second passes
    Then the property "visible" of "d" is false
    And the property "position" of "d" is 0

  Scenario: A touch beside a drawer sends it out
    When the method "open" of "d" is called
    And 1 second passes
    And the user touches 300, 150
    And 1 second passes
    Then the property "visible" of "d" is false

  Scenario: A touch on a drawer leaves it in
    When the method "open" of "d" is called
    And 1 second passes
    And the user touches 60, 150
    And 1 second passes
    Then the property "visible" of "d" is true
    And the property "position" of "d" is 1
