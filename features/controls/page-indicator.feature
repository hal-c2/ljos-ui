# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-pageindicator.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: PageIndicator
  A page indicator is a row of `count` dots, one per page, with the one at
  `currentIndex` stronger than the others. An `interactive` one takes a
  new current page from the dot nearest a click. Each dot is made from
  `delegate`, which knows its `index` and whether it is `pressed`.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        PageIndicator { id: p; objectName: "p"; count: 5; interactive: true }
        PageIndicator { objectName: "q"; y: 100; count: 3 }
        PageIndicator {
          objectName: "c"; y: 200; count: 3; interactive: true
          delegate: Rectangle {
            required property int index
            objectName: "dot" + index
            implicitWidth: 10; implicitHeight: 10
            color: pressed ? "red" : "blue"
          }
        }
        property real first: p.contentItem.children[0].opacity
        property real second: p.contentItem.children[1].opacity
        property real secondWidth: p.contentItem.children[1].width
        property real secondX: p.contentItem.children[1].x
        property real secondRadius: p.contentItem.children[1].radius
        objectName: "root"
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: Three dots of 8 pixels with 6 between them
      Then the property "<property>" of "q" is <value>

      Examples:
        | property       | value |
        | count          | 3     |
        | currentIndex   | 0     |
        | interactive    | false |
        | focusPolicy    | 0     |
        | implicitWidth  | 48    |
        | implicitHeight | 20    |
        | padding        | 6     |
        | spacing        | 6     |

    Scenario: Each dot is a circle, and the current one is the strongest
      Then the property "implicitWidth" of "p" is 76
      And the property "secondWidth" of "root" is 8
      And the property "secondX" of "root" is 14
      And the property "secondRadius" of "root" is 4
      And the property "first" of "root" is 0.95
      And the property "second" of "root" is 0.45

    Scenario: The current dot moves with the current page
      When the property "currentIndex" of "p" is set to 1
      And 300 milliseconds pass
      Then the property "first" of "root" is 0.45
      And the property "second" of "root" is 0.95

    Scenario: The current index is not held to the count
      When the property "currentIndex" of "p" is set to 7
      Then the property "currentIndex" of "p" is 7

  @qt-parity
  @api-QtQuick.Controls.PageIndicator.interactive
  Rule: Clicking a dot

    Scenario: A click makes the nearest dot current, when it is let go
      When the user presses the pointer at 31, 10
      Then the property "currentIndex" of "p" is 0
      When the user releases the pointer at 31, 10
      Then the property "currentIndex" of "p" is 1

    Scenario: A click on the edge takes the dot beside it
      When the property "currentIndex" of "p" is set to 3
      And the user presses the pointer at 2, 2
      And the user releases the pointer at 2, 2
      Then the property "currentIndex" of "p" is 0

    Scenario: The dot is the one under the pointer when it lets go
      When the user presses the pointer at 60, 10
      And the user moves the pointer to 24, 10
      And the user releases the pointer at 24, 10
      Then the property "currentIndex" of "p" is 1

    Scenario: A page indicator that is not interactive takes no click
      When the user presses the pointer at 30, 110
      And the user releases the pointer at 30, 110
      Then the property "currentIndex" of "q" is 0

  @qt-parity
  @api-QtQuick.Controls.PageIndicator.delegate
  Rule: The delegate

    Scenario: A delegate of another size makes the row another size
      Then the property "implicitWidth" of "c" is 54
      And the property "implicitHeight" of "c" is 22
      And the property "color" of "dot0" is "#0000ff"

    Scenario: A delegate knows whether it is the one pressed
      When the user presses the pointer at 25, 210
      Then the property "color" of "dot1" is "#ff0000"
      And the property "color" of "dot0" is "#0000ff"
      When the user releases the pointer at 25, 210
      Then the property "color" of "dot1" is "#0000ff"
      And the property "currentIndex" of "c" is 1
