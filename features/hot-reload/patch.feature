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

  Rule: A function a script made before lines moved is told at the line it is now on

    A callback given to `connect` or a binding made with `Qt.binding`
    was made from the file as it was. What it throws after a change that
    moved its lines is told at the line the file now gives it.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var thing: ({ name: "a" })
          property string bound
          signal poke
          Component.onCompleted: {
            poke.connect(function () {
              console.log(thing.name)
            })
            bound = Qt.binding(function () {
              return thing.name
            })
          }
          function drop() { thing = null; poke() }
        }
        """

    Scenario: A callback and a binding made before a line was added above them
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var thing: ({ name: "a" })
          // A line added above them.
          property string bound
          signal poke
          Component.onCompleted: {
            poke.connect(function () {
              console.log(thing.name)
            })
            bound = Qt.binding(function () {
              return thing.name
            })
          }
          function drop() { thing = null; poke() }
        }
        """
      And a reload happens
      And the method "drop" of "root" is called
      Then the log has a warning matching "Scenario.qml:10:"
      And the log has a warning matching "Scenario.qml:13:"
      And the log has no warning matching "Scenario.qml:9:"
      And the log has no warning matching "Scenario.qml:12:"

  Rule: An object added to one that stays, or one removed, leaves every other object where it is

    The added object is made into the object it is written in, at the
    place the file gives it among its siblings, and runs
    `Component.onCompleted` as any object made does; a removed one is
    destroyed. The others go on as they are, an animation half way too.

    Background:
      Given the file "Badge.qml":
        """qml
        import QtQuick
        Rectangle { property int size: 4; width: size; height: size }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string word: "two"
          property int made: 0
          property int seen: -1
          property string last: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Column {
            id: list; objectName: "list"
            Text { objectName: "first"; text: "one" }
          }
          function go() { slide.start() }
          function probe() {
            seen = list.children.length
            last = seen > 0 ? list.children[seen - 1].objectName : ""
          }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass

    Scenario: An added object is there, last among its siblings, and runs onCompleted once
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string word: "two"
          property int made: 0
          property int seen: -1
          property string last: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Column {
            id: list; objectName: "list"
            Text { objectName: "first"; text: "one" }
            Text {
              objectName: "second"; text: root.word
              Component.onCompleted: root.made += 1
            }
          }
          function go() { slide.start() }
          function probe() {
            seen = list.children.length
            last = seen > 0 ? list.children[seen - 1].objectName : ""
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "text" of "second" is "two"
      And the property "made" of "root" is 1
      And the property "seen" of "root" is 2
      And the property "last" of "root" is "second"
      And the property "running" of "slide" is true
      And the property "x" of "box" is at least 40
      When the property "word" of "root" is set to "three"
      Then the property "text" of "second" is "three"

    Scenario: An added object made from a file of the project is there
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string word: "two"
          property int made: 0
          property int seen: -1
          property string last: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Column {
            id: list; objectName: "list"
            Text { objectName: "first"; text: "one" }
            Badge { objectName: "badge"; size: 6 }
          }
          function go() { slide.start() }
          function probe() {
            seen = list.children.length
            last = seen > 0 ? list.children[seen - 1].objectName : ""
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "width" of "badge" is 6
      And the property "last" of "root" is "badge"
      And the property "running" of "slide" is true
      And the property "x" of "box" is at least 40

    Scenario: A removed object is gone
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string word: "two"
          property int made: 0
          property int seen: -1
          property string last: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Column {
            id: list; objectName: "list"
          }
          function go() { slide.start() }
          function probe() {
            seen = list.children.length
            last = seen > 0 ? list.children[seen - 1].objectName : ""
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the object "first" does not exist
      And the property "seen" of "root" is 0
      And the property "running" of "slide" is true
      And the property "x" of "box" is at least 40

    Scenario: A property edited and an object added in one change
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string word: "two"
          property int made: 0
          property int seen: -1
          property string last: ""
          Rectangle {
            objectName: "box"; width: 30; height: 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Column {
            id: list; objectName: "list"
            Text { objectName: "first"; text: second.text + "!" }
            Text { id: second; objectName: "second"; text: root.word }
          }
          function go() { slide.start() }
          function probe() {
            seen = list.children.length
            last = seen > 0 ? list.children[seen - 1].objectName : ""
          }
        }
        """
      And a reload happens
      Then the property "width" of "box" is 30
      And the property "text" of "first" is "two!"
      And the property "running" of "slide" is true
      And the property "x" of "box" is at least 40
      When the property "word" of "root" is set to "three"
      Then the property "text" of "first" is "three!"

    Scenario: An object added before one that stays comes before it
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string word: "two"
          property int made: 0
          property int seen: -1
          property string last: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Column {
            id: list; objectName: "list"
            Text { objectName: "zero"; text: "zero" }
            Text { objectName: "first"; text: "one" }
          }
          function go() { slide.start() }
          function probe() {
            seen = list.children.length
            last = seen > 0 ? list.children[seen - 1].objectName : ""
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "seen" of "root" is 2
      And the property "last" of "root" is "first"
      And the property "running" of "slide" is true
      And the property "x" of "box" is at least 40

    Scenario: An object of another type added before one that stays is of its own type
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string word: "two"
          property int made: 0
          property int seen: -1
          property string last: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Column {
            id: list; objectName: "list"
            Rectangle { objectName: "zero"; width: 3; height: 3; radius: 1 }
            Text { objectName: "first"; text: "one" }
          }
          function go() { slide.start() }
          function probe() {
            seen = list.children.length
            last = seen > 0 ? list.children[seen - 1].objectName : ""
          }
        }
        """
      And a reload happens
      Then the property "width" of "zero" is 3
      And the property "radius" of "zero" is 1
      And the property "text" of "first" is "one"
      And the property "running" of "slide" is true
      And the property "x" of "box" is at least 40

  Rule: A page a StackView pushed from a file has objects added where it stands

    The page is made once from its file, as a root is, so it grows in
    place too.

    Background:
      Given the file "Page.qml":
        """qml
        import QtQuick
        Rectangle {
          objectName: "page"; width: 10; height: 10
          property int count: 0
          property int seen: -1
          NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          Item { objectName: "a" }
          function go() { slide.start() }
          function probe() { seen = children.length }
        }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          objectName: "root"
          width: 300; height: 200
          StackView { id: stack; objectName: "stack"; anchors.fill: parent }
          function openFile() { stack.push("Page.qml") }
        }
        """
      When the method "openFile" of "root" is called
      And the property "count" of "page" is set to 5
      And the method "go" of "page" is called
      And 500 milliseconds pass

    Scenario: An object added to the page is there and the page goes on as it was
      When the file "Page.qml" is changed to:
        """qml
        import QtQuick
        Rectangle {
          objectName: "page"; width: 10; height: 10
          property int count: 0
          property int seen: -1
          NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          Item { objectName: "a" }
          Text { objectName: "b"; text: "b" + count }
          function go() { slide.start() }
          function probe() { seen = children.length }
        }
        """
      And a reload happens
      And the method "probe" of "page" is called
      Then the property "seen" of "page" is 2
      And the property "text" of "b" is "b5"
      And the property "depth" of "stack" is 1
      And the property "running" of "slide" is true
      And the property "x" of "page" is at least 40

  Rule: The rows of a view or a Repeater have objects added where they stand

    An object added to a delegate, or removed from it, is added to or
    removed from every row there is, in place; the rows made later are
    made from the delegate as the file now writes it. A row goes on as it
    was, an animation half way too.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          ListModel { id: things; ListElement { name: "a" } ListElement { name: "b" } }
          Repeater {
            model: things
            Rectangle {
              objectName: "cell" + name; width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
              Item { objectName: "spare" + name }
            }
          }
          ListView {
            width: 50; height: 100; model: things
            delegate: Rectangle {
              objectName: "line" + name; width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            }
          }
          function more() { things.append({ name: "c" }) }
        }
        """
      When 500 milliseconds pass

    Scenario: An object added to a delegate is in every row, and the rows go on as they were
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          ListModel { id: things; ListElement { name: "a" } ListElement { name: "b" } }
          Repeater {
            model: things
            Rectangle {
              objectName: "cell" + name; width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
              Item { objectName: "spare" + name }
              Text { objectName: "label" + name; text: "row " + name }
            }
          }
          ListView {
            width: 50; height: 100; model: things
            delegate: Rectangle {
              objectName: "line" + name; width: 10; height: 10
              Text { objectName: "tag" + name; text: "line " + name }
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            }
          }
          function more() { things.append({ name: "c" }) }
        }
        """
      And a reload happens
      Then the property "text" of "labela" is "row a"
      And the property "text" of "labelb" is "row b"
      And the property "text" of "tagb" is "line b"
      And the property "x" of "cella" is at least 40
      And the property "x" of "linea" is at least 40
      When the method "more" of "root" is called
      Then the property "text" of "labelc" is "row c"
      And the property "text" of "tagc" is "line c"

    Scenario: An object removed from a delegate is gone from every row
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          ListModel { id: things; ListElement { name: "a" } ListElement { name: "b" } }
          Repeater {
            model: things
            Rectangle {
              objectName: "cell" + name; width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            }
          }
          ListView {
            width: 50; height: 100; model: things
            delegate: Rectangle {
              objectName: "line" + name; width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            }
          }
          function more() { things.append({ name: "c" }) }
        }
        """
      And a reload happens
      Then the object "sparea" does not exist
      And the object "spareb" does not exist
      And the property "x" of "cella" is at least 40
      When the method "more" of "root" is called
      Then the object "cellc" exists
      And the object "sparec" does not exist

    Scenario: A property edited and an object added in a delegate in one change
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          ListModel { id: things; ListElement { name: "a" } ListElement { name: "b" } }
          Repeater {
            model: things
            Rectangle {
              objectName: "cell" + name; width: 30; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
              Item { objectName: "spare" + name }
              Text { objectName: "label" + name; text: "row " + name }
            }
          }
          ListView {
            width: 50; height: 100; model: things
            delegate: Rectangle {
              objectName: "line" + name; width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            }
          }
          function more() { things.append({ name: "c" }) }
        }
        """
      And a reload happens
      Then the property "width" of "cella" is 30
      And the property "text" of "labelb" is "row b"
      And the property "x" of "cellb" is at least 40

  Rule: What a Loader made from a component written inline has objects added where it stands

    Scenario: An object added to a Loader's inline component is there and the object goes on as it was
      Given the QML:
        """qml
        import QtQuick
        Loader {
          objectName: "loader"
          sourceComponent: Rectangle {
            objectName: "inside"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Loader {
          objectName: "loader"
          sourceComponent: Rectangle {
            objectName: "inside"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            Text { objectName: "note"; text: "inside" }
          }
        }
        """
      And a reload happens
      Then the property "text" of "note" is "inside"
      And the property "x" of "inside" is at least 40

  Rule: An object moved or given another type is made again where the file puts it, and only that object

    An object put in another order among its siblings, under another
    parent, or given another type is destroyed and made again as the file
    now writes it, at its new place; a binding that names it by its id
    names the new object. Every other object goes on as it was, an
    animation half way too.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string order: ""
          property string tag: one.objectName
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Column {
            id: list; objectName: "list"
            Text { id: one; objectName: "one"; text: "one" }
            Text { objectName: "two"; text: "two" }
          }
          Item { id: side; objectName: "side" }
          function probe() {
            var r = []
            for (var i = 0; i < list.children.length; i++) r.push(list.children[i].objectName)
            order = r.join(",") + "|" + side.children.length
          }
        }
        """
      When 500 milliseconds pass

    Scenario: Siblings put in another order are in that order
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string order: ""
          property string tag: one.objectName
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Column {
            id: list; objectName: "list"
            Text { objectName: "two"; text: "two" }
            Text { id: one; objectName: "one"; text: "one" }
          }
          Item { id: side; objectName: "side" }
          function probe() {
            var r = []
            for (var i = 0; i < list.children.length; i++) r.push(list.children[i].objectName)
            order = r.join(",") + "|" + side.children.length
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "two,one|0"
      And the property "tag" of "root" is "one"
      And the property "x" of "box" is at least 40

    Scenario: An object moved under another parent is there
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string order: ""
          property string tag: one.objectName
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Column {
            id: list; objectName: "list"
            Text { objectName: "two"; text: "two" }
          }
          Item {
            id: side; objectName: "side"
              Text { id: one; objectName: "one"; text: "one" }
          }
          function probe() {
            var r = []
            for (var i = 0; i < list.children.length; i++) r.push(list.children[i].objectName)
            order = r.join(",") + "|" + side.children.length
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "two|1"
      And the property "text" of "one" is "one"
      And the property "x" of "box" is at least 40

    Scenario: An object given another type is of that type, and its id names it
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string order: ""
          property string tag: one.objectName
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Column {
            id: list; objectName: "list"
            Rectangle { id: one; objectName: "uno"; width: 7; height: 7 }
            Text { objectName: "two"; text: "two" }
          }
          Item { id: side; objectName: "side" }
          function probe() {
            var r = []
            for (var i = 0; i < list.children.length; i++) r.push(list.children[i].objectName)
            order = r.join(",") + "|" + side.children.length
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "width" of "uno" is 7
      And the property "tag" of "root" is "uno"
      And the property "order" of "root" is "uno,two|0"
      And the property "x" of "box" is at least 40

  Rule: Other changes to objects make them again, as the file now writes them

    An object moved, or given another type or another id, is there as the
    file writes it once the reload is through.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Column {
          id: root; objectName: "root"
          property string last: ""
          property string tag: a.objectName
          Text { id: a; objectName: "a"; text: "a" }
          Text { objectName: "b"; text: "b" }
          Repeater {
            model: 2
            Item { objectName: "row" + index }
          }
          function probe() { last = children[0].objectName }
        }
        """

    Scenario: An object given another type is of that type
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Column {
          id: root; objectName: "root"
          property string last: ""
          property string tag: a.objectName
          Text { id: a; objectName: "a"; text: "a" }
          Rectangle { objectName: "b"; width: 7; height: 7 }
          Repeater {
            model: 2
            Item { objectName: "row" + index }
          }
          function probe() { last = children[0].objectName }
        }
        """
      And a reload happens
      Then the property "width" of "b" is 7

    Scenario: An id given to another object names that object
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Column {
          id: root; objectName: "root"
          property string last: ""
          property string tag: a.objectName
          Text { objectName: "a"; text: "a" }
          Text { id: a; objectName: "b"; text: "b" }
          Repeater {
            model: 2
            Item { objectName: "row" + index }
          }
          function probe() { last = children[0].objectName }
        }
        """
      And a reload happens
      Then the property "tag" of "root" is "b"
