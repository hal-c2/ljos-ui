# Sources:
#   docs/DIALECT.md §4.7 (input: one pointer stream, gestures as children), §4.14
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html
#   https://doc.qt.io/qt-6/qml-qtquick-hoverhandler.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# On Qt, a TapHandler whose gesturePolicy is ReleaseWithinBounds and whose longPressThreshold is 0
# counts the clicks of the first scenario as the MouseArea does; the `Tap` here is held to them.
# Neither backend shows a cursor headlessly and the Qt one cannot say which it asks for, so the
# cursor is pinned on Ljós's alone.

Feature: A MouseArea that fills its parent is gestures on it
  An area that fills its parent and takes clicks is a `Tap` on the
  parent that taps when the pointer is let go inside, however it moved
  and however long it was held, as a click is; the cursor it asks for is
  a `Hover`'s. Where a `Tap` would not act as the area does, it stays a
  `PointerArea`: something declared before it may take the pointer, it
  is stacked by `z`, or it does more than click.

  Rule: An area that fills its parent and takes clicks is a Tap on it

    @qt-parity
    Scenario: A MouseArea that fills its parent clicks wherever the pointer went in between
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 300; height: 200
          property int below: 0
          property int clicks: 0
          MouseArea { anchors.fill: parent; onClicked: root.below += 1 }
          Rectangle {
            width: 100; height: 60
            MouseArea { anchors.fill: parent; onClicked: root.clicks += 1 }
          }
        }
        """
      When the user presses the pointer at 10, 10
      And the user releases the pointer at 90, 50
      And the user presses the pointer at 50, 30
      And the user releases the pointer at 150, 30
      Then the property "clicks" of "root" is 1
      When the user presses the pointer at 50, 30
      And 1200 milliseconds pass
      And the user releases the pointer at 50, 30
      And the user presses the pointer at 50, 30
      And the user moves the pointer to 150, 30
      And the user moves the pointer to 50, 40
      And the user releases the pointer at 50, 40
      Then the property "clicks" of "root" is 3
      When the user double-clicks at 50, 30
      And the user clicks the right button at 50, 30
      Then the property "clicks" of "root" is 5
      And the property "below" of "root" is 0

    @ljos-only
    Scenario: A Tap let go inside taps wherever the pointer went in between
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          width: 300; height: 200
          property int below: 0
          property int clicks: 0
          Tap { gesturePolicy: Tap.ReleaseWithinBounds; longPressThreshold: 0; onTapped: root.below += 1 }
          Rectangle {
            width: 100; height: 60
            Tap { gesturePolicy: Tap.ReleaseWithinBounds; longPressThreshold: 0; onTapped: root.clicks += 1 }
          }
        }
        """
      When the user presses the pointer at 10, 10
      And the user releases the pointer at 90, 50
      And the user presses the pointer at 50, 30
      And the user releases the pointer at 150, 30
      Then the property "clicks" of "root" is 1
      When the user presses the pointer at 50, 30
      And 1200 milliseconds pass
      And the user releases the pointer at 50, 30
      And the user presses the pointer at 50, 30
      And the user moves the pointer to 150, 30
      And the user moves the pointer to 50, 40
      And the user releases the pointer at 50, 40
      Then the property "clicks" of "root" is 3
      When the user double-clicks at 50, 30
      And the user clicks the right button at 50, 30
      Then the property "clicks" of "root" is 5
      And the property "below" of "root" is 0

    @ljos-only
    Scenario: The area lowered clicks as it did
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 300; height: 200
          property int below: 0
          property int clicks: 0
          MouseArea { anchors.fill: parent; onClicked: root.below += 1 }
          Rectangle {
            width: 100; height: 60
            MouseArea { objectName: "area"; anchors.fill: parent; onClicked: root.clicks += 1 }
          }
        }
        """
      Then the property "longPressThreshold" of "area" is 0
      When the user presses the pointer at 10, 10
      And the user releases the pointer at 90, 50
      And the user presses the pointer at 50, 30
      And the user releases the pointer at 150, 30
      Then the property "clicks" of "root" is 1
      When the user presses the pointer at 50, 30
      And 1200 milliseconds pass
      And the user releases the pointer at 50, 30
      And the user presses the pointer at 50, 30
      And the user moves the pointer to 150, 30
      And the user moves the pointer to 50, 40
      And the user releases the pointer at 50, 40
      Then the property "clicks" of "root" is 3
      When the user double-clicks at 50, 30
      And the user clicks the right button at 50, 30
      Then the property "clicks" of "root" is 5
      And the property "below" of "root" is 0

  Rule: The cursor an area asks for is a Hover's, disabled as the area is

    @ljos-only
    Scenario: The area lowered asks for its cursor while it is enabled
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Rectangle {
          id: card; objectName: "card"
          width: 100; height: 60
          property bool on: true
          MouseArea {
            anchors.fill: parent
            enabled: card.on
            cursorShape: Qt.PointingHandCursor
          }
        }
        """
      When the user moves the pointer to 50, 30
      Then the pointer shows the "PointingHandCursor" cursor
      When the property "on" of "card" is set to false
      And the user moves the pointer to 60, 30
      Then the pointer shows the "ArrowCursor" cursor

  Rule: An area a Tap would not act as stays a PointerArea

    # The area is over what is declared before it in its parent; a Tap on the parent would be under it.
    @qt-parity
    Scenario: An area over a sibling that takes the pointer takes its clicks
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          id: card; objectName: "card"
          width: 100; height: 60
          property string log: ""
          Rectangle {
            width: 40; height: 40
            MouseArea { anchors.fill: parent; onClicked: card.log += "inner " }
          }
          MouseArea { anchors.fill: parent; onClicked: card.log += "card " }
        }
        """
      When the user presses the pointer at 20, 20
      And the user releases the pointer at 20, 20
      Then the property "log" of "card" is "card "

    @ljos-only
    Scenario: The area over a sibling that takes the pointer, lowered, takes its clicks
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Rectangle {
          id: card; objectName: "card"
          width: 100; height: 60
          property string log: ""
          Rectangle {
            width: 40; height: 40
            MouseArea { anchors.fill: parent; onClicked: card.log += "inner " }
          }
          MouseArea { anchors.fill: parent; onClicked: card.log += "card " }
        }
        """
      When the user presses the pointer at 20, 20
      And the user releases the pointer at 20, 20
      Then the property "log" of "card" is "card "

    @ljos-only @headless
    Scenario: ljos migrate writes an area that only clicks as gestures and keeps one that does more
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 60
          MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: console.log("open")
          }
          Rectangle {
            width: 20; height: 20
            MouseArea { anchors.fill: parent; onPressed: console.log("down") }
          }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (7 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        Rectangle {
          width: 100; height: 60
          Tap {
            gesturePolicy: Tap.ReleaseWithinBounds; longPressThreshold: 0

            onTapped: console.log("open")
          } Hover { cursorShape: Qt.PointingHandCursor }
          Rectangle {
            width: 20; height: 20
            PointerArea { anchors.fill: parent; onPressed: console.log("down") }
          }
        }
        """
