# Sources:
#   https://doc.qt.io/qt-6/qtqml-documents-scope.html
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Names of the component around
  A component written inside another sees the names of the one around it. Assigning to such a
  name changes the object that has it, as reading it reads that object.

  Rule: Assigning to a name of an outer component changes the outer object

    Scenario: Each delegate counts itself in properties of the root
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int created: 0
          property int total: 0
          Repeater {
            model: 3
            Item { Component.onCompleted: { created++; total += index } }
          }
        }
        """
      Then the property "created" of "root" is 3
      And the property "total" of "root" is 3

    Scenario: A method of a loaded component assigns to properties outside it
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string picked: "none"
          property int hits: 0
          property int doubled: hits * 2
          Loader {
            sourceComponent: Item {
              objectName: "inner"
              property int seen: hits
              function pick(name) { picked = name; hits += 1; return hits }
            }
          }
        }
        """
      When the method "pick" of "inner" is called with "b"
      Then the property "picked" of "root" is "b"
      And the property "hits" of "root" is 1
      And the property "doubled" of "root" is 2
      And the property "seen" of "inner" is 1

    Scenario: A delegate inside a delegate assigns through both
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int created: 0
          Repeater {
            model: 2
            Item {
              objectName: "outer" + index
              property int mine: 0
              Repeater {
                model: 3
                Item { Component.onCompleted: { mine += index; created++ } }
              }
            }
          }
        }
        """
      Then the property "created" of "root" is 6
      And the property "mine" of "outer1" is 3

    Scenario: The nearest object that has the name is the one assigned
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int count: 0
          Repeater {
            model: 1
            Item {
              objectName: "near"
              property int count: 10
              function bump() { count++ }
            }
          }
        }
        """
      When the method "bump" of "near" is called
      Then the property "count" of "near" is 11
      And the property "count" of "root" is 0
