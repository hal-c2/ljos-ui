# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html
# Verified on Qt 6.11.2 (qml6, vnc).

@qt-parity
Feature: Dragging a Flickable
  The content of a Flickable follows a pointer that is pressed and moved.
  The press first belongs to what it landed on. A move past the drag
  threshold makes the Flickable ready; with the move after that it takes
  the press, and the content follows from where the pointer then is.

  Rule: The content follows the pointer

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          Flickable {
            objectName: "f"
            width: 200; height: 200
            contentWidth: 200; contentHeight: 1000
            boundsBehavior: Flickable.StopAtBounds
            onDragStarted: root.log += "dragStarted "
            onDragEnded: root.log += "dragEnded "
            onMovementStarted: root.log += "movementStarted "
            onMovementEnded: root.log += "movementEnded "
          }
        }
        """

    @api-QtQuick.Flickable.dragging
    Scenario: A press alone moves nothing
      When the user presses the pointer at 100, 150
      Then the property "dragging" of "f" is false
      And the property "moving" of "f" is false
      And the property "contentY" of "f" is 0

    Scenario: The first move past the threshold only makes it ready
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 120
      Then the property "contentY" of "f" is 0
      And the property "dragging" of "f" is false

    Scenario: A move that stays inside the threshold is no drag
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 146
      And the user moves the pointer to 100, 144
      And the user moves the pointer to 100, 146
      Then the property "contentY" of "f" is 0
      And the property "dragging" of "f" is false

    Scenario: The second move begins the drag, and moves nothing yet
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 130
      And the user moves the pointer to 100, 120
      Then the property "dragging" of "f" is true
      And the property "moving" of "f" is true
      And the property "log" of "root" is "dragStarted movementStarted "
      And the property "contentY" of "f" is 0

    @api-QtQuick.Flickable.contentY
    Scenario: The content follows from the second move on
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 130
      And the user moves the pointer to 100, 120
      And the user moves the pointer to 100, 100
      Then the property "contentY" of "f" is 20

    @api-QtQuick.Flickable.dragging @api-QtQuick.Flickable.draggingVertically
    @api-QtQuick.Flickable.moving @api-QtQuick.Flickable.movingVertically
    Scenario Outline: While it is dragged it is dragging and moving
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 130
      And the user moves the pointer to 100, 120
      And the user moves the pointer to 100, 100
      Then the property "<property>" of "f" is <value>

      Examples:
        | property             | value |
        | dragging             | true  |
        | draggingVertically   | true  |
        | draggingHorizontally | false |
        | moving               | true  |
        | movingVertically     | true  |
        | movingHorizontally   | false |
        | flicking             | false |
        | atYBeginning         | false |

    Scenario: The content goes back when the pointer does
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 130
      And the user moves the pointer to 100, 120
      And the user moves the pointer to 100, 60
      And the user moves the pointer to 100, 90
      Then the property "contentY" of "f" is 30

    @api-QtQuick.Flickable.dragEnded @api-QtQuick.Flickable.movementEnded
    Scenario: Letting go after a pause leaves the content where it is
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 130
      And the user moves the pointer to 100, 120
      And the user moves the pointer to 100, 100
      And 500 milliseconds pass
      And the user releases the pointer at 100, 100
      And 100 milliseconds pass
      Then the property "contentY" of "f" is 20
      And the property "dragging" of "f" is false
      And the property "moving" of "f" is false

    @api-QtQuick.Flickable.dragStarted @api-QtQuick.Flickable.movementStarted
    Scenario: The drag starts before the movement and ends before it
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 130
      And the user moves the pointer to 100, 120
      And the user moves the pointer to 100, 100
      And 500 milliseconds pass
      And the user releases the pointer at 100, 100
      And 100 milliseconds pass
      Then the property "log" of "root" is "dragStarted movementStarted dragEnded movementEnded "

    Scenario: A press that is let go without a drag starts nothing
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 120
      And 500 milliseconds pass
      And the user releases the pointer at 100, 120
      And 100 milliseconds pass
      Then the property "log" of "root" is ""
      And the property "contentY" of "f" is 0

    Scenario: A sideways move does not drag content that is no wider
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 70, 150
      And the user moves the pointer to 50, 150
      And the user moves the pointer to 20, 150
      Then the property "dragging" of "f" is false
      And the property "contentX" of "f" is 0

  Rule: The content stops at its bounds

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 400; height: 400
          Flickable {
            objectName: "f"
            width: 200; height: 200
            contentWidth: 260; contentHeight: 300
            boundsBehavior: Flickable.StopAtBounds
          }
        }
        """

    @api-QtQuick.Flickable.boundsBehavior
    Scenario: A drag past the end stops at the end
      When the user presses the pointer at 100, 190
      And the user moves the pointer to 100, 170
      And the user moves the pointer to 100, 160
      And the user moves the pointer to 100, 10
      Then the property "contentY" of "f" is 100
      And the property "atYEnd" of "f" is true

    Scenario: A drag towards the beginning stays at the beginning
      When the user presses the pointer at 100, 50
      And the user moves the pointer to 100, 70
      And the user moves the pointer to 100, 80
      And the user moves the pointer to 100, 150
      Then the property "contentY" of "f" is 0
      And the property "atYBeginning" of "f" is true

    @api-QtQuick.Flickable.draggingHorizontally
    Scenario: Both directions follow a slanted drag
      When the user presses the pointer at 150, 150
      And the user moves the pointer to 130, 130
      And the user moves the pointer to 120, 120
      And the user moves the pointer to 100, 90
      Then the property "contentX" of "f" is 20
      And the property "contentY" of "f" is 30
      And the property "draggingHorizontally" of "f" is true
      And the property "draggingVertically" of "f" is true

    Scenario: A direction that joins late starts from where the pointer is
      When the user presses the pointer at 150, 150
      And the user moves the pointer to 150, 130
      And the user moves the pointer to 150, 120
      And the user moves the pointer to 120, 100
      And the user moves the pointer to 100, 90
      Then the property "contentY" of "f" is 30
      And the property "contentX" of "f" is 20

  Rule: A drag takes the press from what had it

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          Flickable {
            objectName: "f"
            width: 200; height: 200
            contentWidth: 200; contentHeight: 1000
            boundsBehavior: Flickable.StopAtBounds
            MouseArea {
              objectName: "area"
              width: 200; height: 400
              onPressed: root.log += "pressed "
              onReleased: root.log += "released "
              onCanceled: root.log += "canceled "
              onClicked: root.log += "clicked "
            }
          }
        }
        """

    Scenario: The child has the press until the drag begins
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 130
      Then the property "pressed" of "area" is true
      And the property "log" of "root" is "pressed "

    @api-QtQuick.MouseArea.canceled
    Scenario: The child's press is cancelled when the drag begins
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 130
      And the user moves the pointer to 100, 120
      And the user moves the pointer to 100, 100
      Then the property "pressed" of "area" is false
      And the property "log" of "root" is "pressed canceled "
      And the property "contentY" of "f" is 20

    Scenario: A press that became a drag is no click
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 130
      And the user moves the pointer to 100, 120
      And the user moves the pointer to 100, 100
      And 500 milliseconds pass
      And the user releases the pointer at 100, 100
      Then the property "log" of "root" is "pressed canceled "

    Scenario: A press that stays in place is a click
      When the user presses the pointer at 100, 150
      And the user releases the pointer at 100, 150
      Then the property "log" of "root" is "pressed released clicked "
      And the property "contentY" of "f" is 0

    @api-QtQuick.MouseArea.preventStealing
    Scenario: A MouseArea that prevents stealing keeps its press
      When the property "preventStealing" of "area" is set to true
      And the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 130
      And the user moves the pointer to 100, 120
      And the user moves the pointer to 100, 100
      Then the property "pressed" of "area" is true
      And the property "dragging" of "f" is false
      And the property "contentY" of "f" is 0

    @api-QtQuick.Flickable.interactive
    Scenario: A Flickable that is not interactive leaves the press alone
      When the property "interactive" of "f" is set to false
      And the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 130
      And the user moves the pointer to 100, 120
      And the user moves the pointer to 100, 100
      Then the property "pressed" of "area" is true
      And the property "contentY" of "f" is 0

  Rule: A Flickable is in the way of what is behind it

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property int clicks: 0
          MouseArea {
            objectName: "behind"
            width: 200; height: 200
            onClicked: root.clicks++
          }
          Flickable { objectName: "f"; width: 200; height: 200 }
        }
        """

    Scenario: An interactive Flickable takes the press, with or without content
      When the user touches 100, 100
      Then the property "clicks" of "root" is 0

    Scenario: A Flickable that is not interactive lets the press through
      When the property "interactive" of "f" is set to false
      And the user touches 100, 100
      Then the property "clicks" of "root" is 1

  Rule: Flickables inside each other

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 400; height: 400
          Flickable {
            objectName: "outer"
            width: 300; height: 300
            contentWidth: 300; contentHeight: 1000
            boundsBehavior: Flickable.StopAtBounds
            Flickable {
              objectName: "inner"
              width: 200; height: 200
              contentWidth: 800; contentHeight: 200
              boundsBehavior: Flickable.StopAtBounds
            }
          }
        }
        """

    Scenario: A drag along the inner one's direction moves the inner one
      When the user presses the pointer at 150, 100
      And the user moves the pointer to 130, 100
      And the user moves the pointer to 120, 100
      And the user moves the pointer to 100, 100
      Then the property "contentX" of "inner" is 20
      And the property "contentY" of "outer" is 0

    Scenario: A drag across the inner one moves the outer one
      When the user presses the pointer at 100, 150
      And the user moves the pointer to 100, 130
      And the user moves the pointer to 100, 120
      And the user moves the pointer to 100, 100
      Then the property "contentY" of "outer" is 20
      And the property "contentX" of "inner" is 0
