# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-droparea.html
#   https://doc.qt.io/qt-6/qml-qtquick-dragevent.html#accepted-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# Qt tells the areas of a move of the dragged item later, once for many
# moves; the scenarios let a few milliseconds pass for it.

@qt-parity
Feature: A drag one DropArea turns down goes to the areas beneath it
  The DropAreas under the drag are asked in turn, from the top down, until
  one takes it in `entered`. While an area beneath holds the drag, every
  move over an area above that turned it down asks that one again, and the
  areas after it, as if the drag had just come; an area that turned it
  down and has nothing beneath to take it hears nothing more until the
  drag leaves it.

  Background:
    Given the file "Area.qml":
      """qml
      import QtQuick
      DropArea {
        property string name
        property int asked: 0
        property int takesFrom: 1
        signal said(string s)
        onEntered: (drag) => {
          asked++
          said(name + ".entered ")
          if (asked < takesFrom) drag.accepted = false
        }
        onPositionChanged: said(name + ".moved ")
        onExited: said(name + ".exited ")
        onDropped: (drop) => { said(name + ".dropped "); drop.accept() }
        onContainsDragChanged: said(name + ".contains=" + containsDrag + " ")
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property string log: ""
        property int result: -1
        function start() { box.Drag.active = true }
        function drop() { result = box.Drag.drop() }
        Area { objectName: "bottom"; name: "B"; width: 300; height: 300; onSaid: (s) => root.log += s }
        Area { objectName: "top"; name: "T"; width: 100; height: 100; takesFrom: 99; onSaid: (s) => root.log += s }
        Rectangle {
          id: box; objectName: "box"; x: 40; y: 40; width: 20; height: 20
          Drag.hotSpot.x: 10
          Drag.hotSpot.y: 10
          Drag.onTargetChanged: root.log += "target=" + (Drag.target ? Drag.target.objectName : "null") + " "
        }
      }
      """

  Rule: The areas under the drag are asked from the top down

    @api-QtQuick.DragEvent.accepted
    Scenario: The area beneath one that turns the drag down takes it
      When the method "start" of "root" is called
      Then the property "log" of "root" is "T.entered B.entered B.contains=true target=bottom "
      And the property "containsDrag" of "top" is false

    @api-QtQuick.DropArea.dropped
    Scenario: The drop goes to the area beneath that took the drag
      When the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the method "drop" of "root" is called
      Then the property "log" of "root" is "B.dropped B.contains=false "
      And the property "result" of "root" is 2

    @api-QtQuick.DragEvent.accepted
    Scenario: When every area turns the drag down none takes it
      When the property "takesFrom" of "bottom" is set to 99
      And the method "start" of "root" is called
      Then the property "log" of "root" is "T.entered B.entered "
      And the property "containsDrag" of "bottom" is false

  Rule: A move over an area that turned the drag down asks it again

    @api-QtQuick.DropArea.entered
    Scenario: A move over the area above passes the drag on again
      When the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the property "x" of "box" is set to 50
      And 20 milliseconds pass
      Then the property "log" of "root" is "B.exited B.contains=false T.entered B.entered B.contains=true "

    @api-QtQuick.DropArea.entered
    Scenario: An area that takes the drag when asked again gets it from the area beneath
      When the property "takesFrom" of "top" is set to 2
      And the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the property "x" of "box" is set to 50
      And 20 milliseconds pass
      And the property "x" of "box" is set to 55
      And 20 milliseconds pass
      Then the property "log" of "root" is "B.exited B.contains=false T.entered T.contains=true target=top T.moved "

    @api-QtQuick.DropArea.positionChanged
    Scenario: A move beyond the area above tells the area beneath where the drag is
      When the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the property "x" of "box" is set to 150
      And 20 milliseconds pass
      And the property "x" of "box" is set to 160
      And 20 milliseconds pass
      Then the property "log" of "root" is "B.moved B.moved "

    @api-QtQuick.DragEvent.accepted
    Scenario: Areas that all turned the drag down hear nothing of a move over them
      When the property "takesFrom" of "bottom" is set to 99
      And the method "start" of "root" is called
      And the property "log" of "root" is set to ""
      And the property "x" of "box" is set to 50
      And 20 milliseconds pass
      Then the property "log" of "root" is ""
