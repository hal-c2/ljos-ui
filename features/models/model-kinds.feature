# Sources:
#   https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Kinds of model
  A number, an array, a list property, an object and a ListModel can all be a model.

  Background:
    Given the file "Kinds.qml":
      """qml
      import QtQml
      QtObject {
        property list<string> words: ["p", "q"]
        property list<QtObject> things: [
          QtObject { objectName: "t0" }, QtObject { objectName: "t1" }, QtObject { objectName: "t2" }
        ]
      }
      """

  Scenario Outline: The row count and modelData of each kind of model
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string seen: ""
        Kinds { id: kinds }
        Repeater {
          objectName: "rep"
          model: <model>
          Item { Component.onCompleted: root.seen += "[" + index + ":" + <shown> + "]" }
        }
      }
      """
    Then the property "count" of "rep" is <count>
    And the property "seen" of "root" is "<seen>"

    Examples:
      | model                | shown                | count | seen                |
      | 3                    | modelData            | 3     | [0:0][1:1][2:2]     |
      | ["x", "y"]           | modelData            | 2     | [0:x][1:y]          |
      | [10, "y", true]      | model.modelData      | 3     | [0:10][1:y][2:true] |
      | kinds.words          | modelData            | 2     | [0:p][1:q]          |
      | kinds.things         | modelData.objectName | 3     | [0:t0][1:t1][2:t2]  |
      | kinds.things[1]      | modelData.objectName | 1     | [0:t1]              |
      | [{n: "u"}, {n: "v"}] | modelData.n          | 2     | [0:u][1:v]          |
      | "abc"                | modelData            | 3     | [0:a][1:b][2:c]     |
      | null                 | modelData            | 0     |                     |
      | undefined            | modelData            | 0     |                     |
      | -2                   | modelData            | 0     |                     |

  Scenario: The properties of an object in the model are roles
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        property list<QtObject> things: [t0, t1]
        QtObject { id: t0; objectName: "t0"; property int n: 4 }
        QtObject { id: t1; objectName: "t1"; property int n: 5 }
        Repeater {
          model: root.things
          Item {
            objectName: "d" + index
            property int viaName: n
            property int viaModel: model.n
          }
        }
      }
      """
    Then the property "viaName" of "d0" is 4
    And the property "viaModel" of "d1" is 5
    When the property "n" of "t1" is set to 9
    Then the property "viaName" of "d1" is 9
    And the property "viaModel" of "d1" is 9

  Scenario: An array of JavaScript objects is read through modelData only
    Given the QML:
      """qml
      import QtQuick
      Item {
        Repeater {
          model: [{n: 4}, {n: 5}]
          Item {
            objectName: "d" + index
            property int viaData: modelData.n
            property string viaModel: typeof model.n
            property string viaName: typeof n
          }
        }
      }
      """
    Then the property "viaData" of "d1" is 5
    And the property "viaModel" of "d1" is "undefined"
    And the property "viaName" of "d1" is "undefined"

  Scenario: A negative number is reported and gives no rows
    Given the QML:
      """qml
      import QtQuick
      Item {
        Repeater { objectName: "rep"; model: -2; Item {} }
      }
      """
    Then the property "count" of "rep" is 0
    And the log has a warning matching "Model size of -2 is less than 0"

  Scenario: Without a delegate nothing is created
    Given the QML:
      """qml
      import QtQuick
      Item {
        Repeater { objectName: "rep"; model: 3 }
      }
      """
    Then the property "count" of "rep" is 0
