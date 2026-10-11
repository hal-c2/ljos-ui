# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
# Verified on Qt 6.11.2 (qml6, offscreen and vnc).

@qt-parity
Feature: ScrollBar
  A scroll bar shows which part of something larger is in view, and moves
  it: its handle can be dragged and its track pressed. Attached to a
  Flickable it follows the content and the content follows it.

  Rule: The Basic style

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          ScrollBar { objectName: "bar" }
          ScrollBar { objectName: "tall"; x: 100; height: 200; size: 0.25 }
          ScrollBar {
            objectName: "wide"
            y: 220; width: 200; size: 0.5; position: 0.25
            orientation: Qt.Horizontal
          }
        }
        """

    @api-QtQuick.Controls.ScrollBar.position @api-QtQuick.Controls.ScrollBar.size
    @api-QtQuick.Controls.ScrollBar.active @api-QtQuick.Controls.ScrollBar.pressed
    @api-QtQuick.Controls.ScrollBar.policy @api-QtQuick.Controls.ScrollBar.orientation
    @api-QtQuick.Controls.ScrollBar.stepSize @api-QtQuick.Controls.ScrollBar.snapMode
    @api-QtQuick.Controls.ScrollBar.interactive
    Scenario Outline: A scroll bar starts out upright, at the beginning and idle
      Then the property "<property>" of "bar" is <value>

      Examples:
        | property              | value |
        | implicitWidth         | 10    |
        | implicitHeight        | 10    |
        | padding               | 2     |
        | implicitContentWidth  | 6     |
        | implicitContentHeight | 6     |
        | position              | 0     |
        | size                  | 0     |
        | active                | false |
        | pressed               | false |
        | policy                | 0     |
        | orientation           | 2     |
        | vertical              | true  |
        | horizontal            | false |
        | stepSize              | 0     |
        | snapMode              | 0     |
        | interactive           | true  |
        | visible               | true  |
        | hoverEnabled          | true  |
        | wheelEnabled          | false |
        | focusPolicy           | 0     |

    Scenario Outline: The handle of an upright bar is as long as the share in view
      Then the property "<property>" of "tall" is <value>

      Examples:
        | property           | value     |
        | width              | 10        |
        | availableHeight    | 196       |
        | contentItem.x      | 2         |
        | contentItem.y      | 2         |
        | contentItem.width  | 6         |
        | contentItem.height | 49        |
        | contentItem.radius | 3         |
        | contentItem.color  | "#bdbdbd" |

    @api-QtQuick.Controls.ScrollBar.horizontal @api-QtQuick.Controls.ScrollBar.vertical
    Scenario Outline: The handle of a level bar lies along it
      Then the property "<property>" of "wide" is <value>

      Examples:
        | property           | value |
        | orientation        | 1     |
        | horizontal         | true  |
        | vertical           | false |
        | height             | 10    |
        | contentItem.x      | 51    |
        | contentItem.y      | 2     |
        | contentItem.width  | 98    |
        | contentItem.height | 6     |

    Scenario: The handle moves along the bar with the position
      When the property "position" of "tall" is set to 0.5
      Then the property "contentItem.y" of "tall" is 100
      And the property "contentItem.height" of "tall" is 49

    @api-QtQuick.Controls.ScrollBar.minimumSize
    Scenario Outline: The Basic style keeps the handle at least as long as the bar is thick
      Then the property "minimumSize" of "<bar>" is <value>

      Examples:
        | bar  | value |
        | tall | 0.05  |
        | wide | 0.05  |

    Scenario: A bar that takes no pointer is thinner
      When the property "interactive" of "tall" is set to false
      Then the property "implicitWidth" of "tall" is 6
      And the property "contentItem.implicitWidth" of "tall" is 2

  Rule: Position and size

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          ScrollBar {
            objectName: "bar"
            x: 100; height: 200; size: 0.25
            property string log: ""
            onPositionChanged: log += "position "
            onSizeChanged: log += "size "
          }
        }
        """

    @api-QtQuick.Controls.ScrollBar.size
    Scenario Outline: The size is a share between nothing and all
      When the property "size" of "bar" is set to <given>
      Then the property "size" of "bar" is <kept>

      Examples:
        | given | kept |
        | 0.5   | 0.5  |
        | 1     | 1    |
        | 1.5   | 1    |
        | -0.5  | 0    |

    @api-QtQuick.Controls.ScrollBar.position
    Scenario Outline: An assigned position is kept as it is
      When the property "position" of "bar" is set to <given>
      Then the property "position" of "bar" is <given>
      And the property "log" of "bar" is "position "

      Examples:
        | given |
        | 0.5   |
        | 0.75  |
        | 0.9   |
        | -0.1  |

    Scenario: A handle that a new size would take past the end is moved back
      When the property "position" of "bar" is set to 0.9
      And the property "size" of "bar" is set to 0.5
      Then the property "position" of "bar" is 0.5
      When the property "size" of "bar" is set to 0.2
      Then the property "position" of "bar" is 0.5

    @api-QtQuick.Controls.ScrollBar.visualPosition @api-QtQuick.Controls.ScrollBar.visualSize
    Scenario Outline: What shows is the part of the handle that is on the bar
      When the property "position" of "bar" is set to <position>
      Then the property "visualPosition" of "bar" is within 0.0001 of <at>
      And the property "visualSize" of "bar" is within 0.0001 of <long>

      Examples:
        | position | at   | long |
        | 0        | 0    | 0.25 |
        | 0.5      | 0.5  | 0.25 |
        | 0.75     | 0.75 | 0.25 |
        | 0.9      | 0.9  | 0.1  |
        | 0.99     | 0.95 | 0.05 |
        | -0.1     | 0    | 0.15 |

    @api-QtQuick.Controls.ScrollBar.minimumSize
    Scenario Outline: A handle shorter than the minimum is shown at the minimum
      When the property "size" of "bar" is set to 0.2
      And the property "minimumSize" of "bar" is set to 0.4
      And the property "position" of "bar" is set to <position>
      Then the property "visualSize" of "bar" is within 0.0001 of 0.4
      And the property "visualPosition" of "bar" is within 0.0001 of <at>
      And the property "size" of "bar" is 0.2

      Examples:
        | position | at  |
        | 0        | 0   |
        | 0.4      | 0.3 |
        | 0.8      | 0.6 |

    @api-QtQuick.Controls.ScrollBar.increase @api-QtQuick.Controls.ScrollBar.decrease
    Scenario: increase() and decrease() step by a tenth
      When the method "increase" of "bar" is called
      And the method "increase" of "bar" is called
      Then the property "position" of "bar" is within 0.0001 of 0.2
      When the method "decrease" of "bar" is called
      Then the property "position" of "bar" is within 0.0001 of 0.1

    @api-QtQuick.Controls.ScrollBar.stepSize
    Scenario: A step size replaces the tenth
      When the property "stepSize" of "bar" is set to 0.25
      And the method "increase" of "bar" is called
      Then the property "position" of "bar" is 0.25

    Scenario: Steps stop where the handle reaches the ends
      When the property "stepSize" of "bar" is set to 0.5
      And the method "increase" of "bar" is called
      And the method "increase" of "bar" is called
      Then the property "position" of "bar" is 0.75
      When the method "decrease" of "bar" is called
      And the method "decrease" of "bar" is called
      Then the property "position" of "bar" is 0

  Rule: The pointer

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          ScrollBar { objectName: "bar"; x: 100; height: 200; size: 0.25 }
        }
        """

    @api-QtQuick.Controls.ScrollBar.active
    Scenario: A bar under the pointer is active
      When the user moves the pointer to 105, 100
      Then the property "hovered" of "bar" is true
      And the property "active" of "bar" is true
      When the user moves the pointer to 200, 100
      Then the property "hovered" of "bar" is false
      And the property "active" of "bar" is false

    @api-QtQuick.Controls.ScrollBar.pressed
    Scenario: A press on the handle holds it where it is
      When the user presses the pointer at 105, 20
      Then the property "pressed" of "bar" is true
      And the property "active" of "bar" is true
      And the property "position" of "bar" is 0
      And the property "contentItem.color" of "bar" is "#353637"

    Scenario: The held handle follows the pointer
      When the user presses the pointer at 105, 20
      And the user moves the pointer to 105, 69
      Then the property "position" of "bar" is within 0.0001 of 0.25
      When the user moves the pointer to 105, 118
      Then the property "position" of "bar" is within 0.0001 of 0.5

    Scenario: The handle follows a pointer that left the bar
      When the user presses the pointer at 105, 20
      And the user moves the pointer to 200, 69
      Then the property "position" of "bar" is within 0.0001 of 0.25
      And the property "pressed" of "bar" is true

    Scenario: The handle stops at the ends of the bar
      When the user presses the pointer at 105, 20
      And the user moves the pointer to 105, 240
      Then the property "position" of "bar" is 0.75
      When the user moves the pointer to 105, 5
      Then the property "position" of "bar" is 0

    Scenario: Letting go leaves the handle where it is
      When the user presses the pointer at 105, 20
      And the user moves the pointer to 105, 69
      And the user releases the pointer at 105, 69
      Then the property "pressed" of "bar" is false
      And the property "position" of "bar" is within 0.0001 of 0.25

    Scenario: A press on the track puts the middle of the handle there
      When the user presses the pointer at 105, 100
      Then the property "position" of "bar" is within 0.0001 of 0.375
      And the property "pressed" of "bar" is true

    Scenario Outline: A press near an end of the track puts the handle at that end
      When the user presses the pointer at 105, <y>
      Then the property "position" of "bar" is <position>

      Examples:
        | y   | position |
        | 195 | 0.75     |
        | 190 | 0.75     |

    @api-QtQuick.Controls.ScrollBar.interactive
    Scenario: A bar that is not interactive leaves the pointer alone
      When the property "interactive" of "bar" is set to false
      And the user presses the pointer at 105, 100
      Then the property "pressed" of "bar" is false
      And the property "position" of "bar" is 0

    Scenario: A bar that is not enabled leaves the pointer alone
      When the property "enabled" of "bar" is set to false
      And the user presses the pointer at 105, 100
      Then the property "pressed" of "bar" is false
      And the property "position" of "bar" is 0

  Rule: Snapping to steps

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          ScrollBar {
            objectName: "bar"
            x: 100; height: 200; size: 0.25; stepSize: 0.2
          }
        }
        """

    @api-QtQuick.Controls.ScrollBar.snapMode
    Scenario: By default the handle does not snap
      When the user presses the pointer at 105, 110
      And the user releases the pointer at 105, 110
      Then the property "position" of "bar" is within 0.001 of 0.426

    Scenario: A bar that always snaps puts a pressed handle on a step
      When the property "snapMode" of "bar" is set to 1
      And the user presses the pointer at 105, 110
      Then the property "position" of "bar" is within 0.0001 of 0.45

    Scenario: The step the handle snaps to is the nearest one
      When the property "snapMode" of "bar" is set to 1
      And the user presses the pointer at 105, 66
      Then the property "position" of "bar" is within 0.0001 of 0.15

    Scenario: A bar that always snaps drags its handle from step to step
      When the property "snapMode" of "bar" is set to 1
      And the user presses the pointer at 105, 20
      And the user moves the pointer to 105, 40
      Then the property "position" of "bar" is within 0.0001 of 0.15
      When the user moves the pointer to 105, 75
      Then the property "position" of "bar" is within 0.0001 of 0.3

    Scenario: A bar that snaps on release moves freely until it is let go
      When the property "snapMode" of "bar" is set to 2
      And the user presses the pointer at 105, 110
      Then the property "position" of "bar" is within 0.001 of 0.426
      When the user releases the pointer at 105, 110
      Then the property "position" of "bar" is within 0.0001 of 0.45

  Rule: When the handle shows

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          ScrollBar { objectName: "bar"; x: 100; height: 200; size: 0.25 }
          ScrollBar {
            objectName: "on"
            x: 150; height: 200; size: 0.25
            policy: ScrollBar.AlwaysOn
          }
          ScrollBar {
            objectName: "off"
            x: 200; height: 200; size: 0.25
            policy: ScrollBar.AlwaysOff
          }
        }
        """

    @api-QtQuick.Controls.ScrollBar.policy
    Scenario Outline: The policy says whether there is a bar at all
      Then the property "policy" of "<bar>" is <policy>
      And the property "visible" of "<bar>" is <visible>
      And the property "contentItem.opacity" of "<bar>" is <opacity>

      Examples:
        | bar | policy | visible | opacity |
        | bar | 0      | true    | 0       |
        | on  | 2      | true    | 0.75    |
        | off | 1      | false   | 0       |

    Scenario: A bar that is switched off comes back when it is switched on
      When the property "policy" of "off" is set to 2
      Then the property "visible" of "off" is true
      And the property "contentItem.opacity" of "off" is 0.75

    @api-QtQuick.Controls.ScrollBar.active
    Scenario: An active bar shows its handle at once
      When the property "active" of "bar" is set to true
      Then the property "contentItem.opacity" of "bar" is 0.75

    Scenario: A bar that is no longer active fades its handle out after a while
      When the property "active" of "bar" is set to true
      And the property "active" of "bar" is set to false
      And 2 seconds pass
      Then the property "contentItem.opacity" of "bar" is 0

    Scenario: A handle that covers the whole bar is not shown
      When the property "size" of "bar" is set to 1
      And the property "active" of "bar" is set to true
      Then the property "contentItem.opacity" of "bar" is 0

  Rule: Attached to a Flickable

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          Flickable {
            objectName: "f"
            width: 150; height: 100
            contentWidth: 300; contentHeight: 400
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { objectName: "v" }
            ScrollBar.horizontal: ScrollBar { objectName: "h" }
          }
        }
        """

    @api-QtQuick.Controls.ScrollBar.vertical
    Scenario Outline: The vertical bar runs down the right edge of the Flickable
      Then the property "<property>" of "v" is <value>

      Examples:
        | property          | value |
        | parent.objectName | "f"   |
        | x                 | 140   |
        | y                 | 0     |
        | width             | 10    |
        | height            | 100   |
        | orientation       | 2     |
        | size              | 0.25  |
        | position          | 0     |

    @api-QtQuick.Controls.ScrollBar.horizontal
    Scenario Outline: The horizontal bar runs along the bottom edge
      Then the property "<property>" of "h" is <value>

      Examples:
        | property          | value |
        | parent.objectName | "f"   |
        | x                 | 0     |
        | y                 | 90    |
        | width             | 150   |
        | height            | 10    |
        | orientation       | 1     |
        | size              | 0.5   |
        | position          | 0     |

    Scenario: The bars follow the size of the Flickable
      When the property "width" of "f" is set to 200
      And the property "height" of "f" is set to 200
      Then the property "x" of "v" is 190
      And the property "height" of "v" is 200
      And the property "size" of "v" is 0.5
      And the property "y" of "h" is 190
      And the property "width" of "h" is 200

    Scenario: The bars follow the content
      When the property "contentY" of "f" is set to 100
      And the property "contentX" of "f" is set to 75
      Then the property "position" of "v" is 0.25
      And the property "position" of "h" is 0.25

    Scenario: The bars follow the size of the content
      When the property "contentHeight" of "f" is set to 200
      Then the property "size" of "v" is 0.5

    Scenario: The content follows a bar that is given a position
      When the property "position" of "v" is set to 0.5
      And the property "position" of "h" is set to 0.25
      Then the property "contentY" of "f" is 200
      And the property "contentX" of "f" is 75

    Scenario: The content follows a bar that is stepped
      When the method "increase" of "v" is called
      Then the property "contentY" of "f" is within 0.001 of 40

    Scenario: Dragging the handle scrolls the content and does not drag it
      When the user presses the pointer at 145, 10
      And the user moves the pointer to 145, 34
      Then the property "position" of "v" is within 0.0001 of 0.25
      And the property "contentY" of "f" is within 0.001 of 100
      And the property "dragging" of "f" is false
      And the property "pressed" of "v" is true

    Scenario: A press on the track scrolls the content there
      When the user presses the pointer at 145, 50
      Then the property "position" of "v" is within 0.0001 of 0.375
      And the property "contentY" of "f" is within 0.001 of 150

    @api-QtQuick.Controls.ScrollBar.active
    Scenario: A bar is active while the content moves its way
      When the user presses the pointer at 60, 80
      And the user moves the pointer to 60, 60
      And the user moves the pointer to 60, 50
      And the user moves the pointer to 60, 30
      Then the property "moving" of "f" is true
      And the property "active" of "v" is true
      And the property "active" of "h" is false
      When 500 milliseconds pass
      And the user releases the pointer at 60, 30
      And 100 milliseconds pass
      Then the property "active" of "v" is false

    Scenario: A bar of content that fits covers the whole track
      When the property "contentHeight" of "f" is set to 100
      Then the property "size" of "v" is 1
      And the property "position" of "v" is 0
