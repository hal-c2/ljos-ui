# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-instantiator.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Instantiator
  An Instantiator creates one object per row of its model; the objects need not be items.

  Scenario: Without a model one object is created
    Given the QML:
      """qml
      import QtQuick
      Item {
        Instantiator {
          id: inst
          objectName: "inst"
          property string first: object ? object.objectName : "none"
          QtObject { objectName: "only" + index }
        }
      }
      """
    Then the property "count" of "inst" is 1
    And the property "model" of "inst" is 1
    And the property "active" of "inst" is true
    And the property "asynchronous" of "inst" is false
    And the property "first" of "inst" is "only0"

  Rule: The objects follow the model

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string log: ""
          property string picked: "?"
          function pick(i) { var o = inst.objectAt(i); picked = o ? o.label : "none" }
          ListModel {
            id: lm
            objectName: "lm"
            ListElement { name: "a" }
            ListElement { name: "b" }
          }
          Instantiator {
            id: inst
            objectName: "inst"
            model: lm
            delegate: QtObject { property string label: name + index }
            onObjectAdded: (i, o) => root.log += "+" + i + o.label
            onObjectRemoved: (i, o) => root.log += "-" + i
          }
        }
        """

    Scenario: One object per row, reported by objectAdded
      Then the property "count" of "inst" is 2
      And the property "log" of "root" is "+0a0+1b1"

    Scenario Outline: objectAt returns the object of a row
      When the method "pick" of "root" is called with <row>
      Then the property "picked" of "root" is "<label>"

      Examples:
        | row | label |
        | 0   | a0    |
        | 1   | b1    |
        | 2   | none  |

    Scenario: A new row creates one object
      When the method "append" of "lm" is called with {"name": "c"}
      Then the property "count" of "inst" is 3
      And the property "log" of "root" is "+0a0+1b1+2c2"

    Scenario: A removed row destroys its object and the rest move up
      When the method "remove" of "lm" is called with 0
      And the method "pick" of "root" is called with 0
      Then the property "count" of "inst" is 1
      And the property "log" of "root" is "+0a0+1b1-0"
      And the property "picked" of "root" is "b0"

    Scenario: An inactive Instantiator has no objects
      When the property "active" of "inst" is set to false
      And the method "pick" of "root" is called with 0
      Then the property "count" of "inst" is 0
      And the property "picked" of "root" is "none"
      And the property "log" of "root" is "+0a0+1b1-0-1"

    Scenario: Activating it again creates the objects again
      When the property "active" of "inst" is set to false
      And the property "active" of "inst" is set to true
      Then the property "count" of "inst" is 2
      And the property "log" of "root" is "+0a0+1b1-0-1+0a0+1b1"

  Rule: Creation can be put off

    Scenario: An Instantiator that starts inactive creates nothing until it is activated
      Given the QML:
        """qml
        import QtQuick
        Item {
          Instantiator {
            objectName: "inst"
            active: false
            model: 3
            QtObject { objectName: "o" + index }
          }
        }
        """
      Then the property "count" of "inst" is 0
      And the object "o0" does not exist
      When the property "active" of "inst" is set to true
      Then the property "count" of "inst" is 3

    Scenario: An asynchronous Instantiator has its objects after a moment
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string log: ""
          Instantiator {
            objectName: "inst"
            asynchronous: true
            model: 2
            QtObject { objectName: "o" + index }
            onObjectAdded: (i, o) => root.log += "+" + o.objectName
          }
        }
        """
      When 100 milliseconds pass
      Then the property "count" of "inst" is 2
      And the property "log" of "root" is "+o0+o1"
