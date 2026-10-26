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

  Rule: A function of an outer component is called by its name

    Scenario: Each delegate calls a function of the root
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int total: 0
          function add(n) { total += n; return total }
          Repeater {
            model: 3
            Item { Component.onCompleted: add(index + 1) }
          }
        }
        """
      Then the property "total" of "root" is 6

    Scenario: A delegate's binding calls a function of the root
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string prefix: "row"
          function describe(i) { return prefix + " " + i }
          Repeater {
            model: 2
            Item { objectName: "row" + index; property string label: describe(index) }
          }
        }
        """
      Then the property "label" of "row1" is "row 1"
      When the property "prefix" of "root" is set to "line"
      Then the property "label" of "row1" is "line 1"

    Scenario: A loaded component calls a function of the root
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int total: 0
          function add(n) { total += n }
          Component { id: later; Item { Component.onCompleted: add(10) } }
          Loader { sourceComponent: later }
          Loader { sourceComponent: Item { Component.onCompleted: add(100) } }
        }
        """
      Then the property "total" of "root" is 110

    Scenario: A component file's delegate calls a function of that file's root
      Given the file "Panel.qml":
        """qml
        import QtQuick
        Item {
          objectName: "panel"
          property int total: 0
          function add(n) { total += n }
          Repeater {
            model: 2
            Item { Component.onCompleted: add(5) }
          }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item { Panel { } }
        """
      Then the property "total" of "panel" is 10

  Rule: A file's root finds what its own file does not have where it is made

    Scenario: A file whose root is not an item reads the members of the file it is made in
      Given the file "Look.qml":
        """qml
        import QtQuick
        QtObject {
          objectName: "look"
          property int seen: foo
          property bool sameParent: parent === outer.parent
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: outer
          objectName: "root"
          property int foo: 7
          Look { }
        }
        """
      Then the property "seen" of "look" is 7
      And the property "sameParent" of "look" is true
      When the property "foo" of "root" is set to 8
      Then the property "seen" of "look" is 8

    Scenario: A Binding in a file of its own finds its target where it is made
      Given the file "Hold.qml":
        """qml
        import QtQuick
        Binding { target: panel; property: "width"; value: 42 }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          property Item panel: box
          Item { id: box; objectName: "panel" }
          Hold { }
        }
        """
      Then the property "width" of "panel" is 42

    Scenario: The nearest file that has the name is the one read
      Given the file "Look.qml":
        """qml
        import QtQuick
        QtObject { objectName: "look"; property int seen: foo }
        """
      And the file "Middle.qml":
        """qml
        import QtQuick
        Item { property int foo: 99; Look { } }
        """
      And the QML:
        """qml
        import QtQuick
        Item { property int foo: 7; Middle { } }
        """
      Then the property "seen" of "look" is 99

    Scenario: A delegate's own member comes before the row's
      Given the file "Look.qml":
        """qml
        import QtQuick
        QtObject { objectName: "look"; property string seen: modelData }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          Repeater { model: ["row"]; Item { property string modelData: "own"; Look { } } }
        }
        """
      Then the property "seen" of "look" is "own"

    Scenario: A Loader's item reads the Loader's members first
      Given the file "Look.qml":
        """qml
        import QtQuick
        QtObject { objectName: "look"; property string seen: foo + " " + bar }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int foo: 7
          property int bar: 1
          property string seen: loader.item ? loader.item.seen : ""
          Loader { id: loader; property int foo: 3; source: "Look.qml" }
        }
        """
      Then the property "seen" of "root" is "3 1"
