# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#destruction-signal
#   https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html#deleting-objects-dynamically
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Component.onDestruction
  An object made from a component tells its Component.onDestruction handlers before it goes.

  Rule: The objects of one component hear of it children first

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string events: ""
          property string early: "unset"
          property var made: null
          Component {
            id: comp
            Item {
              id: top
              objectName: "top"
              Component.onDestruction: root.events += "top" + children.length + (parent === root) + " "
              Item {
                Component.onDestruction: root.events += "middle "
                Item { Component.onDestruction: root.events += "bottom " }
              }
              QtObject { Component.onDestruction: root.events += "plain " }
            }
          }
          function make() { made = comp.createObject(root) }
          function drop() { made.destroy(); early = events }
          Component.onCompleted: make()
        }
        """

    Scenario: Nothing is told while the object lives
      Then the property "events" of "root" is ""

    Scenario: Children are told before their parent
      When the method "drop" of "root" is called
      And 10 milliseconds pass
      Then the property "events" of "root" is "bottom middle plain top1true "

    Scenario: destroy() does not take the object away under the running function
      When the method "drop" of "root" is called
      Then the property "early" of "root" is ""

    Scenario: The object is gone after its handlers have run
      When the method "drop" of "root" is called
      And 10 milliseconds pass
      Then the object "root/top" does not exist

  Rule: Objects made from another component hear of it after the file that uses them

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string events: ""
          component Leaf: Item {
            property string name
            Component.onDestruction: root.events += name + " "
            Item { Component.onDestruction: root.events += parent.name + ".inner " }
          }
          Component {
            id: comp
            Item {
              Component.onDestruction: root.events += "top "
              Leaf { name: "first"; Component.onDestruction: root.events += "(first) " }
              Item {
                objectName: "inline"
                Component.onDestruction: root.events += "inline "
                Leaf { name: "second" }
              }
            }
          }
          property var made: comp.createObject(root)
          function drop() { made.destroy() }
          function dropInline() { made.children[1].destroy() }
        }
        """

    Scenario: The last one made is told first
      When the method "drop" of "root" is called
      And 10 milliseconds pass
      Then the property "events" of "root" is "inline top second.inner second first.inner first (first) "

    Scenario: An object written inline is not told when only it is destroyed
      When the method "dropInline" of "root" is called
      And 10 milliseconds pass
      Then the property "events" of "root" is "second.inner second "
      And the object "root/inline" does not exist
