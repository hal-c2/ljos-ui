# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tumbler.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tumbler-members.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Tumbler
  A tumbler shows the rows of a model as a column that turns: the current
  row is in the middle and the ones before and after it above and below.
  A tumbler with at least as many rows as it shows wraps round; one with
  fewer stops at its ends. Each delegate knows which tumbler it is in and
  how far from the middle it is.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300; property string seen: ""
        ListModel { id: names; ListElement { name: "a" } ListElement { name: "b" } ListElement { name: "c" } }
        Tumbler {
          id: t; objectName: "t"; model: 10; focus: true
          delegate: Text { required property int index; required property var modelData; objectName: "d" + index; text: modelData; property real disp: Tumbler.displacement; property bool mine: Tumbler.tumbler === t }
        }
        Tumbler {
          id: s; objectName: "s"; x: 100; model: 6; wrap: false; visibleItemCount: 3
          delegate: Text { required property int index; required property var modelData; objectName: "s" + index; text: modelData; property real disp: Tumbler.displacement }
        }
        Tumbler { id: l; objectName: "l"; x: 200; model: names }
        Tumbler { id: a; objectName: "a"; x: 300; model: ["x", "y"] }
        Tumbler { id: e; objectName: "e"; y: 220 }
        Item { id: plain; objectName: "plain" }
        function outside() { seen = [plain.Tumbler.tumbler, plain.Tumbler.displacement].join("|") }
        function modes() { seen = [Tumbler.Beginning, Tumbler.Center, Tumbler.End, Tumbler.Visible, Tumbler.Contain, Tumbler.SnapPosition].join(" ") }
      }
      """

  @qt-parity
  @api-QtQuick.Controls.Tumbler.count @api-QtQuick.Controls.Tumbler.currentIndex
  @api-QtQuick.Controls.Tumbler.visibleItemCount @api-QtQuick.Controls.Tumbler.wrap
  @api-QtQuick.Controls.Tumbler.moving
  Rule: What a tumbler is at the start

    Scenario Outline: A tumbler of ten rows wraps and shows five
      Then the property "<property>" of "t" is <value>

      Examples:
        | property         | value |
        | count            | 10    |
        | currentIndex     | 0     |
        | visibleItemCount | 5     |
        | wrap             | true  |
        | moving           | false |

    @api-QtQuick.Controls.Tumbler.flickDeceleration
    Scenario: A wrapping tumbler flicks as a path view does
      Then the property "flickDeceleration" of "t" is 100

    Scenario: A tumbler that stops at its ends flicks as a list does
      Then the property "flickDeceleration" of "s" is 1500

    Scenario Outline: A tumbler is as large as five rows of a narrow column
      Then the property "<property>" of "<name>" is <value>

      Examples:
        | name | property       | value |
        | t    | implicitWidth  | 60    |
        | t    | implicitHeight | 200   |
        | s    | implicitWidth  | 60    |
        | s    | implicitHeight | 200   |
        | t    | width          | 60    |
        | t    | height         | 200   |

    Scenario: A tumbler without rows has none and no current one
      Then the property "count" of "e" is 0
      And the property "currentIndex" of "e" is -1
      And the property "currentItem" of "e" is null

    Scenario: A tumbler of a number of rows has the first one current
      When 1 second passes
      Then the property "currentItem.objectName" of "t" is "d0"

    Scenario: The rows of a list model and of a list are counted
      Then the property "count" of "l" is 3
      And the property "count" of "a" is 2

  @qt-parity
  Rule: Whether it wraps

    Scenario Outline: A tumbler wraps when it has at least as many rows as it shows
      When the property "model" of "t" is set to <rows>
      Then the property "wrap" of "t" is <wraps>

      Examples:
        | rows | wraps |
        | 3    | false |
        | 4    | false |
        | 5    | true  |
        | 12   | true  |

    Scenario: A tumbler with few rows does not wrap
      Then the property "wrap" of "a" is false
      And the property "wrap" of "s" is false

    Scenario: Asking a tumbler to wrap or not is kept when the rows change
      When the property "wrap" of "t" is set to false
      And the property "model" of "t" is set to 20
      Then the property "wrap" of "t" is false

  @qt-parity
  Rule: The current index

    @api-QtQuick.Controls.Tumbler.currentItem
    Scenario: Setting the current index makes that row the current one
      When the property "currentIndex" of "t" is set to 3
      And 1 second passes
      Then the property "currentIndex" of "t" is 3
      And the property "currentItem.objectName" of "t" is "d3"

    Scenario: A row that is the current one is in the middle
      When the property "currentIndex" of "t" is set to 3
      And 1 second passes
      Then the property "y" of "d3" is within 0.5 of 80

    Scenario Outline: An index that is not a row is not taken
      When the property "currentIndex" of "s" is set to 3
      And the property "currentIndex" of "s" is set to <given>
      And 1 second passes
      Then the property "currentIndex" of "s" is 3

      Examples:
        | given |
        | 9     |
        | -1    |

    Scenario: A model that comes to have fewer rows than the index goes to the first
      When the property "currentIndex" of "t" is set to 8
      And the property "model" of "t" is set to 4
      And 1 second passes
      Then the property "currentIndex" of "t" is 0

    Scenario: Giving a model to an empty tumbler makes the first row current
      When the property "model" of "e" is set to 7
      Then the property "currentIndex" of "e" is 0
      And the property "count" of "e" is 7

  @qt-parity
  @api-QtQuick.Controls.Tumbler.delegate
  Rule: Delegates

    Scenario Outline: A delegate is as wide as the tumbler and a fifth of its height
      When 1 second passes
      Then the property "<property>" of "d0" is within 0.5 of <value>

      Examples:
        | property | value |
        | width    | 60    |
        | height   | 40    |

    Scenario Outline: Rows are one below the other, the current one in the middle
      When 1 second passes
      Then the property "y" of "<row>" is within 0.5 of <y>

      Examples:
        | row | y   |
        | d9  | 40  |
        | d0  | 80  |
        | d1  | 120 |

    Scenario: A wrapping tumbler makes a delegate for each row it shows and one more
      When 1 second passes
      Then the object "d7" exists
      And the object "d2" exists
      And the object "d3" does not exist
      And the object "d6" does not exist

    Scenario: A delegate is a third of the height of a tumbler that shows three
      When 1 second passes
      Then the property "height" of "s0" is within 0.5 of 66.67
      And the property "width" of "s0" is within 0.5 of 60

    Scenario: Changing how many rows show changes how tall each is
      When the property "visibleItemCount" of "t" is set to 3
      And 1 second passes
      Then the property "height" of "d0" is within 0.5 of 66.67
      And the property "y" of "d0" is within 0.5 of 66.67
      And the object "d2" does not exist

  @qt-parity
  @api-QtQuick.Controls.Tumbler.tumbler @api-QtQuick.Controls.Tumbler.displacement
  Rule: What a delegate knows of the tumbler

    Scenario: A delegate reads the tumbler it is in
      When 1 second passes
      Then the property "mine" of "d0" is true

    Scenario Outline: A row is as many rows from the middle as its displacement, below being negative
      When 1 second passes
      Then the property "disp" of "<row>" is within 0.01 of <displacement>

      Examples:
        | row | displacement |
        | d0  | 0            |
        | d1  | -1           |
        | d2  | -2           |
        | d9  | 1            |
        | d8  | 2            |

    Scenario: The displacement follows the current index
      When the property "currentIndex" of "t" is set to 4
      And 1 second passes
      Then the property "disp" of "d4" is within 0.01 of 0
      And the property "disp" of "d5" is within 0.01 of -1

    Scenario: A tumbler that stops at its ends has displacements as well
      When 1 second passes
      Then the property "disp" of "s0" is within 0.01 of 0
      And the property "disp" of "s1" is within 0.01 of -1

    Scenario: Outside a delegate there is no tumbler and no displacement
      When the method "outside" of "root" is called
      Then the property "seen" of "root" is "|0"

  @qt-parity
  @api-QtQuick.Controls.Tumbler.positionViewAtIndex
  Rule: Positioning

    Scenario: The modes of positioning
      When the method "modes" of "root" is called
      Then the property "seen" of "root" is "0 1 2 3 4 5"

    Scenario: Positioning at the centre makes the row the current one
      When the method "positionViewAtIndex" of "t" is called with 4, 1
      And 1 second passes
      Then the property "currentIndex" of "t" is 4

  @qt-parity
  Rule: Keys

    Scenario: Down is the next row
      When the user presses "Down"
      And 1 second passes
      Then the property "currentIndex" of "t" is 1

    Scenario: Up from the first row of a wrapping tumbler is the last
      When the user presses "Up"
      And 1 second passes
      Then the property "currentIndex" of "t" is 9

    Scenario: Up from the first row of a tumbler that does not wrap stays
      When the property "focus" of "t" is set to false
      And the property "focus" of "s" is set to true
      And the user presses "Up"
      And 1 second passes
      Then the property "currentIndex" of "s" is 0

    Scenario: Down from the last row of a tumbler that does not wrap stays
      When the property "currentIndex" of "s" is set to 5
      And the property "focus" of "t" is set to false
      And the property "focus" of "s" is set to true
      And the user presses "Down"
      And 1 second passes
      Then the property "currentIndex" of "s" is 5

  @qt-parity
  Rule: The pointer

    Scenario: A short drag up comes back to the same row
      When the user drags from 30, 150 to 30, 125
      And 1 second passes
      Then the property "currentIndex" of "t" is 0

    Scenario: A drag of a row and a half up goes to the next row
      When the user drags from 30, 150 to 30, 90
      And 1 second passes
      Then the property "currentIndex" of "t" is 1

    Scenario: A tumbler that does not wrap stops at its first row
      When the user drags from 130, 50 to 130, 150
      And 1 second passes
      Then the property "currentIndex" of "s" is 0

    Scenario: A drag past the end of a tumbler that does not wrap goes to a row
      When the user drags from 130, 150 to 130, 50
      And 1 second passes
      Then the property "currentIndex" of "s" is 1

    Scenario: A tumbler is moving while it is dragged
      When the user presses the pointer at 30, 150
      And the user moves the pointer to 30, 110
      And the user moves the pointer to 30, 100
      Then the property "moving" of "t" is true
      And the property "currentIndex" of "t" is 0

    Scenario: A tumbler that was let go is moving until it rests
      When the user drags from 30, 150 to 30, 90
      Then the property "moving" of "t" is true
      When 2 seconds pass
      Then the property "moving" of "t" is false

    Scenario: A press on a row does not make it the current one
      When the user presses the pointer at 30, 170
      And the user releases the pointer at 30, 170
      And 1 second passes
      Then the property "currentIndex" of "t" is 0

  Rule: The wheel

    @ljos-only
    Scenario: A notch of the wheel goes to the next row
      When the user turns the wheel 1 notch down at 30, 100
      And 1 second passes
      Then the property "currentIndex" of "t" is 1

    @ljos-only
    Scenario: A notch up from the first row of a wrapping tumbler is the last
      When the user turns the wheel 1 notch up at 30, 100
      And 1 second passes
      Then the property "currentIndex" of "t" is 9

    @ljos-only
    Scenario: The wheel stops at the end of a tumbler that does not wrap
      When the user turns the wheel 1 notch up at 130, 100
      And 1 second passes
      Then the property "currentIndex" of "s" is 0

  @a11y @ljos-only
  Rule: What an assistive technology reads

    Scenario: A tumbler is a spin button
      Given an assistive technology is listening
      Then the object "t" is exposed as a "spinbutton" named ""

    Scenario: A tumbler tells its place among the rows
      Given an assistive technology is listening
      When the property "currentIndex" of "t" is set to 4
      Then the "spinbutton" named "" is at 4 in the range 0 to 9

    Scenario: An assistive technology can ask for the next row
      Given an assistive technology is listening
      When an assistive technology asks the "spinbutton" named "" to increase
      And 1 second passes
      Then the property "currentIndex" of "t" is 1
