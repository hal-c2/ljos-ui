# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Flickable
  A Flickable shows part of a surface that is larger than itself. Its
  children live on that surface, the content item, which the Flickable
  moves the opposite way of contentX and contentY.

  Rule: A Flickable that was told nothing

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 200; height: 200
          property var directions: [Flickable.AutoFlickDirection,
            Flickable.HorizontalFlick, Flickable.VerticalFlick,
            Flickable.HorizontalAndVerticalFlick,
            Flickable.AutoFlickIfNeeded]
          property var bounds: [Flickable.StopAtBounds,
            Flickable.DragOverBounds, Flickable.OvershootBounds,
            Flickable.DragAndOvershootBounds,
            Flickable.FollowBoundsBehavior]
          Flickable { objectName: "f"; width: 100; height: 80 }
        }
        """

    @api-QtQuick.Flickable.contentX @api-QtQuick.Flickable.contentY
    @api-QtQuick.Flickable.contentWidth @api-QtQuick.Flickable.contentHeight
    @api-QtQuick.Flickable.interactive @api-QtQuick.Flickable.boundsBehavior
    @api-QtQuick.Flickable.flickableDirection @api-QtQuick.Flickable.pressDelay
    Scenario Outline: Defaults
      Then the property "<property>" of "f" is <value>

      Examples:
        | property              | value |
        | contentX              | 0     |
        | contentY              | 0     |
        | contentWidth          | -1    |
        | contentHeight         | -1    |
        | originX               | 0     |
        | originY               | 0     |
        | topMargin             | 0     |
        | bottomMargin          | 0     |
        | leftMargin            | 0     |
        | rightMargin           | 0     |
        | interactive           | true  |
        | flickableDirection    | 0     |
        | boundsBehavior        | 3     |
        | boundsMovement        | 1     |
        | pressDelay            | 0     |
        | flickDeceleration     | 1500  |
        | maximumFlickVelocity  | 2500  |
        | pixelAligned          | false |
        | synchronousDrag       | false |
        | clip                  | false |
        | horizontalVelocity    | 0     |
        | verticalVelocity      | 0     |
        | horizontalOvershoot   | 0     |
        | verticalOvershoot     | 0     |
        | rebound               | null  |

    @api-QtQuick.Flickable.moving @api-QtQuick.Flickable.dragging
    @api-QtQuick.Flickable.flicking @api-QtQuick.Flickable.atYBeginning
    Scenario Outline: It is at rest, at the beginning and at the end
      Then the property "<property>" of "f" is <value>

      Examples:
        | property             | value |
        | moving               | false |
        | movingHorizontally   | false |
        | movingVertically     | false |
        | dragging             | false |
        | draggingHorizontally | false |
        | draggingVertically   | false |
        | flicking             | false |
        | flickingHorizontally | false |
        | flickingVertically   | false |
        | atXBeginning         | true  |
        | atYBeginning         | true  |
        | atXEnd               | true  |
        | atYEnd               | true  |

    Scenario: The names of the directions and of the bounds behaviours
      Then the property "directions" of "root" is [0, 1, 2, 3, 12]
      And the property "bounds" of "root" is [0, 1, 2, 3, 1]

    @api-QtQuick.Flickable.contentItem
    Scenario: Without a content size the content item is as large as the Flickable
      Then the property "contentItem.width" of "f" is 100
      And the property "contentItem.height" of "f" is 80
      When the property "width" of "f" is set to 150
      Then the property "contentItem.width" of "f" is 150

    @api-QtQuick.Flickable.originX @api-QtQuick.Flickable.originY
    Scenario Outline: What the Flickable works out cannot be assigned
      Then setting the property "<property>" of "f" to <value> is rejected

      Examples:
        | property     | value |
        | originY      | 5     |
        | atYEnd       | false |
        | moving       | true  |
        | dragging     | true  |
        | flicking     | true  |

  Rule: The children are on the content item

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 200; height: 200
          property bool onContent: a.parent === f.contentItem
            && b.parent === f.contentItem
          property int ownChildren: f.children.length
          property int contentChildren: f.contentItem.children.length
          Flickable {
            id: f
            objectName: "f"
            width: 100; height: 80
            contentWidth: 300; contentHeight: 400
            Rectangle { id: a; objectName: "a"; width: 300; height: 200 }
            Rectangle { id: b; objectName: "b"; y: 200; width: 300; height: 200 }
          }
        }
        """

    Scenario: A declared child's parent is the content item
      Then the property "onContent" of "root" is true
      And the property "contentChildren" of "root" is 2
      And the property "ownChildren" of "root" is 1

    Scenario: The content item has the content size
      Then the property "contentItem.width" of "f" is 300
      And the property "contentItem.height" of "f" is 400

    Scenario: The content item moves the other way
      When the property "contentX" of "f" is set to 40
      And the property "contentY" of "f" is set to 120
      Then the property "contentItem.x" of "f" is -40
      And the property "contentItem.y" of "f" is -120
      And the property "y" of "b" is 200

    Scenario: A content size smaller than the Flickable is still the content item's
      When the property "contentWidth" of "f" is set to 50
      Then the property "contentItem.width" of "f" is 50
      When the property "contentWidth" of "f" is set to 0
      Then the property "contentItem.width" of "f" is 0

    Scenario: The content size can follow the children
      When the property "contentHeight" of "f" is bound to "contentItem.childrenRect.height"
      And the property "height" of "b" is set to 500
      Then the property "contentHeight" of "f" is 700
      And the property "contentItem.height" of "f" is 700

  Rule: Where the content is

    Background:
      Given the QML:
        """qml
        import QtQuick
        Flickable {
          objectName: "f"
          width: 100; height: 100
          contentWidth: 300; contentHeight: 400
          property string log: ""
          onAtYBeginningChanged: log += "begin:" + atYBeginning + " "
          onAtYEndChanged: log += "end:" + atYEnd + " "
          function grow() { resizeContent(100, 800, Qt.point(0, 50)) }
        }
        """

    @api-QtQuick.Flickable.atXBeginning @api-QtQuick.Flickable.atXEnd
    @api-QtQuick.Flickable.atYBeginning @api-QtQuick.Flickable.atYEnd
    Scenario Outline: The beginning and the end of each direction
      When the property "contentX" of "f" is set to <x>
      And the property "contentY" of "f" is set to <y>
      Then the property "atXBeginning" of "f" is <x begin>
      And the property "atXEnd" of "f" is <x end>
      And the property "atYBeginning" of "f" is <y begin>
      And the property "atYEnd" of "f" is <y end>

      Examples:
        | x   | y   | x begin | x end | y begin | y end |
        | 0   | 0   | true    | false | true    | false |
        | 1   | 1   | false   | false | false   | false |
        | 199 | 299 | false   | false | false   | false |
        | 200 | 300 | false   | true  | false   | true  |
        | 500 | 900 | false   | true  | false   | true  |
        | -5  | -5  | true    | false | true    | false |

    Scenario: Leaving and reaching an end is notified
      When the property "contentY" of "f" is set to 10
      And the property "contentY" of "f" is set to 300
      Then the property "log" of "f" is "begin:false end:true "

    Scenario: An assigned position is not brought inside the bounds
      When the property "contentY" of "f" is set to 2000
      Then the property "contentY" of "f" is 2000
      When the property "contentX" of "f" is set to -50
      Then the property "contentX" of "f" is -50

    Scenario: Content that is smaller than the Flickable is at both ends
      When the property "contentWidth" of "f" is set to 50
      Then the property "atXBeginning" of "f" is true
      And the property "atXEnd" of "f" is true
      When the property "contentX" of "f" is set to 30
      Then the property "atXBeginning" of "f" is false
      And the property "atXEnd" of "f" is true

    Scenario: A Flickable that shrinks keeps showing what it showed
      When the property "contentY" of "f" is set to 100
      And the property "height" of "f" is set to 50
      Then the property "contentY" of "f" is 100

    Scenario: A Flickable that grows past the end pulls the content along
      When the property "contentY" of "f" is set to 100
      And the property "height" of "f" is set to 350
      Then the property "contentY" of "f" is 50
      When the property "height" of "f" is set to 100
      Then the property "contentY" of "f" is 50

    Scenario: Content that shrinks under the view is followed
      When the property "contentY" of "f" is set to 300
      And the property "contentHeight" of "f" is set to 250
      Then the property "contentY" of "f" is 150

    Scenario: A position far outside comes inside when the content size changes
      When the property "contentY" of "f" is set to 2000
      And the property "contentHeight" of "f" is set to 500
      Then the property "contentY" of "f" is 400

    @api-QtQuick.Flickable.returnToBounds
    Scenario Outline: returnToBounds brings the content back inside
      When the property "contentY" of "f" is set to <before>
      And the method "returnToBounds" of "f" is called
      And 2 seconds pass
      Then the property "contentY" of "f" is <after>

      Examples:
        | before | after |
        | -500   | 0     |
        | 2000   | 300   |
        | 120    | 120   |

    @api-QtQuick.Flickable.resizeContent
    Scenario: resizeContent keeps a point of the content where it is
      When the property "contentY" of "f" is set to 150
      And the method "grow" of "f" is called
      Then the property "contentHeight" of "f" is 800
      And the property "contentWidth" of "f" is 100
      And the property "contentY" of "f" is 200
      And the property "contentX" of "f" is 0

    @api-QtQuick.Flickable.pixelAligned
    Scenario Outline: Content that is aligned to pixels rests on whole ones
      When the property "pixelAligned" of "f" is set to true
      And the property "contentY" of "f" is set to <given>
      Then the property "contentY" of "f" is <kept>
      And the property "contentItem.y" of "f" is <item y>

      Examples:
        | given | kept | item y |
        | 10.4  | 10   | -10    |
        | 10.5  | 11   | -11    |
        | 11.5  | 12   | -12    |
        | -0.5  | -1   | 1      |
        | 0.3   | 0    | 0      |

    Scenario: Content that is not aligned to pixels rests where it is put
      When the property "contentY" of "f" is set to 10.4
      And the property "contentX" of "f" is set to 20.5
      Then the property "contentY" of "f" is 10.4
      And the property "contentX" of "f" is 20.5

  Rule: Margins

    Background:
      Given the QML:
        """qml
        import QtQuick
        Flickable {
          objectName: "f"
          width: 100; height: 100
          contentWidth: 300; contentHeight: 400
          topMargin: 10; bottomMargin: 20
          leftMargin: 30; rightMargin: 40
        }
        """

    @api-QtQuick.Flickable.topMargin @api-QtQuick.Flickable.leftMargin
    Scenario: The content starts after the top and left margins
      Then the property "contentX" of "f" is -30
      And the property "contentY" of "f" is -10
      And the property "contentItem.x" of "f" is 30
      And the property "contentItem.y" of "f" is 10
      And the property "atXBeginning" of "f" is true
      And the property "atYBeginning" of "f" is true

    @api-QtQuick.Flickable.bottomMargin @api-QtQuick.Flickable.rightMargin
    Scenario Outline: The end is past the content by the bottom and right margins
      When the property "contentX" of "f" is set to <x>
      And the property "contentY" of "f" is set to <y>
      Then the property "atXEnd" of "f" is <x end>
      And the property "atYEnd" of "f" is <y end>

      Examples:
        | x   | y   | x end | y end |
        | 239 | 319 | false | false |
        | 240 | 320 | true  | true  |

    Scenario: A margin that changes later does not move the content
      When the property "topMargin" of "f" is set to 50
      Then the property "contentY" of "f" is -10

    Scenario: returnToBounds knows the margins
      When the property "contentY" of "f" is set to -500
      And the method "returnToBounds" of "f" is called
      And 2 seconds pass
      Then the property "contentY" of "f" is -10

  Rule: The visible area

    Background:
      Given the QML:
        """qml
        import QtQuick
        Flickable {
          objectName: "f"
          width: 100; height: 100
          contentWidth: 400; contentHeight: 500
        }
        """

    @api-QtQuick.Flickable.visibleArea
    Scenario: The share of the content that is seen, and where it starts
      When the property "contentX" of "f" is set to 100
      And the property "contentY" of "f" is set to 50
      Then the property "visibleArea.widthRatio" of "f" is 0.25
      And the property "visibleArea.heightRatio" of "f" is 0.2
      And the property "visibleArea.xPosition" of "f" is 0.25
      And the property "visibleArea.yPosition" of "f" is 0.1

    Scenario: Content smaller than the Flickable is seen whole
      When the property "contentWidth" of "f" is set to 50
      Then the property "visibleArea.widthRatio" of "f" is 1
      When the property "contentX" of "f" is set to 30
      Then the property "visibleArea.xPosition" of "f" is 0.3

    Scenario: Without a content size everything is seen
      When the property "contentHeight" of "f" is set to -1
      Then the property "visibleArea.heightRatio" of "f" is 1
      And the property "visibleArea.yPosition" of "f" is 0

    Scenario: The margins count as content
      When the property "leftMargin" of "f" is set to 20
      And the property "rightMargin" of "f" is set to 30
      And the property "contentX" of "f" is set to 100
      Then the property "visibleArea.widthRatio" of "f" is within 0.0001 of 0.2222
      And the property "visibleArea.xPosition" of "f" is within 0.0001 of 0.2667

  Rule: A position that was declared

    Scenario: A declared position outside the content stays
      Given the QML:
        """qml
        import QtQuick
        Flickable {
          objectName: "f"
          width: 100; height: 100
          contentWidth: 300; contentHeight: 400
          contentY: 2000
        }
        """
      Then the property "contentY" of "f" is 2000
      And the property "atYEnd" of "f" is true

    Scenario: A declared position wins over the top margin
      Given the QML:
        """qml
        import QtQuick
        Flickable {
          objectName: "f"
          width: 100; height: 100
          contentWidth: 300; contentHeight: 400
          topMargin: 10
          contentY: 50
        }
        """
      Then the property "contentY" of "f" is 50
