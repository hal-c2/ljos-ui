# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A Repeater follows its model
  When rows are added, removed or moved, only the delegates of those rows are created or destroyed.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Row {
        id: root
        objectName: "root"
        property int created: 0
        property string log: ""
        function swap() { rep.model = ["p", "q", "r"] }
        ListModel {
          id: lm; objectName: "lm"
          ListElement { name: "a" }
          ListElement { name: "b" }
          ListElement { name: "c" }
        }
        Repeater {
          id: rep
          objectName: "rep"
          model: lm
          onItemAdded: (i, item) => root.log += "+" + i
          onItemRemoved: (i, item) => root.log += "-" + i
          Item {
            objectName: "d-" + (model.name ?? modelData); width: 10; height: 10
            Component.onCompleted: root.created++
          }
        }
      }
      """
    When 100 milliseconds pass

  Scenario: Every row has a delegate at the start
    Then the property "created" of "root" is 3
    And the property "log" of "root" is "+0+1+2"
    And the property "x" of "d-c" is 20

  Scenario: Appending a row creates one delegate
    When the method "append" of "lm" is called with {"name": "z"}
    And 100 milliseconds pass
    Then the property "created" of "root" is 4
    And the property "count" of "rep" is 4
    And the property "log" of "root" is "+0+1+2+3"
    And the property "x" of "d-z" is 30

  Scenario: Inserting a row creates one delegate at its place
    When the method "insert" of "lm" is called with 1, {"name": "z"}
    And 100 milliseconds pass
    Then the property "created" of "root" is 4
    And the property "log" of "root" is "+0+1+2+1"
    And the property "x" of "d-a" is 0
    And the property "x" of "d-z" is 10
    And the property "x" of "d-b" is 20
    And the property "x" of "d-c" is 30

  Scenario: Removing a row destroys its delegate only
    When the method "remove" of "lm" is called with 1
    And 100 milliseconds pass
    Then the property "created" of "root" is 3
    And the property "count" of "rep" is 2
    And the property "log" of "root" is "+0+1+2-1"
    And the object "d-b" does not exist
    And the property "x" of "d-c" is 10

  Scenario: Removing several rows reports the first row each time
    When the method "remove" of "lm" is called with 0, 2
    Then the property "log" of "root" is "+0+1+2-0-0"
    And the property "count" of "rep" is 1

  Scenario: Moving a row moves its delegate without creating one
    When the method "move" of "lm" is called with 0, 2, 1
    And 100 milliseconds pass
    Then the property "created" of "root" is 3
    And the property "log" of "root" is "+0+1+2"
    And the property "x" of "d-b" is 0
    And the property "x" of "d-c" is 10
    And the property "x" of "d-a" is 20

  Scenario: Changing a role creates nothing
    When the method "setProperty" of "lm" is called with 1, "name", "z"
    Then the property "created" of "root" is 3
    And the object "d-z" exists
    And the object "d-b" does not exist

  Scenario: Clearing the model destroys every delegate
    When the method "clear" of "lm" is called
    Then the property "count" of "rep" is 0
    And the object "d-a" does not exist
    And the property "log" of "root" is "+0+1+2-0-0-0"

  Scenario: A new model replaces every delegate
    When the method "swap" of "root" is called
    And 100 milliseconds pass
    Then the property "created" of "root" is 6
    And the property "count" of "rep" is 3
    And the object "d-a" does not exist
    And the property "x" of "d-r" is 20
    And the property "log" of "root" is "+0+1+2-2-1-0+0+1+2"
