# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qml-qtqml-models-listelement.html
#   https://doc.qt.io/qt-6/qml-qtquick-column.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Repeaters in positioners and in each other
  Delegates take part in the positioner that holds the Repeater, and a delegate may hold a
  Repeater of its own.

  Scenario: A Column stacks the delegates of a Repeater
    Given the QML:
      """qml
      import QtQuick
      Column {
        objectName: "col"
        spacing: 2
        Item { objectName: "head"; width: 30; height: 4 }
        Repeater {
          model: 3
          Item { objectName: "d" + index; width: 10 + index; height: 10 }
        }
        Item { objectName: "foot"; width: 5; height: 4 }
      }
      """
    When 100 milliseconds pass
    Then the property "y" of "d0" is 6
    And the property "y" of "d1" is 18
    And the property "y" of "d2" is 30
    And the property "y" of "foot" is 42
    And the property "height" of "col" is 46
    And the property "width" of "col" is 30

  Scenario: An inner Repeater sees its own index and reaches the outer row by id
    Given the QML:
      """qml
      import QtQuick
      Column {
        Repeater {
          model: 2
          Row {
            id: line
            objectName: "line" + index
            property int outerRow: index
            Repeater {
              model: 3
              Item { objectName: "c" + line.outerRow + index; width: 10; height: 10 }
            }
          }
        }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "c02" is 20
    And the property "x" of "c11" is 10
    And the property "y" of "line1" is 10
    And the property "width" of "line0" is 30
    And the object "c13" does not exist

  Rule: A list inside a ListElement is a model of its own

    Background:
      Given the QML:
        """qml
        import QtQuick
        Column {
          id: root
          objectName: "root"
          property int inner: -1
          function grow() { lm.get(0).parts.append({tag: "w"}); inner = lm.get(0).parts.count }
          ListModel {
            id: lm
            ListElement { name: "a"; parts: [ ListElement { tag: "x" }, ListElement { tag: "y" } ] }
            ListElement { name: "b"; parts: [ ListElement { tag: "z" } ] }
          }
          Repeater {
            model: lm
            Row {
              id: line
              objectName: "line-" + name
              property string rowName: name
              Repeater {
                objectName: "inner-" + line.rowName
                model: parts
                Item { objectName: line.rowName + tag; width: 10; height: 10 }
              }
            }
          }
        }
        """
      When 100 milliseconds pass

    Scenario: The nested list drives an inner Repeater
      Then the property "count" of "inner-a" is 2
      And the property "count" of "inner-b" is 1
      And the property "x" of "ay" is 10
      And the property "y" of "line-b" is 10

    Scenario: The nested list can be changed through get
      When the method "grow" of "root" is called
      And 100 milliseconds pass
      Then the property "inner" of "root" is 3
      And the property "count" of "inner-a" is 3
      And the property "x" of "aw" is 20

  Rule: A nested list reaches an inner delegate through required properties

    Background:
      Given the QML:
        """qml
        import QtQuick
        Column {
          id: root
          objectName: "root"
          property string seen: ""
          function read() { seen = "" + lm.get(0).parts.get(1).done + lm.get(1).parts.get(0).done }
          ListModel { id: lm
            ListElement { parts: [ ListElement { tag: "x"; done: false }, ListElement { tag: "y"; done: false } ] }
            ListElement { parts: [ ListElement { tag: "z"; done: false } ] } }
          Repeater {
            model: lm
            Row {
              required property var parts
              Repeater { model: parts
                Item {
                  required property string tag
                  required property bool done
                  objectName: "tag-" + tag
                  property bool shown: done
                  function flip() { done = !done }
                }
              }
            }
          }
        }
        """
      When 100 milliseconds pass

    Scenario: The inner delegate is created with the roles of its element
      Then the property "shown" of "tag-x" is false
      And the property "shown" of "tag-z" is false

    Scenario: Writing a required property of an inner delegate changes the delegate only
      When the method "flip" of "tag-y" is called
      And the method "read" of "root" is called
      Then the property "seen" of "root" is "falsefalse"
      And the property "shown" of "tag-y" is true

  Rule: A nested list declared as list<var> is a plain list, not a model

    Scenario: An inner delegate has no roles to read from a list<var> property
      Given the QML:
        """qml
        import QtQuick
        Column {
          ListModel {
            id: lm
            ListElement { parts: [ ListElement { tag: "x" } ] }
          }
          Repeater {
            model: lm
            Row {
              required property list<var> parts
              Repeater {
                model: parts
                Item { required property string tag; objectName: "tag-" + tag }
              }
            }
          }
        }
        """
      When 100 milliseconds pass
      Then the object "tag-x" does not exist

    Scenario: An appended array of objects read through list<var> has no roles either
      Given the QML:
        """qml
        import QtQuick
        Column {
          ListModel { id: lm }
          Component.onCompleted: lm.append({ parts: [ { tag: "y" } ] })
          Repeater {
            model: lm
            Row {
              required property list<var> parts
              Repeater {
                model: parts
                Item { required property string tag; objectName: "tag-" + tag }
              }
            }
          }
        }
        """
      When 100 milliseconds pass
      Then the object "tag-y" does not exist

  Rule: An array of objects appended to a ListModel is a model of its own

    Scenario: An inner delegate of an appended row has the roles of its objects
      Given the QML:
        """qml
        import QtQuick
        Column {
          ListModel { id: lm }
          Component.onCompleted: lm.append({ parts: [ { tag: "x", done: true }, { tag: "y", done: false } ] })
          Repeater {
            model: lm
            Row {
              required property var parts
              Repeater { model: parts
                Item { required property string tag; required property bool done; objectName: "tag-" + tag; property bool shown: done }
              }
            }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "shown" of "tag-x" is true
      And the property "shown" of "tag-y" is false
