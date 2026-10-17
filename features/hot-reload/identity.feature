# Sources:
#   docs/ARCHITECTURE.md (section 12.1)
#   docs/research/03-runtime-concurrency-hot-reload.md
# Hot reload is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: Which object is the same object after a reload
  When a file changes while the application runs, an object of the new file
  is the same object as one of the old file when both have the same id;
  without an id the same objectName; in a view the same row; otherwise the
  same type at the same place among its siblings of that type. The same
  object keeps what the application assigned to it. Any other object is new.

  Rule: An id names the same object

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Rectangle { id: box; objectName: "box"; property int taps: 0; width: 10 }
        }
        """
      When the property "taps" of "box" is set to 3

    Scenario: An object with an id keeps its state while the file around it changes
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Text { text: "new" }
          Rectangle { id: box; objectName: "box"; property int taps: 0; width: 20 }
        }
        """
      And a reload happens
      Then the property "taps" of "box" is 3
      And the property "width" of "box" is 20

    Scenario: An object with an id keeps its state when it moves to another parent
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Item {
            objectName: "group"
            Rectangle { id: box; objectName: "box"; property int taps: 0; width: 10 }
          }
        }
        """
      And a reload happens
      Then the object "group/box" exists
      And the property "taps" of "box" is 3

    Scenario: An object whose id was renamed is a new object
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Rectangle { id: panel; objectName: "box"; property int taps: 0; width: 10 }
        }
        """
      And a reload happens
      Then the property "taps" of "box" is 0

  Rule: Without an id, an objectName names the same object

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Rectangle { objectName: "first"; property int taps: 0 }
          Rectangle { objectName: "second"; property int taps: 0 }
        }
        """
      When the property "taps" of "second" is set to 3

    Scenario: A named object keeps its state when its siblings change places
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Rectangle { objectName: "second"; property int taps: 0 }
          Rectangle { objectName: "first"; property int taps: 0 }
        }
        """
      And a reload happens
      Then the property "taps" of "second" is 3
      And the property "taps" of "first" is 0

    Scenario: An object whose objectName changed is a new object
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Rectangle { objectName: "first"; property int taps: 0 }
          Rectangle { objectName: "third"; property int taps: 0 }
        }
        """
      And a reload happens
      Then the property "taps" of "third" is 0

  Rule: Without an id or a name, the type and the place among siblings of that type name the same object

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int seen: -1
          Rectangle { property int taps: 0 }
          Rectangle { property int taps: 0 }
          function tap(i) { children[i].taps = 3 }
          function probe(i) { seen = children[i].taps }
        }
        """

    Scenario: Two siblings of one type are told apart by their order
      When the method "tap" of "root" is called with 1
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int seen: -1
          width: 50
          Rectangle { property int taps: 0 }
          Rectangle { property int taps: 0 }
          function tap(i) { children[i].taps = 3 }
          function probe(i) { seen = children[i].taps }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called with 1
      Then the property "seen" of "root" is 3
      When the method "probe" of "root" is called with 0
      Then the property "seen" of "root" is 0

    Scenario: A new sibling of another type does not change which object it is
      When the method "tap" of "root" is called with 1
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int seen: -1
          Text { text: "new" }
          Rectangle { property int taps: 0 }
          Rectangle { property int taps: 0 }
          function tap(i) { children[i].taps = 3 }
          function probe(i) { seen = children[i].taps }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called with 2
      Then the property "seen" of "root" is 3

    # This is why the compiler asks for ids on such siblings.
    Scenario: A new sibling of the same type before it takes its place
      When the method "tap" of "root" is called with 0
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int seen: -1
          Rectangle { property int taps: 0; width: 5 }
          Rectangle { property int taps: 0 }
          Rectangle { property int taps: 0 }
          function tap(i) { children[i].taps = 3 }
          function probe(i) { seen = children[i].taps }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called with 0
      Then the property "seen" of "root" is 3
      When the method "probe" of "root" is called with 1
      Then the property "seen" of "root" is 0

  Rule: A delegate is the same object when it shows the same row

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Repeater {
            model: 3
            Rectangle { objectName: "cell" + index; property int taps: 0; width: 10 }
          }
        }
        """
      When the property "taps" of "cell1" is set to 4

    Scenario: The delegate of a row keeps its state when the delegate changes
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Repeater {
            model: 3
            Rectangle { objectName: "cell" + index; property int taps: 0; width: 20 }
          }
        }
        """
      And a reload happens
      Then the property "taps" of "cell1" is 4
      And the property "width" of "cell1" is 20
      And the property "taps" of "cell0" is 0

    Scenario: The delegate of a row the model no longer has is gone
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Repeater {
            model: 1
            Rectangle { objectName: "cell" + index; property int taps: 0; width: 10 }
          }
        }
        """
      And a reload happens
      Then the object "cell1" does not exist
      And the object "cell0" exists

  Rule: A delegate knows its row however it is given its data and wherever its Repeater is

    Scenario: A delegate that takes its roles as required properties keeps its state
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Repeater {
            model: ListModel { ListElement { name: "a" } ListElement { name: "b" } }
            Rectangle { required property string name; objectName: "cell" + name; property int taps: 0; width: 10 }
          }
        }
        """
      When the property "taps" of "cellb" is set to 4
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Repeater {
            model: ListModel { ListElement { name: "a" } ListElement { name: "b" } }
            Rectangle { required property string name; objectName: "cell" + name; property int taps: 0; width: 20 }
          }
        }
        """
      And a reload happens
      Then the property "taps" of "cellb" is 4
      And the property "width" of "cellb" is 20
      And the property "taps" of "cella" is 0

    Scenario: The rows of a Repeater in a container keep their state
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        TabBar {
          objectName: "root"
          Repeater {
            model: 3
            TabButton { objectName: "tab" + index; property int taps: 0; text: "one" }
          }
        }
        """
      When the property "taps" of "tab1" is set to 4
      And the property "taps" of "tab2" is set to 9
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        TabBar {
          objectName: "root"
          Repeater {
            model: 3
            TabButton { objectName: "tab" + index; property int taps: 0; text: "two" }
          }
        }
        """
      And a reload happens
      Then the property "taps" of "tab1" is 4
      And the property "taps" of "tab2" is 9
      And the property "text" of "tab1" is "two"

  Rule: An object of another type is a new object, and so is everything below it

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Rectangle {
            id: box; objectName: "box"; property int taps: 0
            Item { id: inner; objectName: "inner"; property int taps: 0 }
          }
        }
        """
      When the property "taps" of "box" is set to 3
      And the property "taps" of "inner" is set to 5
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Item {
            id: box; objectName: "box"; property int taps: 0
            Item { id: inner; objectName: "inner"; property int taps: 0 }
          }
        }
        """
      And a reload happens

    Scenario: A different type under the same id starts afresh
      Then the property "taps" of "box" is 0

    Scenario: The objects below it start afresh too
      Then the property "taps" of "inner" is 0
