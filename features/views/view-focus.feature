# Sources:
#   https://doc.qt.io/qt-6/qtquick-input-focus.html (focus scopes, "Use Case: Using ListView")
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#currentItem-prop
# Verified on Qt 6.11.2 (qml6 and qmltestrunner, offscreen).

@qt-parity
Feature: The current item of a view has the focus in it
  A ListView, GridView or PathView is a focus scope, and its current item
  is the item in it with `focus`. With the view focused, the current
  delegate has the active focus, so what it holds takes the keys; when
  the current row moves, the focus goes with it. A delegate's focus
  stays inside the view and takes none from the items around it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 240; height: 320; property string log: ""
        Item { objectName: "other"; focus: true; width: 10; height: 10 }
        ListView {
          id: list; objectName: "list"; y: 20; width: 240; height: 100
          model: ListModel { ListElement { name: "Bob" } ListElement { name: "John" } ListElement { name: "Michael" } }
          delegate: FocusScope {
            objectName: "row" + index; width: 100; height: 20
            TextInput {
              objectName: "input" + index; width: 100; height: 20; focus: true; text: name
              Keys.onReturnPressed: root.log += text + " "
            }
          }
        }
        GridView {
          y: 130; width: 240; height: 60; cellWidth: 60; cellHeight: 30; model: 4
          delegate: Item { objectName: "cell" + index; width: 50; height: 20 }
        }
        PathView {
          objectName: "path"; y: 200; width: 240; height: 100; model: 4
          path: Path { startX: 0; startY: 50; PathLine { x: 240; y: 50 } }
          delegate: Item { objectName: "stop" + index; width: 20; height: 20 }
        }
      }
      """
    When 100 milliseconds pass

  Scenario Outline: The current item of a <view> has the focus, and the item outside keeps its own
    Then the property "focus" of "<current>" is true
    And the property "focus" of "<other>" is false
    And the property "activeFocus" of "other" is true

    Examples:
      | view     | current | other |
      | ListView | row0    | row1  |
      | GridView | cell0   | cell1 |
      | PathView | stop0   | stop1 |

  @api-QtQuick.ListView.currentItem
  Scenario: With the view focused, the current delegate takes the keys
    When the method "forceActiveFocus" of "list" is called
    Then the property "activeFocus" of "input0" is true
    When the user types "x"
    And the user presses "Return"
    Then the property "text" of "input0" is "Bobx"
    And the property "log" of "root" is "Bobx "

  Scenario: The focus goes with the current row
    When the method "forceActiveFocus" of "list" is called
    And the user presses "Down"
    And 100 milliseconds pass
    Then the property "currentIndex" of "list" is 1
    And the property "focus" of "row0" is false
    And the property "activeFocus" of "input1" is true
    When the user presses "Return"
    Then the property "log" of "root" is "John "
    When the property "currentIndex" of "list" is set to 2
    And 100 milliseconds pass
    Then the property "activeFocus" of "input2" is true

  Scenario: No current row leaves the focus where it was
    When the property "currentIndex" of "list" is set to -1
    And 100 milliseconds pass
    Then the property "focus" of "row0" is true
