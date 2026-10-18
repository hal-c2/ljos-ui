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

  Rule: A change to values and bindings alone leaves every object where it is

    The objects go on as they are and only the edited properties are set
    again, so what nothing records (an animation half way) goes on too.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text { objectName: "label"; text: "old" }
          function go() { slide.start() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass

    Scenario: A running animation goes on when a text beside it changes
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text { objectName: "label"; text: "new" }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "text" of "label" is "new"
      And the property "running" of "slide" is true
      And the property "x" of "box" is at least 40

  Rule: The rows of a view take an edited binding of their delegate, also the rows made later

    Background:
      Given the QML:
        """qml
        import QtQuick
        Column {
          id: root; objectName: "root"
          property int rows: 2
          Repeater {
            model: root.rows
            Text { objectName: "row" + index; text: "row " + index }
          }
        }
        """

    Scenario: Every row shows the new binding
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Column {
          id: root; objectName: "root"
          property int rows: 2
          Repeater {
            model: root.rows
            Text { objectName: "row" + index; text: "line " + index }
          }
        }
        """
      And a reload happens
      Then the property "text" of "row0" is "line 0"
      And the property "text" of "row1" is "line 1"
      When the property "rows" of "root" is set to 3
      Then the property "text" of "row2" is "line 2"

  Rule: A change to the body of a handler or a function leaves every object where it is

    The objects go on as they are. The next time the signal comes, the
    handler runs as the file now writes it; the next call of the function
    does too, and so do the bindings that call it. A value made a binding,
    or a binding made a value, is set again the same way.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int hits: 0
          signal poke
          onPoke: hits += 1
          Rectangle {
            objectName: "box"; width: 10; height: root.hits + 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text { objectName: "label"; text: "step " + root.step() }
          function step() { return 1 }
          function add() { hits += step() }
          function tap() { poke() }
          function go() { slide.start() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass

    Scenario: A handler runs its new body on the next signal
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int hits: 0
          signal poke
          onPoke: hits += 10
          Rectangle {
            objectName: "box"; width: 10; height: root.hits + 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text { objectName: "label"; text: "step " + root.step() }
          function step() { return 1 }
          function add() { hits += step() }
          function tap() { poke() }
          function go() { slide.start() }
        }
        """
      And a reload happens
      When the method "tap" of "root" is called
      Then the property "hits" of "root" is 10
      And the property "running" of "slide" is true
      And the property "x" of "box" is at least 40

    Scenario: A function runs its new body, also in the bindings that call it
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int hits: 0
          signal poke
          onPoke: hits += 1
          Rectangle {
            objectName: "box"; width: 10; height: root.hits + 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text { objectName: "label"; text: "step " + root.step() }
          function step() { return 5 }
          function add() { hits += step() }
          function tap() { poke() }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "text" of "label" is "step 5"
      When the method "add" of "root" is called
      Then the property "hits" of "root" is 5
      And the property "running" of "slide" is true
      And the property "x" of "box" is at least 40

    Scenario: A value made a binding follows what it reads
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int hits: 0
          signal poke
          onPoke: hits += 1
          Rectangle {
            objectName: "box"; width: root.hits + 20; height: root.hits + 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text { objectName: "label"; text: "step " + root.step() }
          function step() { return 1 }
          function add() { hits += step() }
          function tap() { poke() }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "width" of "box" is 20
      When the method "tap" of "root" is called
      Then the property "width" of "box" is 21
      And the property "running" of "slide" is true
      And the property "x" of "box" is at least 40

    Scenario: A binding made a value no longer follows what it read
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int hits: 0
          signal poke
          onPoke: hits += 1
          Rectangle {
            objectName: "box"; width: 10; height: 4
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text { objectName: "label"; text: "step " + root.step() }
          function step() { return 1 }
          function add() { hits += step() }
          function tap() { poke() }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "height" of "box" is 4
      When the method "tap" of "root" is called
      Then the property "height" of "box" is 4
      And the property "running" of "slide" is true
      And the property "x" of "box" is at least 40

  Rule: A change that moves lines leaves every object where it is

    A line added above an object moves it and all that follows. The
    objects go on as they are, and what they run is told by the line the
    file now gives it.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var thing: ({ name: "a" })
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text { objectName: "label"; text: root.thing.name }
          function go() { slide.start() }
          function drop() { thing = null }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass

    Scenario: A member added on a line of its own
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var thing: ({ name: "a" })
          Rectangle {
            objectName: "box"; width: 10; height: 10
            opacity: 0.5
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text { objectName: "label"; text: root.thing.name }
          function go() { slide.start() }
          function drop() { thing = null }
        }
        """
      And a reload happens
      Then the property "opacity" of "box" is 0.5
      And the property "running" of "slide" is true
      And the property "x" of "box" is at least 40

    Scenario: A binding that fails after a line was added above it is told at its new line
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var thing: ({ name: "a" })
          // A line added above the objects.
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text { objectName: "label"; text: root.thing.name }
          function go() { slide.start() }
          function drop() { thing = null }
        }
        """
      And a reload happens
      And the method "drop" of "root" is called
      Then the log has a warning matching "Scenario.qml:10:"
      And the log has no warning matching "Scenario.qml:9:"
      And the property "running" of "slide" is true
      And the property "x" of "box" is at least 40
