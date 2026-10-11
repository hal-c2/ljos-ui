# Sources:
#   docs/ARCHITECTURE.md (section 12.1)
#   docs/research/03-runtime-concurrency-hot-reload.md
# Hot reload is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: What a reload changes
  A reload brings the running application to what the files now say: values,
  bindings, handlers, objects, components, imports and files. It applies all
  of a change or none of it.

  Rule: A changed property takes its new value or binding

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          property int n: 2
          property int scaled: n * 2
        }
        """

    Scenario: A changed value shows
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 20
          property int n: 2
          property int scaled: n * 2
        }
        """
      And a reload happens
      Then the property "width" of "root" is 20

    Scenario: A changed binding is evaluated and follows what it reads
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          property int n: 2
          property int scaled: n * 3
        }
        """
      And a reload happens
      Then the property "scaled" of "root" is 6
      When the property "n" of "root" is set to 5
      Then the property "scaled" of "root" is 15

    Scenario: A value that becomes a binding follows what it reads
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: n * 10
          property int n: 2
          property int scaled: n * 2
        }
        """
      And a reload happens
      Then the property "width" of "root" is 20

    Scenario: A property that is no longer set goes back to its default
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int n: 2
          property int scaled: n * 2
        }
        """
      And a reload happens
      Then the property "width" of "root" is 0

  Rule: A changed handler or function runs its new body

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int n: 0
          property string events: ""
          onNChanged: events += "old "
          function bump() { n += 1 }
        }
        """

    Scenario: A changed function
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int n: 0
          property string events: ""
          onNChanged: events += "old "
          function bump() { n += 10 }
        }
        """
      And a reload happens
      And the method "bump" of "root" is called
      Then the property "n" of "root" is 10

    Scenario: A changed signal handler
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int n: 0
          property string events: ""
          onNChanged: events += "new "
          function bump() { n += 1 }
        }
        """
      And a reload happens
      And the method "bump" of "root" is called
      Then the property "events" of "root" is "new "

    Scenario: A removed signal handler runs no more
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int n: 0
          property string events: ""
          function bump() { n += 1 }
        }
        """
      And a reload happens
      And the method "bump" of "root" is called
      Then the property "events" of "root" is ""

  Rule: Objects are added, removed and moved

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Item { objectName: "group" }
          Rectangle { objectName: "box" }
        }
        """

    Scenario: An added object is there
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Item { objectName: "group" }
          Rectangle { objectName: "box" }
          Rectangle { objectName: "extra" }
        }
        """
      And a reload happens
      Then the object "extra" exists

    Scenario: A removed object is gone
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Item { objectName: "group" }
        }
        """
      And a reload happens
      Then the object "box" does not exist

    Scenario: A moved object is below its new parent
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Item {
            objectName: "group"
            Rectangle { objectName: "box" }
          }
        }
        """
      And a reload happens
      Then the object "group/box" exists

  Rule: An object of a component whose properties changed keeps those whose name and type did not

    Background:
      Given the file "Counter.qml":
        """qml
        import QtQuick
        Item {
          property int count: 0
          property string label: "a"
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Counter { id: counter; objectName: "counter" }
        }
        """
      When the property "count" of "counter" is set to 3
      And the property "label" of "counter" is set to "mine"
      And the file "Counter.qml" is changed to:
        """qml
        import QtQuick
        Item {
          property int count: 0
          property real label: 1.5
          property bool extra: true
        }
        """
      And a reload happens

    Scenario: A property with the same name and type keeps its value
      Then the property "count" of "counter" is 3

    Scenario: A property whose type changed starts from the file
      Then the property "label" of "counter" is 1.5

    Scenario: A new property is there
      Then the property "extra" of "counter" is true

  Rule: Imports and files come and go

    Scenario: An import is added with a type it brings
      Given the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          objectName: "root"
          Button { objectName: "go"; text: "Go" }
        }
        """
      And a reload happens
      Then the property "text" of "go" is "Go"

    Scenario: A file is added and used
      Given the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      When the file "Badge.qml" is changed to:
        """qml
        import QtQuick
        Rectangle { objectName: "badge" }
        """
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Badge {}
        }
        """
      And a reload happens
      Then the object "badge" exists

    Scenario: A file is removed with its last use
      Given the file "Badge.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "badge" }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Badge {}
        }
        """
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      And the file "Badge.qml" is removed
      And a reload happens
      Then there is no reload error
      And the object "badge" does not exist

  Rule: All of a change is applied or none of it

    Background:
      Given the file "Badge.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "badge"; width: 10 }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          height: 10
          Badge {}
        }
        """
      When the file "Badge.qml" is changed to:
        """qml
        import QtQuick
        Rectangle { objectName: "badge"; width: 20 }
        """
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          height: 20 +
          Badge {}
        }
        """
      And a reload happens

    Scenario: A good file is not applied while another file of the change is broken
      Then the property "width" of "badge" is 10
      And the property "height" of "root" is 10

    Scenario: Both files are applied once the broken one is mended
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          height: 20
          Badge {}
        }
        """
      And a reload happens
      Then the property "width" of "badge" is 20
      And the property "height" of "root" is 20
