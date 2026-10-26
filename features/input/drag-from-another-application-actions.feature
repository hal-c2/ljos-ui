# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-droparea.html
#   https://doc.qt.io/qt-6/qml-qtquick-dragevent.html
#   https://doc.qt.io/qt-6/qt.html#DropAction-enum
#   https://doc.qt.io/qt-6/qdropevent.html#setDropAction
# A drag from another application is made by the window system, which the
# oracle's driver cannot stand in for, so these scenarios run on Ljós only.
# The steps say what the window system tells the application and what the
# application answers it.

@ljos-only
Feature: What a drag from another application allows, and what it is told
  A drag from another application says which actions it allows (copy,
  move, link) and which one it would like; a DropArea reads them in
  `supportedActions` and `proposedAction`, with the formats it carries as
  its `keys` and its HTML in `html`. The action the area chooses, by
  `accept(action)` or by leaving `action` alone, is the one the window
  system is told, so that the other application knows whether to delete
  what it moved.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property string log: ""
        property int choose: -1
        DropArea {
          objectName: "html"; width: 200; height: 400
          keys: ["text/html"]
          onEntered: (drag) => root.log += "html(" + drag.keys + "|" + drag.hasHtml
            + " " + drag.hasText + "|" + drag.html + ") "
          onDropped: (drop) => { root.log += "dropped(" + drop.html + ") "; drop.accept() }
        }
        DropArea {
          objectName: "any"; x: 200; width: 200; height: 400
          onEntered: (drag) => {
            root.log += "entered(" + drag.supportedActions + " " + drag.proposedAction + " " + drag.action + ") "
            if (root.choose >= 0) drag.accept(root.choose)
          }
          onDropped: (drop) => {
            root.log += "dropped(" + drop.action + ") "
            if (root.choose >= 0) drop.accept(root.choose)
            else drop.accept()
          }
        }
      }
      """

  @api-QtQuick.DragEvent.html @api-QtQuick.DragEvent.hasHtml @api-QtQuick.DragEvent.keys
  @api-QtQuick.DropArea.keys
  Scenario: A drag's keys are the formats it carries, and its HTML is its html
    When another application drags this over 50, 50:
      | format     | data              |
      | text/plain | bold words        |
      | text/html  | <b>bold</b> words |
    Then the drag from another application is taken
    When the drag from another application is let go
    Then the property "log" of "root" is "html(text/plain,text/html|true true|<b>bold</b> words) dropped(<b>bold</b> words) "

  @api-QtQuick.DropArea.keys
  Scenario: An area whose keys a drag does not carry is not offered it
    When another application drags the text "hello" over 50, 50
    Then the drag from another application is not taken
    And the property "log" of "root" is ""

  @api-QtQuick.DragEvent.supportedActions @api-QtQuick.DragEvent.proposedAction
  @api-QtQuick.DragEvent.action
  Scenario: The area reads the actions the other application allows and the one it would like
    Given the drag from another application allows "copy, move" and proposes "move"
    When another application drags the text "hello" over 250, 50
    Then the property "log" of "root" is "entered(3 2 2) "
    And the drag from another application is taken as move
    When the drag from another application is let go
    Then the drag from another application is taken as move
    And the property "log" of "root" is "entered(3 2 2) dropped(2) "

  @api-QtQuick.DragEvent.accept
  Scenario: The action the area chooses is the one the other application is told
    Given the drag from another application allows "copy, move" and proposes "move"
    When the property "choose" of "root" is set to 1
    And another application drags the text "hello" over 250, 50
    Then the drag from another application is taken as copy
    When the drag from another application is let go
    Then the drag from another application is taken as copy
    And the property "log" of "root" is "entered(3 2 2) dropped(2) "

  @api-QtQuick.DragEvent.proposedAction
  Scenario: The other application proposes another action while the drag goes on
    Given the drag from another application allows "copy, move, link" and proposes "copy"
    When another application drags the text "hello" over 250, 50
    Then the drag from another application is taken as copy
    When the drag from another application now proposes "link"
    Then the drag from another application is taken as link
    When the drag from another application is let go
    Then the drag from another application is taken as link
    And the property "log" of "root" is "entered(7 1 1) dropped(4) "

  # Qt's QDropEvent takes "one of the possible actions" as the one the
  # target chose; one the source does not allow leaves the proposed one.
  @api-QtQuick.DragEvent.accept
  Scenario: An action the other application does not allow is not the one it is told
    Given the drag from another application allows "copy, move" and proposes "copy"
    When the property "choose" of "root" is set to 4
    And another application drags the text "hello" over 250, 50
    Then the drag from another application is taken as copy
    When the drag from another application is let go
    Then the drag from another application is taken as copy

  # Each position is a new DragEvent whose action is the proposed one
  # again, as in qml6. The other application hears the action of the last
  # answer before the drop: a qml6 source reports that one in
  # dragFinished, not the one the area takes the drop with.
  @api-QtQuick.DragEvent.accept @api-QtQuick.DropArea.positionChanged
  Scenario: An action chosen as the drag came in lasts until it moves
    Given the drag from another application allows "copy, move" and proposes "move"
    When the property "choose" of "root" is set to 1
    And another application drags the text "hello" over 250, 50
    Then the drag from another application is taken as copy
    When the drag from another application moves to 260, 60
    Then the drag from another application is taken as move

  # Some window systems' sources (Qt's, on X11) give their data only to a
  # target that answered in time, so the answer comes first, from the
  # formats offered and the areas' keys; no handler runs until the data
  # is read.
  @api-QtQuick.DropArea.keys
  Scenario: The other application is answered from the formats it offers before its data is read
    Given the drag from another application allows "copy, move" and proposes "move"
    When another application offers the formats "text/plain, text/html" over 50, 50
    Then the drag from another application is taken as move
    When another application offers the formats "text/plain" over 50, 50
    Then the drag from another application is not taken
    When another application offers the formats "UTF8_STRING" over 250, 50
    Then the drag from another application is taken as move
    And the property "log" of "root" is ""
