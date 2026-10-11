# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollindicator.html
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
# Verified on Qt 6.11.2 (qml6, offscreen and vnc).

@qt-parity
Feature: ScrollIndicator
  A scroll indicator shows which part of something larger is in view, as
  a scroll bar does, and leaves the pointer alone.

  Rule: The Basic style

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          ScrollIndicator { objectName: "bare" }
          ScrollIndicator { objectName: "tall"; x: 100; height: 200; size: 0.25 }
          ScrollIndicator {
            objectName: "wide"
            y: 220; width: 200; size: 0.5; position: 0.25
            orientation: Qt.Horizontal
          }
        }
        """

    @api-QtQuick.Controls.ScrollIndicator.position @api-QtQuick.Controls.ScrollIndicator.size
    @api-QtQuick.Controls.ScrollIndicator.active
    @api-QtQuick.Controls.ScrollIndicator.orientation
    @api-QtQuick.Controls.ScrollIndicator.minimumSize
    Scenario Outline: An indicator starts out upright, at the beginning and idle
      Then the property "<property>" of "bare" is <value>

      Examples:
        | property              | value |
        | implicitWidth         | 6     |
        | implicitHeight        | 6     |
        | padding               | 2     |
        | implicitContentWidth  | 2     |
        | implicitContentHeight | 2     |
        | position              | 0     |
        | size                  | 0     |
        | active                | false |
        | orientation           | 2     |
        | vertical              | true  |
        | horizontal            | false |
        | minimumSize           | 0     |
        | visible               | true  |

    Scenario Outline: The mark of an upright indicator is as long as the share in view
      Then the property "<property>" of "tall" is <value>

      Examples:
        | property            | value     |
        | width               | 6         |
        | availableHeight     | 196       |
        | contentItem.x       | 2         |
        | contentItem.y       | 2         |
        | contentItem.width   | 2         |
        | contentItem.height  | 49        |
        | contentItem.color   | "#bdbdbd" |
        | contentItem.opacity | 0         |

    @api-QtQuick.Controls.ScrollIndicator.horizontal
    @api-QtQuick.Controls.ScrollIndicator.vertical
    Scenario Outline: The mark of a level indicator lies along it
      Then the property "<property>" of "wide" is <value>

      Examples:
        | property           | value |
        | orientation        | 1     |
        | horizontal         | true  |
        | vertical           | false |
        | height             | 6     |
        | contentItem.x      | 51    |
        | contentItem.y      | 2     |
        | contentItem.width  | 98    |
        | contentItem.height | 2     |

    Scenario: The mark moves along the indicator with the position
      When the property "position" of "tall" is set to 0.5
      Then the property "contentItem.y" of "tall" is 100
      And the property "contentItem.height" of "tall" is 49

    @api-QtQuick.Controls.ScrollIndicator.size
    Scenario Outline: An indicator keeps the size it is given and shows what fits
      When the property "size" of "tall" is set to <given>
      Then the property "size" of "tall" is <given>
      And the property "visualSize" of "tall" is <shown>

      Examples:
        | given | shown |
        | 0.5   | 0.5   |
        | 1.5   | 1     |
        | -0.5  | 0     |

    Scenario: A mark that a new size would take past the end is moved back
      When the property "position" of "tall" is set to 0.9
      And the property "size" of "tall" is set to 0.5
      Then the property "position" of "tall" is 0.5
      When the property "size" of "tall" is set to 1.5
      Then the property "position" of "tall" is -0.5

    @api-QtQuick.Controls.ScrollIndicator.visualPosition
    @api-QtQuick.Controls.ScrollIndicator.visualSize
    Scenario Outline: What shows is the part of the mark that is on the indicator
      When the property "position" of "tall" is set to <position>
      Then the property "visualPosition" of "tall" is within 0.0001 of <at>
      And the property "visualSize" of "tall" is within 0.0001 of <long>

      Examples:
        | position | at   | long |
        | 0        | 0    | 0.25 |
        | 0.5      | 0.5  | 0.25 |
        | 0.9      | 0.9  | 0.1  |
        | -0.1     | 0    | 0.15 |

    @api-QtQuick.Controls.ScrollIndicator.minimumSize
    Scenario: A mark shorter than the minimum is shown at the minimum
      When the property "size" of "tall" is set to 0.2
      And the property "minimumSize" of "tall" is set to 0.4
      And the property "position" of "tall" is set to 0.4
      Then the property "visualSize" of "tall" is within 0.0001 of 0.4
      And the property "visualPosition" of "tall" is within 0.0001 of 0.3

    @api-QtQuick.Controls.ScrollIndicator.active
    Scenario: An active indicator shows its mark at once
      When the property "active" of "tall" is set to true
      Then the property "contentItem.opacity" of "tall" is 0.75

    Scenario: An indicator that is no longer active fades its mark out after a while
      When the property "active" of "tall" is set to true
      And the property "active" of "tall" is set to false
      And 2 seconds pass
      Then the property "contentItem.opacity" of "tall" is 0

    Scenario: The pointer does not make an indicator active
      When the user moves the pointer to 103, 100
      Then the property "active" of "tall" is false

  Rule: Attached to a Flickable

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 300; height: 250
          property int presses: 0
          Flickable {
            objectName: "f"
            width: 150; height: 100
            contentWidth: 300; contentHeight: 400
            boundsBehavior: Flickable.StopAtBounds
            MouseArea { width: 300; height: 400; onPressed: root.presses++ }
            ScrollIndicator.vertical: ScrollIndicator { objectName: "v" }
            ScrollIndicator.horizontal: ScrollIndicator { objectName: "h" }
          }
        }
        """

    @api-QtQuick.Controls.ScrollIndicator.vertical
    Scenario Outline: The vertical indicator runs down the right edge of the Flickable
      Then the property "<property>" of "v" is <value>

      Examples:
        | property          | value |
        | parent.objectName | "f"   |
        | x                 | 144   |
        | y                 | 0     |
        | width             | 6     |
        | height            | 100   |
        | orientation       | 2     |
        | size              | 0.25  |
        | position          | 0     |
        | active            | false |

    @api-QtQuick.Controls.ScrollIndicator.horizontal
    Scenario Outline: The horizontal indicator runs along the bottom edge
      Then the property "<property>" of "h" is <value>

      Examples:
        | property          | value |
        | parent.objectName | "f"   |
        | x                 | 0     |
        | y                 | 94    |
        | width             | 150   |
        | height            | 6     |
        | orientation       | 1     |
        | size              | 0.5   |
        | position          | 0     |

    Scenario: The indicators follow the size of the Flickable
      When the property "width" of "f" is set to 200
      And the property "height" of "f" is set to 200
      Then the property "x" of "v" is 194
      And the property "height" of "v" is 200
      And the property "size" of "v" is 0.5
      And the property "y" of "h" is 194
      And the property "width" of "h" is 200

    Scenario: The indicators follow the content
      When the property "contentY" of "f" is set to 100
      And the property "contentX" of "f" is set to 75
      Then the property "position" of "v" is 0.25
      And the property "position" of "h" is 0.25

    Scenario: The content does not follow an indicator
      When the property "position" of "v" is set to 0.5
      Then the property "contentY" of "f" is 0

    Scenario: A press on an indicator goes to what is under it
      When the user presses the pointer at 147, 50
      Then the property "presses" of "root" is 1

    @api-QtQuick.Controls.ScrollIndicator.active
    Scenario: An indicator is active while the content moves its way
      When the user presses the pointer at 60, 80
      And the user moves the pointer to 60, 60
      And the user moves the pointer to 60, 50
      And the user moves the pointer to 60, 30
      Then the property "moving" of "f" is true
      And the property "active" of "v" is true
      And the property "active" of "h" is false
      And the property "contentItem.opacity" of "v" is 0.75
      When 500 milliseconds pass
      And the user releases the pointer at 60, 30
      And 100 milliseconds pass
      Then the property "active" of "v" is false
