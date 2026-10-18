# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
#   https://doc.qt.io/qt-6/qml-qtquick-wheelevent.html
#   docs/ARCHITECTURE.md (sections 10.3 and 11)
# The Qt oracle cannot send wheel events: its input channel has wheel buttons
# and Qt makes no event of them. So these scenarios run on Ljós only. The
# distances and who gets the wheel were measured on Qt 6.11.2 with Qt Test's
# mouseWheel. Qt eases the content to where a notch sends it over about
# 300 ms, each 16 ms frame leaving 0.69 of the way to go; Ljós does the same,
# and the scenarios let it get there before they look.

@ljos-only
Feature: Scrolling a Flickable with the wheel
  A mouse wheel moves the content of the Flickable under the pointer by
  three lines of text a notch. A touchpad moves it by the pixel.

  Rule: The wheel moves the content

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
            contentWidth: 500; contentHeight: 1000
            onMovementStarted: root.log += "movementStarted "
            onMovementEnded: root.log += "movementEnded "
          }
        }
        """

    @api-QtQuick.Flickable.contentY
    Scenario Outline: A notch is 72 pixels
      When the user turns the wheel <notches> notches <direction> at 100, 100
      And 400 milliseconds pass
      Then the property "contentX" of "f" is <x>
      And the property "contentY" of "f" is <y>

      Examples:
        | notches | direction | x   | y   |
        | 1       | down      | 0   | 72  |
        | 2       | down      | 0   | 144 |
        | 0.5     | down      | 0   | 36  |
        | 1       | right     | 72  | 0   |
        | 3       | right     | 216 | 0   |

    Scenario: A notch glides there, fast first
      When the user turns the wheel 1 notch down at 100, 100
      And 100 milliseconds pass
      Then the property "contentY" of "f" is within 0.5 of 64.86
      And the property "moving" of "f" is true
      When 300 milliseconds pass
      Then the property "contentY" of "f" is 72

    Scenario: A notch that comes while it glides goes on from where it has got to
      When the user turns the wheel 1 notch down at 100, 100
      And 40 milliseconds pass
      And the user turns the wheel 1 notch down at 100, 100
      And 50 milliseconds pass
      Then the property "contentY" of "f" is within 1 of 92.7
      When 400 milliseconds pass
      Then the property "contentY" of "f" is within 1 of 115.4

    Scenario: Content a program moves while it glides stops where it is put
      When the user turns the wheel 1 notch down at 100, 100
      And 50 milliseconds pass
      And the property "contentY" of "f" is set to 500
      Then the property "moving" of "f" is false
      When 400 milliseconds pass
      Then the property "contentY" of "f" is 500

    Scenario: A touchpad does not glide
      When the user scrolls 30 pixels down at 100, 100
      Then the property "contentY" of "f" is 30

    Scenario: Turning back brings the content back
      When the user turns the wheel 3 notches down at 100, 100
      And 400 milliseconds pass
      And the user turns the wheel 1 notch up at 100, 100
      And 400 milliseconds pass
      Then the property "contentY" of "f" is 144

    Scenario Outline: A touchpad scrolls by the pixel
      When the user scrolls <pixels> pixels <direction> at 100, 100
      Then the property "contentX" of "f" is <x>
      And the property "contentY" of "f" is <y>

      Examples:
        | pixels | direction | x  | y  |
        | 30     | down      | 0  | 30 |
        | 1      | down      | 0  | 1  |
        | 45     | right     | 45 | 0  |

    Scenario Outline: The content stops at its bounds
      When the property "contentY" of "f" is set to <from>
      And the user turns the wheel 2 notches <direction> at 100, 100
      And 400 milliseconds pass
      Then the property "contentY" of "f" is <to>

      Examples:
        | from | direction | to  |
        | 30   | up        | 0   |
        | 0    | up        | 0   |
        | 780  | down      | 800 |
        | 800  | down      | 800 |

    Scenario: Content that is past its end is not pushed further
      When the property "contentY" of "f" is set to 900
      And the user turns the wheel 1 notch down at 100, 100
      And 400 milliseconds pass
      Then the property "contentY" of "f" is 900

    Scenario: Content that is past its end can be turned back
      When the property "contentY" of "f" is set to 900
      And the user turns the wheel 1 notch up at 100, 100
      And 400 milliseconds pass
      Then the property "contentY" of "f" is 828

    Scenario: A wheel outside the Flickable does not scroll it
      When the user turns the wheel 1 notch down at 300, 300
      And 400 milliseconds pass
      Then the property "contentY" of "f" is 0

    @api-QtQuick.Flickable.moving @api-QtQuick.Flickable.movingVertically
    Scenario: The content is moving while the wheel turns
      When the user turns the wheel 1 notch down at 100, 100
      Then the property "moving" of "f" is true
      And the property "movingVertically" of "f" is true
      And the property "movingHorizontally" of "f" is false
      And the property "flicking" of "f" is false
      And the property "dragging" of "f" is false

    @api-QtQuick.Flickable.movementStarted @api-QtQuick.Flickable.movementEnded
    Scenario: The movement ends when the wheel has stopped
      When the user turns the wheel 1 notch down at 100, 100
      And the user turns the wheel 1 notch down at 100, 100
      And 1 second passes
      Then the property "moving" of "f" is false
      And the property "log" of "root" is "movementStarted movementEnded "

    Scenario: A wheel that moves nothing starts no movement
      When the user turns the wheel 1 notch up at 100, 100
      And 400 milliseconds pass
      Then the property "moving" of "f" is false
      And the property "log" of "root" is ""

    @api-QtQuick.Flickable.interactive
    Scenario Outline: A Flickable that takes no input is not scrolled
      When the property "<property>" of "f" is set to false
      And the user turns the wheel 1 notch down at 100, 100
      And 400 milliseconds pass
      Then the property "contentY" of "f" is 0

      Examples:
        | property    |
        | interactive |
        | enabled     |

  Rule: Which directions the wheel moves

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

    Scenario: Content that is no wider does not go sideways
      When the user turns the wheel 1 notch right at 100, 100
      And 400 milliseconds pass
      Then the property "contentX" of "f" is 0

    @api-QtQuick.Flickable.flickableDirection
    Scenario: A Flickable told to go sideways only is not scrolled lengthways
      When the property "flickableDirection" of "f" is set to 1
      And the user turns the wheel 1 notch down at 100, 100
      And 400 milliseconds pass
      Then the property "contentY" of "f" is 0

  Rule: What is on top of the content

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
            MouseArea { objectName: "area"; width: 200; height: 1000 }
            Rectangle { y: 300; width: 200; height: 50 }
          }
        }
        """

    Scenario: A MouseArea in the content does not keep the wheel
      When the user turns the wheel 1 notch down at 100, 100
      And 400 milliseconds pass
      Then the property "contentY" of "f" is 72
      And the property "pressed" of "area" is false

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
            Flickable {
              objectName: "inner"
              width: 200; height: 200
              contentWidth: 200; contentHeight: 300
            }
            Flickable {
              objectName: "side"
              x: 200; width: 100; height: 200
              contentWidth: 400; contentHeight: 200
            }
          }
        }
        """

    Scenario: The inner one scrolls first
      When the user turns the wheel 1 notch down at 100, 100
      And 400 milliseconds pass
      Then the property "contentY" of "inner" is 72
      And the property "contentY" of "outer" is 0

    Scenario: At its end the inner one passes the wheel on
      When the property "contentY" of "inner" is set to 100
      And the user turns the wheel 1 notch down at 100, 100
      And 400 milliseconds pass
      Then the property "contentY" of "inner" is 100
      And the property "contentY" of "outer" is 72

    Scenario: An inner one that only goes sideways passes a lengthways wheel on
      When the user turns the wheel 1 notch down at 250, 100
      And 400 milliseconds pass
      Then the property "contentX" of "side" is 0
      And the property "contentY" of "outer" is 72

    Scenario: With both at the beginning nothing moves
      When the user turns the wheel 1 notch up at 100, 100
      And 400 milliseconds pass
      Then the property "contentY" of "inner" is 0
      And the property "contentY" of "outer" is 0
      And the property "moving" of "outer" is false
