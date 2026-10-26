# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-drag.html
#   https://doc.qt.io/qt-6/qml-qtquick-droparea.html
#   https://doc.qt.io/qt-6/qt.html#DropAction-enum
# A drag out of the window is carried by the window system, which the
# oracle's driver cannot stand in for: qml6 never comes back from an
# automatic drag under the offscreen platform (QDrag runs a loop of its
# own until the window system ends the drag), so these scenarios run on
# Ljós only. The steps say what the application asks of the window
# system and what the window system tells it back.

@ljos-only
Feature: A drag out of the window, to another application
  An item whose `Drag.dragType` is `Drag.Automatic`, or that calls
  `Drag.startDrag()`, hands its drag to the window system, which carries
  it to whichever application it is let go over. The drag offers what
  `Drag.mimeData` has, with the actions `Drag.supportedActions` allows and
  the one `Drag.proposedAction` would like. The item hears `dragStarted`
  when it starts and `dragFinished` with the action the target took when
  it ends; `Drag.active` is true in between. A drag that comes back over
  the application's own window reaches its DropAreas as the item's drag,
  with its source and keys. Without a window system that carries drags,
  the drag stays in the window.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 200
        property string log: ""
        function start() { box.Drag.startDrag(Qt.CopyAction) }
        function cancel() { box.Drag.cancel() }
        Rectangle {
          id: box; objectName: "box"; width: 50; height: 50; color: "red"
          Drag.dragType: Drag.Automatic; Drag.active: ma.drag.active
          Drag.keys: ["box"]; Drag.supportedActions: Qt.CopyAction | Qt.MoveAction
          Drag.proposedAction: Qt.MoveAction; Drag.mimeData: ({ "text/plain": "hello", "application/x-example": "42" })
          Drag.onDragStarted: root.log += "started(" + Drag.active + ") "
          Drag.onDragFinished: (action) => root.log += "finished(" + action + " " + Drag.active + ") "
          MouseArea { id: ma; anchors.fill: parent; drag.target: parent }
        }
        DropArea {
          objectName: "area"; x: 200; width: 200; height: 200; onExited: root.log += "exited "
          onEntered: (drag) => root.log += "entered(" + (drag.source === box) + " "
            + drag.keys + " " + drag.supportedActions + " " + drag.proposedAction + ") "
          onDropped: (drop) => {
            root.log += "dropped(" + drop.text + "|" + drop.getDataAsString("application/x-example") + ") "
            drop.accept(Qt.CopyAction)
          }
        }
      }
      """

  @api-QtQuick.Drag.dragType @api-QtQuick.Drag.mimeData @api-QtQuick.Drag.supportedActions
  @api-QtQuick.Drag.proposedAction @api-QtQuick.Drag.dragStarted
  Scenario: A drag out of the window offers what its mimeData has
    Given the window system carries drags to other applications
    When the user presses the pointer at 25, 25
    And the user moves the pointer to 40, 25
    And the user moves the pointer to 100, 25
    Then the window system is asked to carry a drag out of the window with:
      | format                | data  |
      | text/plain            | hello |
      | application/x-example | 42    |
    And the drag out of the window allows "copy, move" and proposes "move"
    And the property "log" of "root" is "started(true) "

  @api-QtQuick.Drag.dragFinished @api-QtQuick.Drag.active
  Scenario: The item hears which action the other application took
    Given the window system carries drags to other applications
    When the user presses the pointer at 25, 25
    And the user moves the pointer to 40, 25
    And the user moves the pointer to 100, 25
    And the user releases the pointer at 100, 25
    Then the property "log" of "root" is "started(true) "
    When the other application takes the drag out of the window as copy
    Then the property "log" of "root" is "started(true) finished(1 false) "

  # The window system ending the drag is no write of the application's:
  # the binding on Drag.active starts the next one. The item was left
  # where it was when the window system took the press, at 60, 0.
  @api-QtQuick.Drag.active
  Scenario: The next press drags out of the window again
    Given the window system carries drags to other applications
    When the user presses the pointer at 25, 25
    And the user moves the pointer to 40, 25
    And the user moves the pointer to 100, 25
    And the user releases the pointer at 100, 25
    And the other application takes the drag out of the window as copy
    Then the property "x" of "box" is 60
    When the user presses the pointer at 85, 25
    And the user moves the pointer to 100, 25
    And the user moves the pointer to 150, 25
    Then the property "log" of "root" is "started(true) finished(1 false) started(true) "

  @api-QtQuick.Drag.dragFinished
  Scenario: A drag out of the window that nobody takes finishes with no action
    Given the window system carries drags to other applications
    When the user presses the pointer at 25, 25
    And the user moves the pointer to 40, 25
    And the user moves the pointer to 100, 25
    And the user releases the pointer at 100, 25
    And the other application turns the drag out of the window down
    Then the property "log" of "root" is "started(true) finished(0 false) "

  @api-QtQuick.Drag.startDrag
  Scenario: startDrag hands the drag to the window system with the actions it is given
    Given the window system carries drags to other applications
    When the method "start" of "root" is called
    Then the window system is asked to carry a drag out of the window with:
      | format                | data  |
      | text/plain            | hello |
      | application/x-example | 42    |
    And the drag out of the window allows "copy" and proposes "move"
    And the property "log" of "root" is "started(true) "

  @api-QtQuick.Drag.cancel
  Scenario: Cancelling a drag out of the window gives it back with no action
    Given the window system carries drags to other applications
    When the method "start" of "root" is called
    And the method "cancel" of "root" is called
    Then the drag out of the window was given up
    And the property "log" of "root" is "started(true) finished(0 false) "

  @api-QtQuick.Drag.keys @api-QtQuick.Drag.source @api-QtQuick.DropArea.dropped
  Scenario: A drag out of the window that comes back over it is the item's drag
    Given the window system carries drags to other applications
    When the user presses the pointer at 25, 25
    And the user moves the pointer to 40, 25
    And the user moves the pointer to 100, 25
    And the drag out of the window comes back over 300, 100
    Then the property "log" of "root" is "started(true) entered(true box 3 2) "
    When the drag out of the window is let go there
    Then the property "log" of "root" is "started(true) entered(true box 3 2) dropped(hello|42) finished(1 false) "

  @api-QtQuick.Drag.dragType
  Scenario: Without a window system that carries drags the drag stays in the window
    When the user presses the pointer at 25, 25
    And the user moves the pointer to 40, 25
    And the user moves the pointer to 250, 25
    Then the window system is asked to carry no drag out of the window
    When the user releases the pointer at 250, 25
    Then the property "log" of "root" is "started(true) entered(true box 3 2) exited finished(0 false) "
