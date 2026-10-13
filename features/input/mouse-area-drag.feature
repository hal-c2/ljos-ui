# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#drag-prop
#   https://doc.qt.io/qt-6/qml-qtquick-drag.html
#   https://doc.qt.io/qt-6/qtquick-draganddrop-example.html
# Verified on Qt 6.11.2 (qml6, qmltestrunner and vnc).
# The drag starts from Qt.styleHints.startDragDistance, 8 pixels
# (drag-threshold.feature): the first move further than that from the
# press marks where the drag starts, and the move after it starts it.
# `drag.active` is heard through a property bound to it: the handler of a
# grouped property's signal (`drag.onActiveChanged`) is not wired yet.

@qt-parity
Feature: Dragging an item with a MouseArea
  A MouseArea with a `drag.target` moves that item as the pointer drags
  it, along the axes it allows and inside the limits it sets. A press that
  became a drag is no click. With `drag.filterChildren` it drags from the
  areas inside it too, and `drag.active` can drive an item's `Drag`.

  Rule: The target follows the pointer once the drag starts

    Background:
      Given the file "Card.qml":
        """qml
        import QtQuick
        Rectangle {
          id: card
          width: 100; height: 100
          property alias smoothed: area.drag.smoothed
          signal said(string s)
          MouseArea {
            id: area
            anchors.fill: parent
            drag.target: card
            onReleased: (mouse) => card.said("released(" + mouse.isClick + ") ")
            onClicked: card.said("clicked ")
            onPositionChanged: (mouse) => card.said("pos(" + mouse.x + "," + mouse.y + ") ")
            property bool dragging: drag.active
            onDraggingChanged: card.said("active=" + dragging + "@" + card.x + "," + card.y + " ")
          }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          Card { objectName: "card"; x: 50; y: 50; onSaid: (s) => root.log += s }
          Card { objectName: "raw"; x: 250; y: 250; smoothed: false }
        }
        """

    @api-QtQuick.MouseArea.drag.active
    Scenario: A move inside the threshold is still a click
      When the user presses the pointer at 100, 100
      And the user moves the pointer to 100, 108
      And the user releases the pointer at 100, 108
      Then the property "log" of "root" is "pos(50,58) released(true) clicked "
      And the property "y" of "card" is 50

    @api-QtQuick.MouseArea.drag.target
    Scenario: The first move past the threshold moves nothing
      When the user presses the pointer at 100, 100
      And the user moves the pointer to 100, 109
      Then the property "log" of "root" is "pos(50,59) "
      And the property "y" of "card" is 50

    @api-QtQuick.MouseArea.drag.active @api-QtQuick.MouseArea.drag.smoothed
    Scenario: The next move starts the drag from where the first one was
      When the user presses the pointer at 100, 100
      And the user moves the pointer to 100, 109
      And the user moves the pointer to 100, 120
      Then the property "log" of "root" is "pos(50,59) active=true@50,50 pos(50,59) "
      And the property "y" of "card" is 61

    @api-QtQuick.MouseArea.drag.active
    Scenario: The release ends the drag and is no click
      When the user presses the pointer at 100, 100
      And the user moves the pointer to 100, 109
      And the user moves the pointer to 100, 120
      And the property "log" of "root" is set to ""
      And the user releases the pointer at 100, 120
      Then the property "log" of "root" is "released(false) active=false@50,61 "

    @api-QtQuick.MouseArea.drag.smoothed
    Scenario: Without smoothing the target jumps to follow the press
      When the user presses the pointer at 300, 300
      And the user moves the pointer to 300, 310
      And the user moves the pointer to 300, 320
      Then the property "y" of "raw" is 270

  Rule: The drag keeps to its axes and limits

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 400; height: 400
          Rectangle {
            id: rail
            objectName: "rail"
            x: 250; y: 50; width: 100; height: 100
            MouseArea { anchors.fill: parent; drag.target: rail; drag.axis: Drag.XAxis }
          }
          Rectangle {
            id: pen
            objectName: "pen"
            x: 50; y: 250; width: 100; height: 100
            MouseArea {
              anchors.fill: parent; drag.target: pen
              drag.minimumX: 30; drag.maximumX: 70
              drag.minimumY: 240; drag.maximumY: 280
            }
          }
        }
        """

    @api-QtQuick.MouseArea.drag.axis
    Scenario: A move across the axis starts nothing
      When the user presses the pointer at 300, 100
      And the user moves the pointer to 300, 120
      And the user moves the pointer to 300, 140
      Then the property "x" of "rail" is 250
      And the property "y" of "rail" is 50

    @api-QtQuick.MouseArea.drag.axis
    Scenario: Only the move along the axis counts, and only it is followed
      When the user presses the pointer at 300, 100
      And the user moves the pointer to 305, 120
      And the user moves the pointer to 320, 140
      And the user moves the pointer to 340, 145
      Then the property "x" of "rail" is 270
      And the property "y" of "rail" is 50

    @api-QtQuick.MouseArea.drag.minimumX @api-QtQuick.MouseArea.drag.maximumX
    @api-QtQuick.MouseArea.drag.minimumY @api-QtQuick.MouseArea.drag.maximumY
    Scenario: The target stays inside the limits
      When the user presses the pointer at 100, 300
      And the user moves the pointer to 110, 310
      And the user moves the pointer to 120, 320
      Then the property "x" of "pen" is 60
      And the property "y" of "pen" is 260
      When the user moves the pointer to 200, 330
      Then the property "x" of "pen" is 70
      When the user moves the pointer to 0, 200
      Then the property "x" of "pen" is 30
      And the property "y" of "pen" is 240

  Rule: An area that filters its children drags from them too

    Background:
      Given the file "Inner.qml":
        """qml
        import QtQuick
        MouseArea {
          signal said(string s)
          onPressed: said("ipressed ")
          onCanceled: said("icanceled ")
          onReleased: said("ireleased ")
          onClicked: said("iclicked ")
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          property bool filtering: true
          Rectangle {
            id: outer
            objectName: "outer"
            width: 200; height: 200
            MouseArea {
              anchors.fill: parent
              drag.target: outer; drag.filterChildren: root.filtering
              onPressed: root.log += "opressed "
              onReleased: (mouse) => root.log += "oreleased(" + mouse.isClick + ") "
              onClicked: root.log += "oclicked "
              property bool dragging: drag.active
              onDraggingChanged: root.log += "oactive=" + dragging + " "
              Inner { x: 50; y: 50; width: 100; height: 100; onSaid: (s) => root.log += s }
            }
          }
        }
        """

    @api-QtQuick.MouseArea.drag.filterChildren
    Scenario: Without filtering the inner area keeps the drag
      When the property "filtering" of "root" is set to false
      And the user presses the pointer at 100, 100
      And the user moves the pointer to 100, 110
      And the user moves the pointer to 100, 120
      And the user releases the pointer at 100, 120
      Then the property "log" of "root" is "ipressed ireleased iclicked "
      And the property "y" of "outer" is 0

    @api-QtQuick.MouseArea.drag.filterChildren
    Scenario: A filtering area hears the click on the inner one first
      When the user presses the pointer at 100, 100
      And the user moves the pointer to 100, 105
      And the user releases the pointer at 100, 105
      Then the property "log" of "root" is "opressed ipressed oreleased(true) oclicked ireleased iclicked "

    @api-QtQuick.MouseArea.drag.filterChildren
    Scenario: A filtering area takes the drag from the inner one
      When the user presses the pointer at 100, 100
      And the user moves the pointer to 100, 110
      And the user moves the pointer to 100, 120
      And the user moves the pointer to 100, 140
      And the user releases the pointer at 100, 140
      Then the property "log" of "root" is "opressed ipressed oactive=true icanceled oreleased(false) oactive=false "
      And the property "y" of "outer" is 30

  Rule: The drag of an area can be the drag of an item

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          Rectangle {
            id: card
            objectName: "card"
            x: 50; y: 50; width: 50; height: 50
            Drag.active: area.drag.active
            Drag.onActiveChanged: root.log += "Drag.active=" + Drag.active + " "
            MouseArea {
              id: area
              anchors.fill: parent; drag.target: card
              onReleased: { root.log += "released "; card.Drag.drop() }
            }
          }
          DropArea {
            x: 250; y: 50; width: 100; height: 100
            onEntered: root.log += "entered "
            onDropped: root.log += "dropped "
          }
        }
        """

    @api-QtQuick.MouseArea.drag.active @api-QtQuick.Drag.active
    Scenario: An item dragged by its area is dropped where the area lets go
      When the user presses the pointer at 60, 60
      And the user moves the pointer to 60, 70
      And the user moves the pointer to 60, 80
      And the user moves the pointer to 200, 80
      And the user moves the pointer to 270, 80
      And the user releases the pointer at 270, 80
      Then the property "log" of "root" is "Drag.active=true entered released dropped Drag.active=false "
      And the property "x" of "card" is 260
      And the property "y" of "card" is 60

  Rule: Inside a Flickable the drag keeps the press it started

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          property int axis: Drag.XAndYAxis
          Flickable {
            objectName: "f"
            width: 200; height: 200
            contentWidth: 200; contentHeight: 2000
            Rectangle {
              id: card
              objectName: "card"
              x: 50; y: 50; width: 50; height: 50
              MouseArea {
                anchors.fill: parent; drag.target: card; drag.axis: root.axis
                onCanceled: root.log += "canceled "
              }
            }
          }
        }
        """

    @api-QtQuick.MouseArea.drag.target
    Scenario: A drag along the Flickable is the area's
      When the user presses the pointer at 75, 75
      And the user moves the pointer to 75, 65
      And the user moves the pointer to 75, 55
      And the user moves the pointer to 75, 30
      Then the property "y" of "card" is 15
      And the property "contentY" of "f" is 0
      And the property "dragging" of "f" is false

    @api-QtQuick.MouseArea.drag.axis
    Scenario: A drag the area does not allow is the Flickable's
      When the property "axis" of "root" is set to 1
      And the user presses the pointer at 75, 75
      And the user moves the pointer to 75, 65
      And the user moves the pointer to 75, 55
      And the user moves the pointer to 75, 30
      Then the property "log" of "root" is "canceled "
      And the property "dragging" of "f" is true
      And the property "contentY" of "f" is 25
      And the property "y" of "card" is 50

    @api-QtQuick.MouseArea.drag.axis
    Scenario: A drag along the area's own axis is the area's
      When the property "axis" of "root" is set to 1
      And the user presses the pointer at 75, 75
      And the user moves the pointer to 85, 75
      And the user moves the pointer to 95, 75
      And the user moves the pointer to 120, 65
      Then the property "x" of "card" is 85
      And the property "y" of "card" is 50
      And the property "contentY" of "f" is 0
