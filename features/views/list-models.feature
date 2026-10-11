# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html
#   https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.model @api-QtQuick.ListView.delegate @api-QtQuick.ListView.count
Feature: A ListView shows the rows of its model
  A number, an array, a list property and a ListModel can all be the model of a view.

  Background:
    Given the file "Kinds.qml":
      """qml
      import QtQml
      QtObject {
        property list<string> words: ["p", "q"]
        property list<QtObject> things: [
          QtObject { objectName: "t0" }, QtObject { objectName: "t1" }, QtObject { objectName: "t2" }
        ]
        property var mixed: [10, "y", true]
      }
      """

  Scenario Outline: The row count and modelData of each kind of model
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 300
        property string seen: ""
        Kinds { id: kinds }
        ListView {
          objectName: "list"
          anchors.fill: parent
          model: <model>
          delegate: Item {
            width: 200; height: 20
            Component.onCompleted: root.seen += "[" + index + ":" + <shown> + "]"
          }
        }
      }
      """
    Then the property "count" of "list" is <count>
    And the property "seen" of "root" is "<seen>"
    And the property "contentHeight" of "list" is <height>

    Examples:
      | model        | shown                | count | seen                | height |
      | 3            | modelData            | 3     | [0:0][1:1][2:2]     | 60     |
      | ["x", "y"]   | modelData            | 2     | [0:x][1:y]          | 40     |
      | kinds.mixed  | model.modelData      | 3     | [0:10][1:y][2:true] | 60     |
      | kinds.words  | modelData            | 2     | [0:p][1:q]          | 40     |
      | kinds.things | modelData.objectName | 3     | [0:t0][1:t1][2:t2]  | 60     |
      | 0            | modelData            | 0     |                     | 0      |

  Scenario: A view without a model has no rows
    Given the QML:
      """qml
      import QtQuick
      ListView {
        objectName: "list"
        width: 200; height: 300
        delegate: Item { objectName: "row"; width: 200; height: 20 }
      }
      """
    Then the property "count" of "list" is 0
    And the property "currentIndex" of "list" is -1
    And the property "currentItem" of "list" is null
    And the object "row" does not exist

  Scenario: A view without a delegate has no rows
    Given the QML:
      """qml
      import QtQuick
      ListView {
        objectName: "list"
        width: 200; height: 300
        model: 4
      }
      """
    Then the property "count" of "list" is 0
    And the property "contentHeight" of "list" is 0
    And the log has no warnings

  Scenario: Giving another model replaces every row
    Given the QML:
      """qml
      import QtQuick
      ListView {
        objectName: "list"
        width: 200; height: 300
        model: 3
        delegate: Item { objectName: "r" + modelData; width: 200; height: 20 }
      }
      """
    When the property "model" of "list" is set to ["a", "b"]
    And 100 milliseconds pass
    Then the property "count" of "list" is 2
    And the property "contentHeight" of "list" is 40
    And the property "y" of "rb" is 20
    And the object "r2" does not exist

  Scenario: Giving another delegate makes every row again
    Given the QML:
      """qml
      import QtQuick
      ListView {
        id: list
        objectName: "list"
        width: 200; height: 300
        model: 3
        function swap() { list.delegate = tall }
        delegate: Item { objectName: "short" + index; width: 200; height: 20 }
        Component { id: tall; Item { objectName: "tall" + index; width: 200; height: 50 } }
      }
      """
    When the method "swap" of "list" is called
    And 100 milliseconds pass
    Then the property "contentHeight" of "list" is 150
    And the property "y" of "tall2" is 100
    And the object "short0" does not exist
