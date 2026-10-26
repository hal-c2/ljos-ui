# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#cursorShape-prop
#   https://doc.qt.io/qt-6/qml-qtquick-pointerhandler.html#cursorShape-prop
#   https://doc.qt.io/qt-6/qml-qtquick-hoverhandler.html
# Verified on Qt 6.11.2 (qml6 under X11, the cursor's picture read with
# XFixes against areas that ask for each shape). The offscreen oracle has no
# pointer to show a cursor.

@ljos-only
Feature: The pointer takes the shape of what it is over
  The pointer shows the shape of the topmost thing under it that has one:
  a MouseArea, which has the arrow until it is given another, or a hovered
  HoverHandler, or an active DragHandler, given a shape. A MouseArea counts
  when its own `enabled` is false, not when an item around it is disabled;
  a hidden one, a disabled HoverHandler and one given no shape do not count.
  While a MouseArea holds the press the pointer still shows what it is over,
  where only the handlers hovered at the press are hovered.

  Scenario Outline: At <x>, <y> the pointer shows the <cursor>
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 400; height: 150
        MouseArea { width: 50; height: 50; cursorShape: Qt.SplitHCursor }
        Rectangle { x: 50; width: 50; height: 50; HoverHandler { cursorShape: Qt.PointingHandCursor } }
        Rectangle { x: 100; width: 100; height: 50; HoverHandler { cursorShape: Qt.CrossCursor }
          MouseArea { width: 25; height: 50 }
          Rectangle { x: 25; width: 25; height: 50 }
          Item { x: 50; width: 25; height: 50; HoverHandler { cursorShape: Qt.IBeamCursor } }
          Item { x: 75; width: 25; height: 50; HoverHandler { } } }
        MouseArea { x: 200; width: 50; height: 50; enabled: false; cursorShape: Qt.SplitVCursor }
        Rectangle { x: 250; width: 50; height: 50; HoverHandler { cursorShape: Qt.CrossCursor }
          Item { anchors.fill: parent; enabled: false
            MouseArea { anchors.fill: parent; cursorShape: Qt.IBeamCursor } } }
        Rectangle { x: 300; width: 50; height: 50; HoverHandler { cursorShape: Qt.CrossCursor }
          MouseArea { anchors.fill: parent; visible: false; cursorShape: Qt.IBeamCursor } }
        Rectangle { x: 350; width: 50; height: 50; HoverHandler { enabled: false; cursorShape: Qt.WaitCursor } }
        MouseArea { y: 50; width: 50; height: 50; acceptedButtons: Qt.NoButton; cursorShape: Qt.PointingHandCursor }
        MouseArea { x: 50; y: 50; width: 50; height: 50; cursorShape: Qt.IBeamCursor }
        Item { x: 50; y: 50; width: 25; height: 50; HoverHandler { cursorShape: Qt.CrossCursor } }
      }
      """
    When the user moves the pointer to <x>, <y>
    Then the pointer shows the "<cursor>" cursor

    Examples:
      | x   | y   | cursor             |
      | 25  | 25  | SplitHCursor       |
      | 75  | 25  | PointingHandCursor |
      | 110 | 25  | ArrowCursor        |
      | 135 | 25  | CrossCursor        |
      | 160 | 25  | IBeamCursor        |
      | 185 | 25  | CrossCursor        |
      | 225 | 25  | SplitVCursor       |
      | 275 | 25  | CrossCursor        |
      | 325 | 25  | CrossCursor        |
      | 375 | 25  | ArrowCursor        |
      | 25  | 75  | PointingHandCursor |
      | 60  | 75  | CrossCursor        |
      | 90  | 75  | IBeamCursor        |
      | 25  | 125 | ArrowCursor        |

  Rule: While a MouseArea holds the press

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 100
          MouseArea { width: 100; height: 100; cursorShape: Qt.SplitHCursor }
          Rectangle { x: 100; width: 100; height: 100; HoverHandler { cursorShape: Qt.CrossCursor }
            MouseArea { y: 50; width: 100; height: 50; cursorShape: Qt.IBeamCursor } }
          MouseArea { x: 200; width: 100; height: 50; cursorShape: Qt.ForbiddenCursor }
        }
        """

    Scenario: The pointer shows the areas it is dragged over, and the arrow elsewhere
      When the user moves the pointer to 50, 50
      And the user presses the pointer at 50, 50
      And the user moves the pointer to 250, 25
      Then the pointer shows the "ForbiddenCursor" cursor
      When the user moves the pointer to 150, 25
      Then the pointer shows the "ArrowCursor" cursor
      When the user moves the pointer to 250, 80
      And the user releases the pointer at 250, 80
      Then the pointer shows the "ArrowCursor" cursor

    Scenario: A handler hovered at the press keeps its shape over its item
      When the user moves the pointer to 150, 75
      And the user presses the pointer at 150, 75
      And the user moves the pointer to 150, 25
      Then the pointer shows the "CrossCursor" cursor
      When the user moves the pointer to 250, 80
      Then the pointer shows the "ArrowCursor" cursor
      When the user releases the pointer at 250, 80
      And the user moves the pointer to 150, 25
      Then the pointer shows the "CrossCursor" cursor

  Rule: A shape that changes shows at once

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 300; height: 100
          property bool other
          function change() { other = !other }
          MouseArea { width: 100; height: 100
            cursorShape: pressed ? Qt.ClosedHandCursor : root.other ? Qt.IBeamCursor : Qt.OpenHandCursor }
          Rectangle { x: 200; width: 100; height: 100
            DragHandler { target: null; cursorShape: Qt.ClosedHandCursor } }
        }
        """

    Scenario: An area's shape follows its binding while the pointer stays
      When the user moves the pointer to 50, 50
      Then the pointer shows the "OpenHandCursor" cursor
      When the user presses the pointer at 50, 50
      Then the pointer shows the "ClosedHandCursor" cursor
      When the user releases the pointer at 50, 50
      And the method "change" of "root" is called
      Then the pointer shows the "IBeamCursor" cursor

    Scenario: A DragHandler shows its shape while it drags, over its item
      When the user moves the pointer to 250, 50
      And the user presses the pointer at 250, 50
      Then the pointer shows the "ArrowCursor" cursor
      When the user moves the pointer to 270, 50
      And the user moves the pointer to 290, 50
      Then the pointer shows the "ClosedHandCursor" cursor
      When the user moves the pointer to 150, 50
      Then the pointer shows the "ArrowCursor" cursor
      When the user releases the pointer at 150, 50
      Then the pointer shows the "ArrowCursor" cursor
