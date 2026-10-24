# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (positionViewAtEnd, positionViewAtIndex, reuseItems)
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollbar.html (attaching ScrollBar to a Flickable)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A list goes where it is sent while its rows take their height
  Rows made at one height that then take the height of the text in them
  are not as long as the view took them to be. A list of them goes to its
  end when sent there, and a contentY that is written is kept, with or
  without a ScrollBar attached: the rows that come into sight take their
  height around where the view was put.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: win
        objectName: "win"; width: 400; height: 300
        property bool bar: false
        readonly property bool lastShown: view.indexAt(10, view.contentY + view.height - 1) === 199
        function toEnd() { view.positionViewAtEnd() }
        ListView {
          id: view; objectName: "list"; anchors.fill: parent; model: 200
          ScrollBar.vertical: win.bar ? scrollBar : null
          delegate: Item {
            id: row
            required property int index
            width: ListView.view.width; height: made ? text.implicitHeight : 40
            property bool made: false
            Component.onCompleted: made = true
            Text {
              id: text; width: parent.width; wrapMode: Text.Wrap
              text: row.index + " " + "word ".repeat(1 + (row.index * 37) % 200)
            }
          }
        }
        ScrollBar { id: scrollBar; objectName: "bar" }
      }
      """
    When 100 milliseconds pass

  Scenario: Sent to its end, the list shows its last row
    When the method "toEnd" of "win" is called
    And 100 milliseconds pass
    Then the property "lastShown" of "win" is true

  Scenario: Sent to its end twice, the list is at its end
    When the method "toEnd" of "win" is called
    And 100 milliseconds pass
    And the method "toEnd" of "win" is called
    And 100 milliseconds pass
    Then the property "atYEnd" of "list" is true
    And the property "lastShown" of "win" is true

  Scenario: A contentY that is written is kept
    When the property "contentY" of "list" is set to 2884
    And 100 milliseconds pass
    Then the property "contentY" of "list" is 2884

  Scenario: A contentY that is written is kept with a ScrollBar attached
    When the property "bar" of "win" is set to true
    And 100 milliseconds pass
    And the property "contentY" of "list" is set to 2884
    And 100 milliseconds pass
    Then the property "contentY" of "list" is 2884
    When the property "contentY" of "list" is set to 6000
    And 100 milliseconds pass
    Then the property "contentY" of "list" is 6000
