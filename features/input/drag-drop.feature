# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-drag.html
#   https://doc.qt.io/qt-6/qml-qtquick-droparea.html
#   https://doc.qt.io/qt-6/qml-qtquick-dragevent.html
# Verified on Qt 6.11.2 (qml6, qmltestrunner).
# Qt tells the areas of a move of the dragged item later, once for many
# moves; the scenarios let a few milliseconds pass for it.

@qt-parity
Feature: Dragging an item onto a DropArea
  An item with an active Drag is dragged over the DropAreas of the window.
  The area under its hot spot is told when the drag enters, moves, leaves
  and is dropped; the item's Drag tells which area took it.

  Background:
    Given the file "Areas.qml":
      """qml
      import QtQuick
      Item {
        id: areas
        property bool reject: false
        property Item dragged
        signal said(string s)
        DropArea {
          objectName: "area"; x: 200; y: 200; width: 100; height: 100
          onEntered: (drag) => {
            areas.said("entered(" + drag.x + "," + drag.y + " " + (drag.source === areas.dragged) + " "
              + drag.accepted + " " + drag.proposedAction + " " + drag.supportedActions + ") ")
            if (areas.reject) drag.accepted = false
          }
          onPositionChanged: (drag) => areas.said("pos(" + drag.x + "," + drag.y + ") ")
          onExited: areas.said("exited ")
          onDropped: (drop) => areas.said("dropped(" + drop.x + "," + drop.y + " " + drop.accepted + " " + drop.action + ") ")
          onContainsDragChanged: areas.said("contains=" + containsDrag + " ")
        }
        DropArea {
          objectName: "keyed"; y: 300; width: 100; height: 100
          keys: ["text/plain"]
          onEntered: areas.said("keyed.entered ")
          onExited: areas.said("keyed.exited ")
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property string log: ""
        property string target: box.Drag.target ? box.Drag.target.objectName : "null"
        property string seen: "-"
        property alias reject: areas.reject
        property int result: -1
        property bool active: box.Drag.active
        function start() { box.Drag.active = true }
        function stop() { box.Drag.active = false }
        function cancel() { box.Drag.cancel() }
        function drop() { result = box.Drag.drop() }
        function copy() { box.Drag.proposedAction = Qt.CopyAction }
        function plain() { box.Drag.keys = ["text/plain"] }
        function moveIn() { box.x = 195; box.y = 195; seen = log }
        Areas { id: areas; anchors.fill: parent; dragged: box; onSaid: (s) => root.log += s }
        Rectangle {
          id: box; objectName: "box"; width: 20; height: 20
          Drag.hotSpot.x: 10
          Drag.hotSpot.y: 10
          Drag.onActiveChanged: root.log += "active=" + Drag.active + " "
          Drag.onTargetChanged: root.log += "target=" + (Drag.target ? Drag.target.objectName : "null") + " "
        }
      }
      """

  Rule: Turning a drag on

    @api-QtQuick.Drag.active @api-QtQuick.Drag.target
    Scenario: A drag that starts away from every area only becomes active
      When the method "start" of "root" is called
      Then the property "log" of "root" is "active=true "
      And the property "target" of "root" is "null"

    @api-QtQuick.Drag.active @api-QtQuick.DropArea.entered @api-QtQuick.DropArea.containsDrag
    Scenario: A drag that starts over an area enters it at once
      When the property "x" of "box" is set to 250
      And the property "y" of "box" is set to 250
      And the method "start" of "root" is called
      Then the property "log" of "root" is "entered(60,60 true true 2 7) contains=true target=area active=true "

    @api-QtQuick.Drag.hotSpot @api-QtQuick.DropArea.drag
    Scenario: The area tells where the hot spot is and what is dragged
      When the property "x" of "box" is set to 250
      And the property "y" of "box" is set to 250
      And the method "start" of "root" is called
      Then the property "drag.x" of "area" is 60
      And the property "drag.y" of "area" is 60

  Rule: Moving the dragged item

    @api-QtQuick.DropArea.entered
    Scenario: Moving into an area enters it a moment later
      When the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the method "moveIn" of "root" is called
      Then the property "seen" of "root" is ""
      When 20 milliseconds pass
      Then the property "log" of "root" is "entered(5,5 true true 2 7) contains=true target=area "

    @api-QtQuick.DropArea.positionChanged
    Scenario: Moving inside the area tells the new position
      When the property "x" of "box" is set to 250
      And the property "y" of "box" is set to 250
      And the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the property "x" of "box" is set to 260
      And 20 milliseconds pass
      Then the property "log" of "root" is "pos(70,60) "

    @api-QtQuick.DropArea.exited
    Scenario: Moving out of the area leaves it
      When the property "x" of "box" is set to 250
      And the property "y" of "box" is set to 250
      And the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the property "x" of "box" is set to 0
      And 20 milliseconds pass
      Then the property "log" of "root" is "exited contains=false target=null "

  Rule: Ending a drag

    @api-QtQuick.Drag.drop @api-QtQuick.DropArea.dropped
    Scenario: A drop on an area is told without an exit, and is ignored unless taken
      When the property "x" of "box" is set to 250
      And the property "y" of "box" is set to 250
      And the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the method "drop" of "root" is called
      Then the property "log" of "root" is "dropped(60,60 false 2) contains=false target=null active=false "
      And the property "result" of "root" is 0

    @api-QtQuick.Drag.drop
    Scenario: A drop tells the last move first
      When the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the property "x" of "box" is set to 200
      And the property "y" of "box" is set to 195
      And the method "drop" of "root" is called
      Then the property "log" of "root" is "entered(10,5 true true 2 7) contains=true target=area dropped(10,5 false 2) contains=false target=null active=false "

    @api-QtQuick.Drag.proposedAction @api-QtQuick.DragEvent.action
    Scenario: The drop proposes the drag's action
      When the property "x" of "box" is set to 250
      And the property "y" of "box" is set to 250
      And the method "copy" of "root" is called
      And the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the method "drop" of "root" is called
      Then the property "log" of "root" is "dropped(60,60 false 1) contains=false target=null active=false "

    @api-QtQuick.Drag.active @api-QtQuick.DropArea.exited
    Scenario: Turning the drag off leaves the area
      When the property "x" of "box" is set to 250
      And the property "y" of "box" is set to 250
      And the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the method "stop" of "root" is called
      Then the property "log" of "root" is "exited contains=false target=null active=false "

    @api-QtQuick.Drag.cancel
    Scenario: Cancelling the drag leaves the area
      When the property "x" of "box" is set to 250
      And the property "y" of "box" is set to 250
      And the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the method "cancel" of "root" is called
      Then the property "log" of "root" is "exited contains=false target=null active=false "
      And the property "active" of "root" is false

  Rule: An area may turn a drag down

    @api-QtQuick.DragEvent.accepted
    Scenario: An area that turns the drag down in entered does not contain it
      When the property "reject" of "root" is set to true
      And the property "x" of "box" is set to 250
      And the property "y" of "box" is set to 250
      And the method "start" of "root" is called
      Then the property "log" of "root" is "entered(60,60 true true 2 7) active=true "
      And the property "containsDrag" of "area" is false

    @api-QtQuick.DragEvent.accepted
    Scenario: An area that turned the drag down hears nothing more of it
      When the property "reject" of "root" is set to true
      And the property "x" of "box" is set to 250
      And the property "y" of "box" is set to 250
      And the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the property "x" of "box" is set to 260
      And 20 milliseconds pass
      And the method "drop" of "root" is called
      Then the property "log" of "root" is "active=false "
      And the property "result" of "root" is 0

    @api-QtQuick.DropArea.keys @api-QtQuick.Drag.keys
    Scenario: An area with keys ignores a drag without one of them
      When the property "x" of "box" is set to 40
      And the property "y" of "box" is set to 340
      And the method "start" of "root" is called
      Then the property "log" of "root" is "active=true "

    @api-QtQuick.DropArea.keys @api-QtQuick.Drag.keys
    Scenario: An area with keys takes a drag with one of them
      When the property "x" of "box" is set to 40
      And the property "y" of "box" is set to 340
      And the method "plain" of "root" is called
      And the method "start" of "root" is called
      And the method "cancel" of "root" is called
      Then the property "log" of "root" is "keyed.entered target=keyed active=true keyed.exited target=null active=false "
