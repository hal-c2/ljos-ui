# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-droparea.html
#   https://doc.qt.io/qt-6/qml-qtquick-dragevent.html#accepted-prop
# A drag from another application is made by the window system, which the
# oracle's driver cannot stand in for, so these scenarios run on Ljós
# only. They are the drag-turned-down.feature rules, checked on Qt with
# an item's drag, for a drag from outside.

@ljos-only
Feature: A drag from another application one DropArea turns down
  A file dragged from another application over two DropAreas, one above
  the other, is offered to the lower one when the upper one turns it down
  in `entered`, and each move over the upper one asks it again.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string log: ""
        property int topAsked: 0
        property int topTakesFrom: 99
        DropArea {
          objectName: "bottom"; anchors.fill: parent
          keys: ["text/uri-list"]
          onEntered: root.log += "B.entered "
          onExited: root.log += "B.exited "
          onDropped: (drop) => { root.log += "B.dropped(" + drop.urls + ") "; drop.accept() }
        }
        DropArea {
          objectName: "top"; anchors.fill: parent
          keys: ["text/uri-list"]
          onEntered: (drag) => {
            root.topAsked++
            root.log += "T.entered "
            if (root.topAsked < root.topTakesFrom) drag.accepted = false
          }
          onDropped: (drop) => { root.log += "T.dropped(" + drop.urls + ") "; drop.accept() }
        }
      }
      """

  @api-QtQuick.DragEvent.accepted @api-QtQuick.DropArea.containsDrag
  Scenario: The area beneath takes a file the area above turns down
    When another application drags the URL "file:///home/a.txt" over 50, 50
    Then the drag from another application is taken
    And the property "containsDrag" of "bottom" is true
    And the property "containsDrag" of "top" is false
    When the drag from another application is let go
    Then the drag from another application is taken
    And the property "log" of "root" is "T.entered B.entered B.dropped(file:///home/a.txt) "

  @api-QtQuick.DropArea.entered
  Scenario: A move over the area above asks it again
    When the property "topTakesFrom" of "root" is set to 2
    And another application drags the URL "file:///home/a.txt" over 50, 50
    And the drag from another application moves to 60, 50
    And the drag from another application is let go
    Then the property "log" of "root" is "T.entered B.entered B.exited T.entered T.dropped(file:///home/a.txt) "
