# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#positionViewAtEnd-method
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#positionViewAtIndex-method
#   https://github.com/hal-c2/ljos-ui/issues/79
# Verified on Qt 6.11.2 (qmltestrunner, offscreen): 6 delegates and one
# move for the end, 5 and one for the beginning, 7 and two for a row.
# `made` counts the delegates made before the method returns: rows of the
# cache buffer that come afterwards are not what it costs.

Feature: What positioning a list costs
  A view sent to a row, its end or its beginning works out where that is
  and goes there once: it makes delegates only for the rows it shows
  there, and an attached scroll bar follows without moving the view.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 300; height: 400
        property int made: 0; property int count: 0; property int moves: 0
        property bool bar: false
        function measure(f) { count = 0; moves = 0; f(); made = count }
        function atEnd() { measure(() => list.positionViewAtEnd()) }
        function atBeginning() { measure(() => list.positionViewAtBeginning()) }
        function atRow(i, mode) { measure(() => list.positionViewAtIndex(i, mode)) }
        ListView {
          id: list; objectName: "list"; anchors.fill: parent; model: 400; spacing: 6
          onContentYChanged: root.moves++
          ScrollBar.vertical: root.bar ? bar : null
          delegate: Column {
            required property int index
            objectName: "r" + index; width: ListView.view.width
            Component.onCompleted: root.count++
            Text { width: parent.width; wrapMode: Text.Wrap
                   text: "Row " + index + " " + "lorem ipsum ".repeat(1 + (index * 7) % 23) }
            Repeater { model: index % 5; Rectangle { width: 200; height: 12 } }
          }
        }
        ScrollBar { id: bar; objectName: "bar"; x: 290; height: 400 }
      }
      """
    When the method "positionViewAtEnd" of "list" is called
    And 100 milliseconds pass
    And the method "positionViewAtBeginning" of "list" is called
    And 100 milliseconds pass

  @qt-parity
  Scenario: The end of rows it measured before is reached in one move
    When the method "atEnd" of "root" is called
    Then the property "moves" of "root" is 1
    And the property "made" of "root" is at most 8
    And the object "r399" exists

  @qt-parity
  Scenario: With a scroll bar the end is still reached in one move
    When the property "bar" of "root" is set to true
    And the method "atEnd" of "root" is called
    Then the property "moves" of "root" is 1
    And the property "made" of "root" is at most 8
    And the property "position" of "bar" is within 0.01 of 0.99

  @qt-diverges
  Scenario: The end is where the last row ends
    # Qt 6.11 leaves the last row 16 pixels below the view here.
    When the method "positionViewAtEnd" of "list" is called
    And 100 milliseconds pass
    Then the property "atYEnd" of "list" is true

  @qt-diverges
  Scenario: With a scroll bar the end is where the last row ends
    When the property "bar" of "root" is set to true
    And the method "positionViewAtEnd" of "list" is called
    And 100 milliseconds pass
    Then the property "atYEnd" of "list" is true

  @qt-parity
  Scenario: The beginning is reached in one move
    When the method "positionViewAtEnd" of "list" is called
    And 100 milliseconds pass
    And the method "atBeginning" of "root" is called
    Then the property "moves" of "root" is 1
    And the property "made" of "root" is at most 7
    And the property "contentY" of "list" is 0

  @qt-parity
  Scenario: With a scroll bar the beginning is reached in one move
    When the property "bar" of "root" is set to true
    And the method "positionViewAtEnd" of "list" is called
    And 100 milliseconds pass
    And the method "atBeginning" of "root" is called
    Then the property "moves" of "root" is 1
    And the property "made" of "root" is at most 7
    And the property "position" of "bar" is 0

  @qt-parity
  Scenario: A row far away is reached making only the rows around it
    When the property "bar" of "root" is set to true
    And the method "atRow" of "root" is called with 200, 1
    Then the property "moves" of "root" is at most 2
    And the property "made" of "root" is at most 9
    And the object "r200" exists

  @ljos-only
  Scenario: While a screen reader is active the rows it keeps come afterwards
    # A reader keeps rows on either side of what is shown; going to the
    # end makes those after it has got there, not on the way.
    When the system setting "screen reader" changes to true
    And 100 milliseconds pass
    And the property "bar" of "root" is set to true
    And the method "atEnd" of "root" is called
    Then the property "moves" of "root" is 1
    And the property "made" of "root" is at most 8
