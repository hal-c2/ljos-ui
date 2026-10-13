# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pointerhandler.html#grabPermissions-prop
#   https://doc.qt.io/qt-6/qtquickhandlers-index.html
#   https://doc.qt.io/qt-6/qml-qtquick-draghandler.html
# Verified on Qt 6.11.2 (qml6, qmltestrunner).
# Qt starts a drag beyond 8 pixels and Ljós beyond 10, so moves go 20 or
# more.

@qt-parity
Feature: Who gets the pointer: handlers, MouseAreas and Flickables
  A DragHandler takes the pointer from what only watched the press: a
  TapHandler beside it, a MouseArea, a Flickable around it. The handler
  of the innermost item wins, and a MouseArea that prevents stealing keeps
  the pointer.

  Background:
    Given the file "Flicked.qml":
      """qml
      import QtQuick
      Flickable {
        id: flick
        objectName: "flick"
        width: 200; height: 200
        contentWidth: 200; contentHeight: 1000
        signal said(string s)
        onDraggingChanged: flick.said("flick.dragging=" + dragging + " ")
        Rectangle {
          objectName: "box"
          width: 100; height: 100
          DragHandler {
            onActiveChanged: flick.said("dh.active=" + active + " ")
          }
        }
        MouseArea {
          y: 120; width: 100; height: 60
          onPressed: flick.said("ma.pressed ")
          onCanceled: flick.said("ma.canceled ")
          DragHandler {
            target: null
            onActiveChanged: flick.said("dh2.active=" + active + " ")
          }
        }
      }
      """
    And the file "Nested.qml":
      """qml
      import QtQuick
      Rectangle {
        id: outer
        objectName: "outer"
        width: 180; height: 180
        signal said(string s)
        DragHandler {
          onActiveChanged: outer.said("outer.active=" + active + " ")
        }
        Rectangle {
          objectName: "inner"
          width: 80; height: 80
          DragHandler {
            onActiveChanged: outer.said("inner.active=" + active + " ")
          }
        }
        MouseArea {
          y: 100; width: 80; height: 60
          preventStealing: true
          onPressed: outer.said("keeper.pressed ")
          onCanceled: outer.said("keeper.canceled ")
          onReleased: outer.said("keeper.released ")
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string log: ""
        Flicked { onSaid: (s) => root.log += s }
        Rectangle {
          objectName: "other"
          x: 300; width: 100; height: 100
          TapHandler {
            onTapped: root.log += "tapped "
            onCanceled: root.log += "tap.canceled "
            onPressedChanged: root.log += "tap.pressed=" + pressed + " "
          }
          DragHandler {
            onActiveChanged: root.log += "dh3.active=" + active + " "
          }
        }
        Nested { x: 210; y: 210; onSaid: (s) => root.log += s }
      }
      """

  Rule: A DragHandler takes the pointer from what watched the press

    @api-QtQuick.PointerHandler.grabPermissions
    Scenario: A DragHandler in a Flickable drags its item, not the content
      When the user presses the pointer at 50, 50
      And the user moves the pointer to 50, 80
      And the user moves the pointer to 50, 110
      And the user releases the pointer at 50, 110
      Then the property "log" of "root" is "dh.active=true dh.active=false "
      And the property "y" of "box" is 60
      And the property "contentY" of "flick" is 0

    @api-QtQuick.PointerHandler.grabPermissions @api-QtQuick.TapHandler.canceled
    Scenario: A drag cancels the tap of a TapHandler beside the DragHandler
      When the user presses the pointer at 350, 50
      And the user moves the pointer to 380, 50
      Then the property "log" of "root" is "tap.pressed=true dh3.active=true tap.pressed=false tap.canceled "
      And the property "x" of "other" is 330

    @api-QtQuick.PointerHandler.grabPermissions @api-QtQuick.MouseArea.canceled
    Scenario: A DragHandler in a MouseArea takes the press from it
      When the user presses the pointer at 50, 150
      And the user moves the pointer to 80, 150
      Then the property "log" of "root" is "ma.pressed ma.canceled dh2.active=true "

  Rule: Which of several handlers

    @api-QtQuick.PointerHandler.grabPermissions
    Scenario: The handler of the innermost item wins
      When the user drags from 230, 230 to 270, 230
      Then the property "log" of "root" is "inner.active=true inner.active=false "
      And the property "x" of "inner" is 40
      And the property "x" of "outer" is 210

    @api-QtQuick.MouseArea.preventStealing
    Scenario: A MouseArea that prevents stealing keeps the pointer
      When the user drags from 230, 330 to 270, 330
      Then the property "log" of "root" is "keeper.pressed keeper.released "
      And the property "x" of "outer" is 210
