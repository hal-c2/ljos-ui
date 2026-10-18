# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
# Verified on Qt 6.11.2 (qml6, vnc; and qmltestrunner for the distances).
# A flung Flickable slows down evenly by flickDeceleration: it goes
# v * v / (2 * flickDeceleration) and takes v / flickDeceleration. Past a
# bound it brakes eight times as hard, at most 150 pixels out, and comes
# back in 400 milliseconds. The distances below are the same on any
# machine; what depends on how fast a pointer moved is asked loosely.

Feature: Flinging a Flickable
  Content that is flung keeps moving, slows down evenly and comes to rest.
  Flung or dragged past its bounds it gives way, and comes back.

  @qt-parity
  Rule: A flick slows down evenly

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
            contentWidth: 200; contentHeight: 5000
            onMovementStarted: root.log += "movementStarted "
            onMovementEnded: root.log += "movementEnded "
            onFlickStarted: root.log += "flickStarted "
            onFlickEnded: root.log += "flickEnded "
          }
        }
        """

    @api-QtQuick.Flickable.flick @api-QtQuick.Flickable.flickDeceleration
    Scenario: A flick goes as far as its velocity and the deceleration say
      When the method "flick" of "f" is called with 0, -800
      And 2 seconds pass
      Then the property "contentY" of "f" is within 0.5 of 213.33
      And the property "flicking" of "f" is false
      And the property "verticalVelocity" of "f" is 0

    Scenario: A harder deceleration stops it sooner
      When the property "flickDeceleration" of "f" is set to 3000
      And the method "flick" of "f" is called with 0, -800
      And 2 seconds pass
      Then the property "contentY" of "f" is within 0.5 of 106.67

    @api-QtQuick.Flickable.maximumFlickVelocity
    Scenario: A flick is no faster than the maximum velocity
      When the property "maximumFlickVelocity" of "f" is set to 1000
      And the method "flick" of "f" is called with 0, -5000
      And 2 seconds pass
      Then the property "contentY" of "f" is within 0.5 of 333.33

    @api-QtQuick.Flickable.horizontalVelocity
    Scenario: A positive velocity moves the content towards its beginning
      When the property "contentY" of "f" is set to 1000
      And the method "flick" of "f" is called with 0, 800
      And 2 seconds pass
      Then the property "contentY" of "f" is within 0.5 of 786.67
      And the property "horizontalVelocity" of "f" is 0

    @api-QtQuick.Flickable.movementStarted @api-QtQuick.Flickable.movementEnded
    Scenario: A flick moves, and ends after its flick has
      When the method "flick" of "f" is called with 0, -800
      And 2 seconds pass
      Then the property "log" of "root" is "movementStarted flickStarted flickEnded movementEnded "
      And the property "moving" of "f" is false

    @api-QtQuick.Flickable.interactive
    Scenario: A Flickable that is not interactive is still flicked by flick()
      When the property "interactive" of "f" is set to false
      And the method "flick" of "f" is called with 0, -800
      Then the property "flicking" of "f" is true

    Scenario: Setting interactive to false does not stop a flick
      When the method "flick" of "f" is called with 0, -800
      And the property "interactive" of "f" is set to false
      And 2 seconds pass
      Then the property "contentY" of "f" is within 0.5 of 213.33

  @qt-parity
  Rule: A press catches a flick

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          property real stoppedAt: -1
          readonly property bool held: f.contentY === stoppedAt
          Flickable {
            id: f
            objectName: "f"
            width: 200; height: 200
            contentWidth: 200; contentHeight: 5000
            onMovementStarted: root.log += "movementStarted "
            onMovementEnded: root.log += "movementEnded "
            onFlickStarted: root.log += "flickStarted "
            onFlickEnded: root.log += "flickEnded "
            onFlickingChanged: if (!flicking) root.stoppedAt = contentY
            MouseArea {
              width: 200; height: 5000
              onPressed: root.log += "pressed "
              onClicked: root.log += "clicked "
            }
          }
        }
        """

    @api-QtQuick.Flickable.cancelFlick
    Scenario: A press on flung content stops it there
      When the method "flick" of "f" is called with 0, -1500
      And 100 milliseconds pass
      And the user presses the pointer at 100, 100
      And 200 milliseconds pass
      Then the property "flicking" of "f" is false
      And the property "held" of "root" is true

    Scenario: The press that catches a flick is no click
      When the method "flick" of "f" is called with 0, -1500
      And 100 milliseconds pass
      And the user presses the pointer at 100, 100
      And the user releases the pointer at 100, 100
      Then the property "log" of "root" is "movementStarted flickStarted flickEnded movementEnded "

    Scenario: Caught content is still moving until the press ends
      When the method "flick" of "f" is called with 0, -1500
      And 100 milliseconds pass
      And the user presses the pointer at 100, 100
      Then the property "moving" of "f" is true
      And the property "log" of "root" is "movementStarted flickStarted "

  @qt-parity
  Rule: Flung past a bound, the content comes back

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          property real furthest: 0
          Flickable {
            objectName: "f"
            width: 200; height: 200
            contentWidth: 200; contentHeight: 5000
            onFlickStarted: root.log += "flickStarted "
            onFlickEnded: root.log += "flickEnded "
            onVerticalOvershootChanged: root.furthest = Math.max(root.furthest, verticalOvershoot)
          }
        }
        """

    @api-QtQuick.Flickable.verticalOvershoot
    Scenario: A flick towards the beginning goes past it for a moment
      When the method "flick" of "f" is called with 0, 800
      And 50 milliseconds pass
      Then the property "contentY" of "f" is at most -1
      And the property "verticalOvershoot" of "f" is at most -1

    Scenario: It comes back to the beginning, and the flick ends there
      When the method "flick" of "f" is called with 0, 800
      And 2 seconds pass
      Then the property "contentY" of "f" is 0
      And the property "verticalOvershoot" of "f" is 0
      And the property "log" of "root" is "flickStarted flickEnded "

    Scenario: A fast flick goes no further than 150 pixels past the end
      When the property "contentY" of "f" is set to 4700
      And the method "flick" of "f" is called with 0, -2500
      And 2 seconds pass
      Then the property "furthest" of "root" is at least 140
      And the property "furthest" of "root" is at most 150
      And the property "contentY" of "f" is 4800
      And the property "log" of "root" is "flickStarted flickEnded "

    Scenario: cancelFlick() leaves content past its bound where it is
      When the property "contentY" of "f" is set to 4700
      And the method "flick" of "f" is called with 0, -2000
      And 120 milliseconds pass
      And the method "cancelFlick" of "f" is called
      And 600 milliseconds pass
      Then the property "contentY" of "f" is at least 4801
      And the property "flicking" of "f" is false
      And the property "moving" of "f" is false

    @api-QtQuick.Flickable.boundsBehavior
    Scenario: With StopAtBounds a flick stops at the bound
      When the property "boundsBehavior" of "f" is set to 0
      And the method "flick" of "f" is called with 0, 800
      And 50 milliseconds pass
      Then the property "contentY" of "f" is 0
      And the property "verticalOvershoot" of "f" is 0

    @api-QtQuick.Flickable.boundsMovement
    Scenario: With boundsMovement StopAtBounds the content stays and the overshoot says how far
      When the property "boundsMovement" of "f" is set to 0
      And the method "flick" of "f" is called with 0, 800
      And 50 milliseconds pass
      Then the property "contentY" of "f" is 0
      And the property "verticalOvershoot" of "f" is at most -1

  @qt-parity
  Rule: Dragged past a bound, the content gives way and comes back

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
            onFlickStarted: root.log += "flickStarted "
            onFlickEnded: root.log += "flickEnded "
            onDragEnded: root.log += "dragEnded "
          }
        }
        """

    Scenario: Past the beginning the content follows half the pointer
      When the user presses the pointer at 100, 50
      And the user moves the pointer to 100, 70
      And the user moves the pointer to 100, 80
      And the user moves the pointer to 100, 180
      Then the property "contentY" of "f" is within 1 of -50
      And the property "verticalOvershoot" of "f" is within 1 of -50

    Scenario: With boundsMovement StopAtBounds a drag reports the overshoot only
      When the property "boundsMovement" of "f" is set to 0
      And the user presses the pointer at 100, 50
      And the user moves the pointer to 100, 70
      And the user moves the pointer to 100, 80
      And the user moves the pointer to 100, 180
      Then the property "contentY" of "f" is 0
      And the property "verticalOvershoot" of "f" is within 1 of -50

    Scenario: With DragOverBounds a drag gives way
      When the property "boundsBehavior" of "f" is set to 1
      And the user presses the pointer at 100, 50
      And the user moves the pointer to 100, 70
      And the user moves the pointer to 100, 80
      And the user moves the pointer to 100, 180
      Then the property "contentY" of "f" is within 1 of -50

    Scenario: With OvershootBounds a drag stops at the bound
      When the property "boundsBehavior" of "f" is set to 2
      And the user presses the pointer at 100, 50
      And the user moves the pointer to 100, 70
      And the user moves the pointer to 100, 80
      And the user moves the pointer to 100, 180
      Then the property "contentY" of "f" is 0
      And the property "verticalOvershoot" of "f" is 0

    # Whether qml6 reports the return as a flick (flickStarted, flickEnded)
    # varies from run to run, so only where it ends is checked.
    Scenario: Let go past the bound, it comes back
      When the user presses the pointer at 100, 50
      And the user moves the pointer to 100, 70
      And the user moves the pointer to 100, 80
      And the user moves the pointer to 100, 180
      And 300 milliseconds pass
      And the user releases the pointer at 100, 180
      And 1 second passes
      Then the property "contentY" of "f" is 0
      And the property "flicking" of "f" is false

  @qt-parity
  Rule: A drag that is let go while it moves is flung

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
            contentWidth: 200; contentHeight: 5000
            onMovementStarted: root.log += "movementStarted "
            onMovementEnded: root.log += "movementEnded "
            onDragStarted: root.log += "dragStarted "
            onDragEnded: root.log += "dragEnded "
            onFlickStarted: root.log += "flickStarted "
            onFlickEnded: root.log += "flickEnded "
          }
        }
        """

    Scenario: The content goes on after the pointer lets go
      When the user presses the pointer at 100, 190
      And the user moves the pointer to 100, 170
      And 16 milliseconds pass
      And the user moves the pointer to 100, 150
      And 16 milliseconds pass
      And the user moves the pointer to 100, 130
      And 16 milliseconds pass
      And the user moves the pointer to 100, 110
      And 16 milliseconds pass
      And the user moves the pointer to 100, 90
      And the user releases the pointer at 100, 90
      And 5 seconds pass
      Then the property "contentY" of "f" is at least 100
      And the property "moving" of "f" is false
      And the property "log" of "root" is "dragStarted movementStarted dragEnded flickStarted flickEnded movementEnded "

    Scenario: A pointer that rested before it let go flings nothing
      When the user presses the pointer at 100, 190
      And the user moves the pointer to 100, 170
      And 16 milliseconds pass
      And the user moves the pointer to 100, 150
      And 16 milliseconds pass
      And the user moves the pointer to 100, 130
      And 16 milliseconds pass
      And the user moves the pointer to 100, 110
      And 16 milliseconds pass
      And the user moves the pointer to 100, 90
      And 300 milliseconds pass
      And the user releases the pointer at 100, 90
      And 1 second passes
      Then the property "contentY" of "f" is 60
      And the property "log" of "root" is "dragStarted movementStarted dragEnded movementEnded "

  # qmltestrunner, at fixed event times: each move's speed is smoothed into
  # the pointer's, 0.7 of the new and 0.3 of the old from rest at the
  # press, and a release throws the mean of the last three. Thrown slower
  # than 75 pixels a second, or after resting a tenth of a second, the
  # content stays. The oracle's clock is too loose for the exact numbers.
  Rule: A flick is as fast as the pointer's last moves

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          property real thrown: 0
          Flickable {
            objectName: "f"
            width: 200; height: 400
            contentWidth: 200; contentHeight: 20000
            onDragStarted: root.log += "dragStarted "
            onFlickStarted: { root.log += "flickStarted "; root.thrown = verticalVelocity }
            onMovementEnded: root.log += "movementEnded "
          }
        }
        """

    @ljos-only
    Scenario: Six moves of 30 pixels 17 milliseconds apart throw 1759 pixels a second
      When the user presses the pointer at 100, 390
      And 17 milliseconds pass
      And the user moves the pointer to 100, 360
      And 17 milliseconds pass
      And the user moves the pointer to 100, 330
      And 17 milliseconds pass
      And the user moves the pointer to 100, 300
      And 17 milliseconds pass
      And the user moves the pointer to 100, 270
      And 17 milliseconds pass
      And the user moves the pointer to 100, 240
      And 17 milliseconds pass
      And the user moves the pointer to 100, 210
      And the user releases the pointer at 100, 210
      Then the property "thrown" of "root" is within 1 of 1758.7
      When 3 seconds pass
      Then the property "contentY" of "f" is within 1 of 1151

    @ljos-only
    Scenario: A slow drag that ends in one fast move is thrown slower than that move
      When the user presses the pointer at 100, 390
      And 17 milliseconds pass
      And the user moves the pointer to 100, 385
      And 17 milliseconds pass
      And the user moves the pointer to 100, 380
      And 17 milliseconds pass
      And the user moves the pointer to 100, 375
      And 17 milliseconds pass
      And the user moves the pointer to 100, 370
      And 17 milliseconds pass
      And the user moves the pointer to 100, 365
      And 17 milliseconds pass
      And the user moves the pointer to 100, 360
      And 17 milliseconds pass
      And the user moves the pointer to 100, 355
      And 17 milliseconds pass
      And the user moves the pointer to 100, 350
      And 17 milliseconds pass
      And the user moves the pointer to 100, 320
      And the user releases the pointer at 100, 320
      Then the property "thrown" of "root" is within 1 of 637.2

    @qt-parity
    Scenario: A pointer that creeps flings nothing
      When the user presses the pointer at 100, 390
      And 50 milliseconds pass
      And the user moves the pointer to 100, 388
      And 50 milliseconds pass
      And the user moves the pointer to 100, 386
      And 50 milliseconds pass
      And the user moves the pointer to 100, 384
      And 50 milliseconds pass
      And the user moves the pointer to 100, 382
      And 50 milliseconds pass
      And the user moves the pointer to 100, 380
      And 50 milliseconds pass
      And the user moves the pointer to 100, 378
      And 50 milliseconds pass
      And the user moves the pointer to 100, 376
      And 50 milliseconds pass
      And the user moves the pointer to 100, 374
      And 50 milliseconds pass
      And the user moves the pointer to 100, 372
      And 50 milliseconds pass
      And the user moves the pointer to 100, 370
      And 50 milliseconds pass
      And the user moves the pointer to 100, 368
      And 50 milliseconds pass
      And the user moves the pointer to 100, 366
      And 50 milliseconds pass
      And the user moves the pointer to 100, 364
      And 50 milliseconds pass
      And the user moves the pointer to 100, 362
      And 50 milliseconds pass
      And the user moves the pointer to 100, 360
      And the user releases the pointer at 100, 360
      And 1 second passes
      Then the property "log" of "root" is "dragStarted movementEnded "

    @qt-parity
    Scenario: One long move is flung when the pointer lets go
      When the user presses the pointer at 100, 390
      And 16 milliseconds pass
      And the user moves the pointer to 100, 350
      And 16 milliseconds pass
      And the user releases the pointer at 100, 350
      And 3 seconds pass
      Then the property "log" of "root" is "flickStarted movementEnded "
      And the property "contentY" of "f" is at least 100

    @qt-parity
    Scenario: One move shorter than twice the drag threshold is not
      When the user presses the pointer at 100, 390
      And 16 milliseconds pass
      And the user moves the pointer to 100, 378
      And 16 milliseconds pass
      And the user releases the pointer at 100, 378
      And 1 second passes
      Then the property "log" of "root" is ""
      And the property "contentY" of "f" is 0
