# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
# Verified on Qt 6.11.2 (qml6, vnc).
# The measured numbers of the same behaviour are in flickable-fling.feature.

@qt-parity
Feature: Flicking a Flickable
  Content that is let go while it moves keeps moving and slows down.
  Dragged past its bounds it gives way, and comes back when it is let go.

  Rule: A flick keeps the content moving

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

    @api-QtQuick.Flickable.flick @api-QtQuick.Flickable.flicking
    @api-QtQuick.Flickable.flickingVertically
    Scenario: flick() sets the content moving
      When the method "flick" of "f" is called with 0, -800
      Then the property "flicking" of "f" is true
      And the property "flickingVertically" of "f" is true
      And the property "flickingHorizontally" of "f" is false
      And the property "moving" of "f" is true
      And the property "dragging" of "f" is false

    @api-QtQuick.Flickable.verticalVelocity
    Scenario: The content has a velocity while it is flicked
      When the method "flick" of "f" is called with 0, -800
      And 100 milliseconds pass
      Then the property "verticalVelocity" of "f" is at least 1
      And the property "contentY" of "f" is at least 1

    @api-QtQuick.Flickable.flickStarted @api-QtQuick.Flickable.flickEnded
    Scenario: A flick comes to rest by itself
      When the method "flick" of "f" is called with 0, -800
      And 5 seconds pass
      Then the property "flicking" of "f" is false
      And the property "moving" of "f" is false
      And the property "verticalVelocity" of "f" is 0
      And the property "contentY" of "f" is at least 50
      And the property "log" of "root" is "movementStarted flickStarted flickEnded movementEnded "

    @api-QtQuick.Flickable.cancelFlick
    Scenario: cancelFlick() stops it where it is
      When the method "flick" of "f" is called with 0, -800
      And the method "cancelFlick" of "f" is called
      Then the property "flicking" of "f" is false
      And the property "moving" of "f" is false

    Scenario: Content a program moves stops where it is put
      When the method "flick" of "f" is called with 0, -2000
      And 100 milliseconds pass
      And the property "contentY" of "f" is set to 500
      Then the property "flicking" of "f" is false
      And the property "moving" of "f" is false
      When 100 milliseconds pass
      Then the property "contentY" of "f" is 500

    # On Ljós the steps of this scenario happen at one instant of the
    # virtual clock, so the pointer has no speed when it is let go; the
    # same drag with time between the moves flicks there
    # (flickable-fling.feature, "The content goes on after the pointer lets go").
    @backlog-headless
    Scenario: A drag that is let go while it moves goes on as a flick
      When the user presses the pointer at 100, 190
      And the user moves the pointer to 100, 160
      And the user moves the pointer to 100, 130
      And the user moves the pointer to 100, 100
      And the user moves the pointer to 100, 70
      And the user releases the pointer at 100, 70
      And 5 seconds pass
      Then the property "contentY" of "f" is at least 100
      And the property "moving" of "f" is false

  Rule: Content gives way past its bounds and comes back

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
            contentWidth: 200; contentHeight: 1000
          }
        }
        """

    @api-QtQuick.Flickable.boundsBehavior
    Scenario: A drag past the beginning pulls the content along
      When the user presses the pointer at 100, 50
      And the user moves the pointer to 100, 80
      And the user moves the pointer to 100, 110
      And the user moves the pointer to 100, 180
      Then the property "contentY" of "f" is at most -1
      And the property "atYBeginning" of "f" is true

    @api-QtQuick.Flickable.verticalOvershoot
    Scenario: How far it was pulled is the overshoot
      When the user presses the pointer at 100, 50
      And the user moves the pointer to 100, 80
      And the user moves the pointer to 100, 110
      And the user moves the pointer to 100, 180
      Then the property "verticalOvershoot" of "f" is at most -1

    Scenario: Let go, the content comes back to its beginning
      When the user presses the pointer at 100, 50
      And the user moves the pointer to 100, 80
      And the user moves the pointer to 100, 110
      And the user moves the pointer to 100, 180
      And 500 milliseconds pass
      And the user releases the pointer at 100, 180
      And 2 seconds pass
      Then the property "contentY" of "f" is 0
      And the property "verticalOvershoot" of "f" is 0
      And the property "moving" of "f" is false

    @api-QtQuick.Flickable.returnToBounds
    Scenario: returnToBounds() takes its time
      When the property "contentY" of "f" is set to 900
      And the method "returnToBounds" of "f" is called
      Then the property "contentY" of "f" is at least 801
