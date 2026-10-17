# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-swipedelegate.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-swipedelegate.html#swipe-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: SwipeDelegate
  A row of a list that can be swiped aside to show what is under it: an
  item at its left, one at its right, or one behind it. The swipe is
  `swipe.position`, from -1 (swiped all the way left, showing the right
  item) to 1 (all the way right, showing the left item). The content and
  the background move by the position times the width of the item shown.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300; property string log: ""
        SwipeDelegate {
          id: d; objectName: "d"; width: 300; height: 50; text: "row"
          swipe.left: Rectangle { objectName: "l"; width: 100; height: 50; color: "green" }
          swipe.right: Rectangle { objectName: "r"; width: 80; height: 50; color: "red"; anchors.right: parent.right }
          onClicked: root.log += "clicked "; onCanceled: root.log += "canceled "
          onPressedChanged: root.log += "pressed" + pressed + " "; swipe.onCompleted: root.log += "completed "
          swipe.onOpened: root.log += "opened "; swipe.onClosed: root.log += "closed "
        }
        SwipeDelegate {
          id: b; objectName: "b"; y: 60; width: 300; height: 50; text: "behind"
          swipe.behind: Rectangle { width: 40; height: 50 }
        }
        SwipeDelegate {
          id: n; objectName: "n"; y: 120; width: 300; height: 50; text: "none"
          onClicked: root.log += "clicked "
        }
        function openLeft() { d.swipe.open(SwipeDelegate.Left) } function close() { d.swipe.close() }
        function openRight() { d.swipe.open(SwipeDelegate.Right) } function openBehind() { b.swipe.open(SwipeDelegate.Left) }
        function enums() { log = SwipeDelegate.Left + " " + SwipeDelegate.Right }
        function made() { log = (d.swipe.leftItem === null) + " " + (d.swipe.rightItem === null) + " " + (d.swipe.transition !== null) }
      }
      """

  Rule: The Basic style

    Scenario Outline: A swipe delegate at rest
      Then the property "<property>" of "<row>" is <value>

      Examples:
        | row | property           | value |
        | n   | implicitWidth      | 100   |
        | n   | implicitHeight     | 40    |
        | n   | padding            | 12    |
        | n   | spacing            | 12    |
        | n   | focusPolicy        | 0     |
        | n   | background.visible | true  |
        | n   | background.x       | 0     |
        | n   | contentItem.x      | 12    |
        | n   | swipe.position     | 0     |
        | n   | swipe.complete     | false |
        | n   | swipe.enabled      | true  |
        | n   | swipe.leftItem     | null  |
        | n   | swipe.rightItem    | null  |
        | n   | swipe.behindItem   | null  |

    @api-QtQuick.Controls.SwipeDelegate.swipe
    Scenario: The sides are numbered, the items are made when they are first shown, and there is a transition
      When the method "enums" of "root" is called
      Then the property "log" of "root" is "1 -1"
      When the method "made" of "root" is called
      Then the property "log" of "root" is "true true true"

  Rule: Opening and closing

    @api-QtQuick.Controls.SwipeDelegate.swipe
    Scenario: Opened to the left, the left item shows and the row moves its width
      When the method "openLeft" of "root" is called
      And 1 second passes
      Then the property "swipe.position" of "d" is 1
      And the property "swipe.complete" of "d" is true
      And the property "swipe.leftItem.visible" of "d" is true
      And the property "swipe.leftItem.x" of "d" is 0
      And the property "swipe.leftItem.z" of "d" is -2
      And the property "contentItem.x" of "d" is 112
      And the property "background.x" of "d" is 100
      And the property "log" of "root" is "completed opened "

    Scenario: Closed, it comes back
      When the method "openLeft" of "root" is called
      And 1 second passes
      And the method "close" of "root" is called
      And 1 second passes
      Then the property "swipe.position" of "d" is 0
      And the property "swipe.complete" of "d" is false
      And the property "contentItem.x" of "d" is 12
      And the property "background.x" of "d" is 0
      And the property "log" of "root" is "completed opened closed "

    Scenario: Opened to the right, the right item shows and the row moves the other way
      When the method "openRight" of "root" is called
      And 1 second passes
      Then the property "swipe.position" of "d" is -1
      And the property "swipe.rightItem.visible" of "d" is true
      And the property "swipe.rightItem.x" of "d" is 220
      And the property "contentItem.x" of "d" is -68
      And the property "background.x" of "d" is -80
      And the property "log" of "root" is "completed opened "

    Scenario: The row moves by the width of the item behind it
      When the method "openBehind" of "root" is called
      And 1 second passes
      Then the property "swipe.behindItem.visible" of "b" is true
      And the property "contentItem.x" of "b" is 52
      And the property "background.x" of "b" is 40

    Scenario: A position set says nothing of being opened
      When the property "swipe.position" of "d" is set to 0.3
      Then the property "contentItem.x" of "d" is 42
      And the property "background.x" of "d" is 30
      And the property "swipe.complete" of "d" is false
      And the property "log" of "root" is ""

  Rule: Swiping with the pointer

    @api-QtQuick.Controls.SwipeDelegate.swipe
    Scenario: Swiped all the way, the row opens, and the press is canceled rather than clicked
      When the user drags from 20, 25 to 160, 25
      And 1 second passes
      Then the property "swipe.position" of "d" is 1
      And the property "swipe.complete" of "d" is true
      And the property "log" of "root" is "pressedtrue pressedfalse canceled completed opened "

    Scenario: Swiped the other way, it opens to the right
      When the user drags from 250, 25 to 120, 25
      And 1 second passes
      Then the property "swipe.position" of "d" is -1
      And the property "log" of "root" is "pressedtrue pressedfalse canceled completed opened "

    Scenario: A click on a closed row is a click
      When the user presses the pointer at 150, 25
      And the user releases the pointer at 150, 25
      Then the property "log" of "root" is "pressedtrue pressedfalse clicked "

    Scenario: A click on an open row says it is open again, and is not a click
      When the method "openLeft" of "root" is called
      And 1 second passes
      And the user presses the pointer at 150, 25
      And the user releases the pointer at 150, 25
      Then the property "swipe.position" of "d" is 1
      And the property "log" of "root" is "completed opened pressedtrue pressedfalse opened canceled "

    @api-QtQuick.Controls.SwipeDelegate.swipe
    Scenario: A row that cannot be swiped takes a drag as a click
      When the property "swipe.enabled" of "d" is set to false
      And the user drags from 20, 25 to 160, 25
      Then the property "swipe.position" of "d" is 0
      And the property "log" of "root" is "pressedtrue pressedfalse clicked "

    Scenario: A row with nothing to show takes a drag as a click
      When the user drags from 20, 145 to 160, 145
      Then the property "swipe.position" of "n" is 0
      And the property "log" of "root" is "clicked "
