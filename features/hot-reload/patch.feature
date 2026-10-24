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

    Scenario: A file that compiles again, which nothing uses, leaves the objects as they are
      Given the file "Spare.qml":
        """qml
        import QtQuick
        Item { Missing {} }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
        }
        """
      When 500 milliseconds pass
      And the file "Spare.qml" is changed to:
        """qml
        import QtQuick
        Item { objectName: "spare" }
        """
      And a reload happens
      Then the property "x" of "box" is at least 40
      And the object "spare" does not exist

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

    Scenario: An object added last in a control is before the parts the control is given
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Button {
          id: root; objectName: "root"
          property string order: ""
          background: Rectangle { objectName: "bg" }
          contentItem: Text { objectName: "ci"; text: "x" }
          Item { objectName: "mine" }
          function probe() {
            var r = []
            for (var i = 0; i < children.length; i++) r.push(children[i].objectName)
            order = r.join(",")
          }
        }
        """
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Button {
          id: root; objectName: "root"
          property string order: ""
          background: Rectangle { objectName: "bg" }
          contentItem: Text { objectName: "ci"; text: "x" }
          Item { objectName: "mine" }
          Item { objectName: "added" }
          function probe() {
            var r = []
            for (var i = 0; i < children.length; i++) r.push(children[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "mine,added,bg,ci"

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

  Rule: An object that holds a delegate written inline goes in place when it is removed or moved

    The delegate goes with the object, as its rows do; one moved is made
    again where the file now writes it, delegate and rows too. The
    delegates of the objects that stay keep their rows. A delegate
    removed from a view that stays takes its rows with it.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          ListModel { id: things; ListElement { name: "a" } ListElement { name: "b" } }
          ListView {
            objectName: "first"; width: 50; height: 100; model: things
            delegate: Rectangle { objectName: "one" + name; width: 10; height: 10 }
          }
          ListView {
            objectName: "second"; width: 50; height: 100; model: things
            delegate: Rectangle {
              objectName: "line" + name; width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            }
          }
          function more() { things.append({ name: "c" }) }
        }
        """
      When 500 milliseconds pass

    Scenario: A view whose delegate is written inline is removed, and the view after it keeps its rows
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          ListModel { id: things; ListElement { name: "a" } ListElement { name: "b" } }
          ListView {
            objectName: "second"; width: 50; height: 100; model: things
            delegate: Rectangle {
              objectName: "line" + name; width: 20; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            }
          }
          function more() { things.append({ name: "c" }) }
        }
        """
      And a reload happens
      Then the object "onea" does not exist
      And the property "x" of "linea" is at least 40
      And the property "width" of "linea" is 20
      When the method "more" of "root" is called
      Then the property "width" of "linec" is 20

    Scenario: A view's delegate written inline is removed, and the view stays without rows
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          ListModel { id: things; ListElement { name: "a" } ListElement { name: "b" } }
          ListView {
            objectName: "first"; width: 50; height: 100; model: things
          }
          ListView {
            objectName: "second"; width: 50; height: 100; model: things
            delegate: Rectangle {
              objectName: "line" + name; width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            }
          }
          function more() { things.append({ name: "c" }) }
        }
        """
      And a reload happens
      Then the object "onea" does not exist
      And the property "delegate" of "first" is null
      And the property "x" of "linea" is at least 40
      When the method "more" of "root" is called
      Then the object "onec" does not exist
      And the object "linec" exists

    Scenario: A view whose delegate is written inline is moved, and is made again there with its rows
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          ListModel { id: things; ListElement { name: "a" } ListElement { name: "b" } }
          Item {
            objectName: "box"
            ListView {
              objectName: "first"; width: 50; height: 100; model: things
              delegate: Rectangle { objectName: "one" + name; width: 10; height: 10 }
            }
          }
          ListView {
            objectName: "second"; width: 50; height: 100; model: things
            delegate: Rectangle {
              objectName: "line" + name; width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            }
          }
          function more() { things.append({ name: "c" }) }
        }
        """
      And a reload happens
      Then the object "onea" exists
      And the property "x" of "linea" is at least 40
      When the method "more" of "root" is called
      Then the object "onec" exists
      And the object "linec" exists

    Scenario: What a script made from a component written inline goes with it
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          ListModel { id: things; ListElement { name: "a" } ListElement { name: "b" } }
          Component { id: spare; Rectangle { objectName: "made"; width: 10; height: 10 } }
          ListView {
            objectName: "second"; width: 50; height: 100; model: things
            delegate: Rectangle {
              objectName: "line" + name; width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            }
          }
          function more() { things.append({ name: "c" }) }
          function make() { spare.createObject(root) }
        }
        """
      And a reload happens
      And the method "make" of "root" is called
      And 500 milliseconds pass
      Then the object "made" exists
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          ListModel { id: things; ListElement { name: "a" } ListElement { name: "b" } }
          ListView {
            objectName: "second"; width: 50; height: 100; model: things
            delegate: Rectangle {
              objectName: "line" + name; width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            }
          }
          function more() { things.append({ name: "c" }) }
          function make() { }
        }
        """
      And a reload happens
      Then the object "made" does not exist
      And the property "x" of "linea" is at least 40

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

    Scenario: An item moved before an object that is not an item is first among the children
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string order: ""
          ListModel { id: model; ListElement { n: 1 } }
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Text { objectName: "two"; text: "two" }
          Text { id: one; objectName: "one"; text: "one" }
          function probe() {
            var r = []
            for (var i = 0; i < children.length; i++) r.push(children[i].objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string order: ""
          Text { id: one; objectName: "one"; text: "one" }
          ListModel { id: model; ListElement { n: 1 } }
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Text { objectName: "two"; text: "two" }
          function probe() {
            var r = []
            for (var i = 0; i < children.length; i++) r.push(children[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "one,box,two"
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

  Rule: An object added before others in a list that sorts what it holds goes in place

    The content of a Flickable, a Pane, a Popup, a Menu, a container such
    as SwipeView, an application window and an ObjectModel takes an object
    added before one that stays where the file puts it, and so does a list
    of objects given to a member, such as states. The rest goes on as it
    was, an animation half way too.

    Scenario: An object added before another in a Flickable is in its place
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Flickable { id: holder; width: 50; height: 50
            Item { objectName: "a" }
            Item { objectName: "b" }
          }
          function probe() {
            var r = [], l = holder.contentItem.children
            for (var i = 0; i < l.length; i++) r.push(l[i].objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Flickable { id: holder; width: 50; height: 50
            Item { objectName: "a" }
            Item { objectName: "c" }
            Item { objectName: "b" }
          }
          function probe() {
            var r = [], l = holder.contentItem.children
            for (var i = 0; i < l.length; i++) r.push(l[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "a,c,b"
      And the property "x" of "box" is at least 40

    Scenario: An object added before another in a Pane is in its place
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Pane { id: holder
            Item { objectName: "a" }
            Item { objectName: "b" }
          }
          function probe() {
            var r = [], l = holder.contentChildren
            for (var i = 0; i < l.length; i++) r.push(l[i].objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Pane { id: holder
            Item { objectName: "a" }
            Item { objectName: "c" }
            Item { objectName: "b" }
          }
          function probe() {
            var r = [], l = holder.contentChildren
            for (var i = 0; i < l.length; i++) r.push(l[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "a,c,b"
      And the property "x" of "box" is at least 40

    Scenario: An object added before another in a Popup is in its place
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Popup { id: holder
            Item { objectName: "a" }
            Item { objectName: "b" }
          }
          function probe() {
            var r = [], l = holder.contentChildren
            for (var i = 0; i < l.length; i++) r.push(l[i].objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Popup { id: holder
            Item { objectName: "a" }
            Item { objectName: "c" }
            Item { objectName: "b" }
          }
          function probe() {
            var r = [], l = holder.contentChildren
            for (var i = 0; i < l.length; i++) r.push(l[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "a,c,b"
      And the property "x" of "box" is at least 40

    Scenario: An object added before another in a SwipeView is in its place
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          SwipeView { id: holder; width: 50; height: 50
            Item { objectName: "a" }
            Item { objectName: "b" }
          }
          function probe() {
            var r = [], l = holder.contentChildren
            for (var i = 0; i < l.length; i++) r.push(l[i].objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          SwipeView { id: holder; width: 50; height: 50
            Item { objectName: "a" }
            Item { objectName: "c" }
            Item { objectName: "b" }
          }
          function probe() {
            var r = [], l = holder.contentChildren
            for (var i = 0; i < l.length; i++) r.push(l[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "a,c,b"
      And the property "x" of "box" is at least 40

    Scenario: An object added before another in an ObjectModel is in its place
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          ObjectModel { id: holder
            Item { objectName: "a" }
            Item { objectName: "b" }
          }
          function probe() {
            var r = [], l = holder.children
            for (var i = 0; i < l.length; i++) r.push(l[i].objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          ObjectModel { id: holder
            Item { objectName: "a" }
            Item { objectName: "c" }
            Item { objectName: "b" }
          }
          function probe() {
            var r = [], l = holder.children
            for (var i = 0; i < l.length; i++) r.push(l[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "a,c,b"
      And the property "x" of "box" is at least 40

    Scenario: An object added before another in an ApplicationWindow is in its place
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        ApplicationWindow {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Item { objectName: "a" }
          Item { objectName: "b" }
          function probe() {
            var r = [], l = root.contentChildren
            for (var i = 0; i < l.length; i++) r.push(l[i].objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        ApplicationWindow {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Item { objectName: "a" }
          Item { objectName: "c" }
          Item { objectName: "b" }
          function probe() {
            var r = [], l = root.contentChildren
            for (var i = 0; i < l.length; i++) r.push(l[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "box,a,c,b"
      And the property "x" of "box" is at least 40

    Scenario: An object added before another in a Menu is in its place
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Menu { id: holder
            MenuItem { objectName: "a" }
            MenuItem { objectName: "b" }
          }
          function probe() {
            var r = []
            for (var i = 0; i < holder.count; i++) r.push(holder.itemAt(i).objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Menu { id: holder
            MenuItem { objectName: "a" }
            MenuItem { objectName: "c" }
            MenuItem { objectName: "b" }
          }
          function probe() {
            var r = []
            for (var i = 0; i < holder.count; i++) r.push(holder.itemAt(i).objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "a,c,b"
      And the property "x" of "box" is at least 40

    Scenario: An object added before another in a list given to states is in its place
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          states: [
            State { name: "one"; objectName: "one"; PropertyChanges { target: root; width: 11 } },
            State { name: "two"; objectName: "two"; PropertyChanges { target: root; width: 33 } },
          ]
          function probe() {
            var r = []
            for (var i = 0; i < states.length; i++) r.push(states[i].objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          states: [
            State { name: "one"; objectName: "one"; PropertyChanges { target: root; width: 11 } },
            State { name: "mid"; objectName: "mid"; PropertyChanges { target: root; width: 22 } },
            State { name: "two"; objectName: "two"; PropertyChanges { target: root; width: 33 } },
          ]
          function probe() {
            var r = []
            for (var i = 0; i < states.length; i++) r.push(states[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "one,mid,two"
      And the property "x" of "box" is at least 40
      When the property "state" of "root" is set to "mid"
      Then the property "width" of "root" is 22

    Scenario: An object added before another in a list given to transitions is in its place
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          transitions: [
            Transition { objectName: "one"; to: "a" },
            Transition { objectName: "two"; to: "b" },
          ]
          function probe() {
            var r = []
            for (var i = 0; i < transitions.length; i++) r.push(transitions[i].objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          transitions: [
            Transition { objectName: "one"; to: "a" },
            Transition { objectName: "mid"; to: "m" },
            Transition { objectName: "two"; to: "b" },
          ]
          function probe() {
            var r = []
            for (var i = 0; i < transitions.length; i++) r.push(transitions[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "one,mid,two"
      And the property "x" of "box" is at least 40

  Rule: An object removed from a list given to a member, or moved in it, goes in place

    A State or a Transition taken out of a list given to states or
    transitions leaves it, and one put in another place there is made again
    at its new place. The rest goes on as it was, an animation half way
    too.

    Scenario: A state removed from a list given to states leaves it
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          states: [
            State { name: "one"; objectName: "one"; PropertyChanges { target: root; width: 11 } },
            State { name: "mid"; objectName: "mid"; PropertyChanges { target: root; width: 22 } },
            State { name: "two"; objectName: "two"; PropertyChanges { target: root; width: 33 } },
          ]
          function probe() {
            var r = []
            for (var i = 0; i < states.length; i++) r.push(states[i].objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          states: [
            State { name: "one"; objectName: "one"; PropertyChanges { target: root; width: 11 } },
            State { name: "two"; objectName: "two"; PropertyChanges { target: root; width: 33 } },
          ]
          function probe() {
            var r = []
            for (var i = 0; i < states.length; i++) r.push(states[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "one,two"
      And the property "x" of "box" is at least 40
      When the property "state" of "root" is set to "two"
      Then the property "width" of "root" is 33

    Scenario: States in another order are in that order and still apply
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          states: [
            State { name: "one"; objectName: "one"; PropertyChanges { target: root; width: 11 } },
            State { name: "two"; objectName: "two"; PropertyChanges { target: root; width: 33 } },
          ]
          function probe() {
            var r = []
            for (var i = 0; i < states.length; i++) r.push(states[i].objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          states: [
            State { name: "two"; objectName: "two"; PropertyChanges { target: root; width: 33 } },
            State { name: "one"; objectName: "one"; PropertyChanges { target: root; width: 11 } },
          ]
          function probe() {
            var r = []
            for (var i = 0; i < states.length; i++) r.push(states[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "two,one"
      And the property "x" of "box" is at least 40
      When the property "state" of "root" is set to "one"
      Then the property "width" of "root" is 11

    Scenario: A transition removed from a list given to transitions leaves it
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          transitions: [
            Transition { objectName: "one"; to: "a" },
            Transition { objectName: "mid"; to: "m" },
            Transition { objectName: "two"; to: "b" },
          ]
          function probe() {
            var r = []
            for (var i = 0; i < transitions.length; i++) r.push(transitions[i].objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          transitions: [
            Transition { objectName: "one"; to: "a" },
            Transition { objectName: "two"; to: "b" },
          ]
          function probe() {
            var r = []
            for (var i = 0; i < transitions.length; i++) r.push(transitions[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "one,two"
      And the property "x" of "box" is at least 40

    Scenario: Transitions in another order are in that order
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          transitions: [
            Transition { objectName: "one"; to: "a" },
            Transition { objectName: "mid"; to: "m" },
            Transition { objectName: "two"; to: "b" },
          ]
          function probe() {
            var r = []
            for (var i = 0; i < transitions.length; i++) r.push(transitions[i].objectName)
            order = r.join(",")
          }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property string order: ""
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          transitions: [
            Transition { objectName: "two"; to: "b" },
            Transition { objectName: "one"; to: "a" },
            Transition { objectName: "mid"; to: "m" },
          ]
          function probe() {
            var r = []
            for (var i = 0; i < transitions.length; i++) r.push(transitions[i].objectName)
            order = r.join(",")
          }
        }
        """
      And a reload happens
      And the method "probe" of "root" is called
      Then the property "order" of "root" is "two,one,mid"
      And the property "x" of "box" is at least 40

  Rule: An id given to another object names it, and the rest goes on

    Bindings that name the id name the object that has it now. The objects
    the id moved between are made again; the others go on as they were, an
    animation half way too.

    Scenario: An id given to a sibling names the sibling
      Given the QML:
        """qml
        import QtQuick
        Column {
          id: root; objectName: "root"
          property string tag: a.objectName
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Text { id: a; objectName: "a"; text: "a" }
          Text { objectName: "b"; text: "b" }
        }
        """
      When 500 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Column {
          id: root; objectName: "root"
          property string tag: a.objectName
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { from: 0; to: 100; duration: 1000 }
          }
          Text { objectName: "a"; text: "a" }
          Text { id: a; objectName: "b"; text: "b" }
        }
        """
      And a reload happens
      Then the property "tag" of "root" is "b"
      And the property "text" of "a" is "a"
      And the property "x" of "box" is at least 40

    Scenario: An anchor and a component made inside follow an object moved among its siblings
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 100
          Item { objectName: "follower"; width: 10; anchors.left: mark.right }
          Loader {
            sourceComponent: Component {
              Item { objectName: "inner"; property real seen: mark.x }
            }
          }
          Item { id: mark; objectName: "mark"; x: 30; width: 20 }
        }
        """
      When 100 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 100
          Item { id: mark; objectName: "mark"; x: 60; width: 20 }
          Item { objectName: "follower"; width: 10; anchors.left: mark.right }
          Loader {
            sourceComponent: Component {
              Item { objectName: "inner"; property real seen: mark.x }
            }
          }
        }
        """
      And a reload happens
      Then the property "x" of "follower" is 80
      And the property "seen" of "inner" is 60

    Scenario: A file edited after one used in it was edited names the objects it did
      Given the file "Bar.qml":
        """qml
        import QtQuick
        Item {
          id: container
          property Item area
          Rectangle { width: 1 }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: window; objectName: "root"; width: 200; height: 100
          ListModel { id: feeds; ListElement { feed: "a" } }
          Bar { area: list }
          ListView {
            id: list; objectName: "list"
            width: 100; height: 50
            model: feeds
            delegate: Item { height: 10 }
          }
          Item { id: close; objectName: "close"; x: 30; width: 20 }
          Item { objectName: "follower"; x: close.x + 1 }
        }
        """
      When 100 milliseconds pass
      And the file "Bar.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: container
          property Item area
        }
        """
      And a reload happens
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: window; objectName: "root"; width: 200; height: 100
          Item { id: close; objectName: "close"; x: 60; width: 20 }
          ListModel { id: feeds; ListElement { feed: "a" } }
          Bar { area: list }
          ListView {
            id: list; objectName: "list"
            width: 100; height: 50
            model: feeds
            delegate: Item { height: 10 }
          }
          Item { objectName: "follower"; x: close.x + 1 }
        }
        """
      And a reload happens
      Then the property "count" of "list" is 1
      And the property "x" of "follower" is 61

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

  Rule: An object removed leaves the lists of the item it was shown in

    A popup's item is shown in the overlay, not in the object it is written
    in. Removing the popup takes it out of the overlay too.

    Scenario: A removed popup leaves the overlay
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        ApplicationWindow {
          objectName: "root"; width: 200; height: 200; visible: true
          property int n: Overlay.overlay ? Overlay.overlay.children.length : -1
          Dialog { width: 50; height: 50 }
          Item { objectName: "other" }
        }
        """
      When 100 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        ApplicationWindow {
          objectName: "root"; width: 200; height: 200; visible: true
          property int n: Overlay.overlay ? Overlay.overlay.children.length : -1
          Item { objectName: "other" }
        }
        """
      And a reload happens
      And 100 milliseconds pass
      Then the property "n" of "root" is 0

  Rule: What a script gave an object moved in place goes on through the reloads after

    The object made again for a moved one has what a script assigned to
    the old one, and a later reload that cannot go in place carries it
    as it carries what a script gave any object.

    Scenario: An object moved in place keeps the parent a script gave it through the next reload
      Given the file "Pack.qml":
        """qml
        import QtQuick
        Item {
          id: container; objectName: "pack"
          property Item area
          property string where: stack.parent ? stack.parent.objectName : ""
          onAreaChanged: stack.parent = area
          Item { id: stack; objectName: "stack" }
          Item { objectName: "holder" }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Item { id: area; objectName: "area"; property int n: children.length }
          Loader { source: "Pack.qml"; onItemChanged: item.area = area }
        }
        """
      When 100 milliseconds pass
      Then the property "where" of "pack" is "area"
      When the file "Pack.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: container; objectName: "pack"
          property Item area
          property string where: stack.parent ? stack.parent.objectName : ""
          onAreaChanged: stack.parent = area
          Item { objectName: "holder"; Item { id: stack; objectName: "stack" } }
        }
        """
      And a reload happens
      Then the property "where" of "pack" is "area"
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int extra: 1
          Item { id: area; objectName: "area"; property int n: children.length }
          Loader { source: "Pack.qml"; onItemChanged: item.area = area }
        }
        """
      And a reload happens
      Then the property "where" of "pack" is "area"
      And the property "n" of "area" is 1

  Rule: An edit to a member of a group or of an attached object leaves every object where it is

    `font.pixelSize`, `border.color` and `Layout.preferredWidth` are set
    again on the object that holds them, as a member of the object is.

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root; objectName: "root"
          Rectangle {
            objectName: "box"; width: 10; height: 10; border.color: "red"
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text {
            objectName: "label"; text: "a"; font.pixelSize: 10
            Layout.preferredWidth: 30; property real pw: Layout.preferredWidth
          }
          function go() { slide.start() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass

    Scenario: A member of a value group takes its new value
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root; objectName: "root"
          Rectangle {
            objectName: "box"; width: 10; height: 10; border.color: "red"
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text {
            objectName: "label"; text: "a"; font.pixelSize: 24
            Layout.preferredWidth: 30; property real pw: Layout.preferredWidth
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "font.pixelSize" of "label" is 24
      And the property "x" of "box" is at least 40

    Scenario: A member of a group made a binding follows what it reads
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root; objectName: "root"
          Rectangle {
            objectName: "box"; width: 10; height: 10; border.color: width > 5 ? "blue" : "red"
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text {
            objectName: "label"; text: "a"; font.pixelSize: 10
            Layout.preferredWidth: 30; property real pw: Layout.preferredWidth
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "border.color" of "box" is "#0000ff"
      And the property "x" of "box" is at least 40
      When the property "width" of "box" is set to 2
      Then the property "border.color" of "box" is "#ff0000"

    Scenario: A member of an attached object takes its new value
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root; objectName: "root"
          Rectangle {
            objectName: "box"; width: 10; height: 10; border.color: "red"
            NumberAnimation on x { id: slide; objectName: "slide"; running: false; from: 0; to: 100; duration: 1000 }
          }
          Text {
            objectName: "label"; text: "a"; font.pixelSize: 10
            Layout.preferredWidth: 50; property real pw: Layout.preferredWidth
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "pw" of "label" is 50
      And the property "x" of "box" is at least 40

  Rule: A member of a group a user of the component sets keeps the user's value

    Scenario: An edited font size of a component its user sets stays the user's
      Given the file "Caption.qml":
        """qml
        import QtQuick
        Text { text: "a"; font.pixelSize: 10 }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          Caption { objectName: "big"; font.pixelSize: 30 }
          Caption { objectName: "plain" }
        }
        """
      When the file "Caption.qml" is changed to:
        """qml
        import QtQuick
        Text { text: "a"; font.pixelSize: 20 }
        """
      And a reload happens
      Then the property "font.pixelSize" of "big" is 30
      And the property "font.pixelSize" of "plain" is 20

  Rule: A binding that reads an id no longer there starts as a fresh load starts it

    It cannot be worked out, so it is not kept from before: the property
    has the value it has when the file is loaded afresh.

    Scenario: A label bound to a removed object's visibility shows again
      Given the QML:
        """qml
        import QtQuick
        Item {
          Item { id: panel; visible: false }
          Text { objectName: "hint"; text: "a"; visible: panel.visible }
        }
        """
      Then the property "visible" of "hint" is false
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          Text { objectName: "hint"; text: "a"; visible: panel.visible }
        }
        """
      And a reload happens
      Then the property "visible" of "hint" is true

    Scenario: An id removed from a component's file names its user's object again
      Given the file "Panel.qml":
        """qml
        import QtQuick
        Item {
          property int rows: 1
          Item { id: settings; property int rows: 7 }
          Text { objectName: "hint"; text: "" + settings.rows }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          Panel { id: settings; rows: 3 }
        }
        """
      Then the property "text" of "hint" is "7"
      When the file "Panel.qml" is changed to:
        """qml
        import QtQuick
        Item {
          property int rows: 1
          Text { objectName: "hint"; text: "" + settings.rows }
        }
        """
      And a reload happens
      Then the property "text" of "hint" is "3"

    Scenario: An object removed that others read goes in place, and what read it starts afresh
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Item { id: panel; visible: false; property real size: 7 }
          Text { objectName: "hint"; text: "a"; visible: panel.visible; property real size: panel.size }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass
      Then the property "size" of "hint" is 7
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Text { objectName: "hint"; text: "a"; visible: panel.visible; property real size: panel.size }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "visible" of "hint" is true
      And the property "size" of "hint" is 0
      And the property "x" of "box" is at least 40

    Scenario: An item anchored to a removed object stands where a fresh load puts it
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          Item { id: mark; x: 50; y: 60; width: 10; height: 10 }
          Rectangle { objectName: "tag"; width: 5; height: 5; anchors.left: mark.right; anchors.top: mark.bottom }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass
      Then the property "x" of "tag" is 60
      And the property "y" of "tag" is 70
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          Rectangle { objectName: "tag"; width: 5; height: 5; anchors.left: mark.right; anchors.top: mark.bottom }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "x" of "tag" is 0
      And the property "y" of "tag" is 0
      And the property "x" of "box" is at least 40

    Scenario: An anchor edited to a removed object leaves the item at the x it gives
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          Item { id: mark; x: 50; y: 60; width: 10; height: 10 }
          Rectangle { objectName: "tag"; x: 7; width: 5; height: 5; anchors.left: mark.right; anchors.top: parent.top }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass
      Then the property "x" of "tag" is 60
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          Rectangle { objectName: "tag"; x: 7; width: 5; height: 5; anchors.left: mark.left; anchors.top: parent.top }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "x" of "tag" is 7
      And the property "x" of "box" is at least 40

  Rule: An edit to a component a user sets the member of goes in place for the others

    The instances whose user sets the edited member keep the user's
    value, as a fresh load gives it; the others take the edit where they
    stand.

    Background:
      Given the file "Pill.qml":
        """qml
        import QtQuick
        Rectangle {
          width: 10; height: 10
          function size() { return "small" }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Pill { objectName: "wide"; width: 30; function size() { return "own" } }
          Pill { objectName: "plain" }
          Rectangle {
            objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          property string plainSize: ""
          property string wideSize: ""
          function go() { slide.start() }
          function ask() { plainSize = children[1].size(); wideSize = children[0].size() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass

    Scenario: A width a user sets stays the user's and the others take the new one
      When the file "Pill.qml" is changed to:
        """qml
        import QtQuick
        Rectangle {
          width: 20; height: 10
          function size() { return "small" }
        }
        """
      And a reload happens
      Then the property "width" of "plain" is 20
      And the property "width" of "wide" is 30
      And the property "x" of "box" is at least 40

    Scenario: A function a user has its own of stays the user's
      When the file "Pill.qml" is changed to:
        """qml
        import QtQuick
        Rectangle {
          width: 10; height: 10
          function size() { return "large" }
        }
        """
      And a reload happens
      When the method "ask" of "root" is called
      Then the property "plainSize" of "root" is "large"
      And the property "wideSize" of "root" is "own"
      And the property "x" of "box" is at least 40

  Rule: A member no longer given goes back to what a new object of its type has, where it stands

    What the component the object is made from gives it, or else what a
    new native object has. Two siblings of one type that trade places are
    the same objects, each given what is now written at its place.

    Background:
      Given the file "Pill.qml":
        """qml
        import QtQuick
        Rectangle { width: 10; height: 10; color: "green" }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root; objectName: "root"
          Pill { objectName: "pill"; color: "red"; width: 40 }
          Text {
            objectName: "label"; text: "a"; font.pixelSize: 30; font.bold: true
            Layout.preferredWidth: 30; property real pw: Layout.preferredWidth
          }
          Rectangle { objectName: "first"; color: "red"; border.color: "blue" }
          Rectangle { objectName: "second"; radius: 4 }
          Rectangle {
            objectName: "box"; width: 5 + 5; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass

    Scenario: A member no longer given to a component's object takes the component's value
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root; objectName: "root"
          Pill { objectName: "pill" }
          Text {
            objectName: "label"; text: "a"; font.pixelSize: 30; font.bold: true
            Layout.preferredWidth: 30; property real pw: Layout.preferredWidth
          }
          Rectangle { objectName: "first"; color: "red"; border.color: "blue" }
          Rectangle { objectName: "second"; radius: 4 }
          Rectangle {
            objectName: "box"; width: 5 + 5; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "color" of "pill" is "#008000"
      And the property "width" of "pill" is 10
      And the property "x" of "box" is at least 40

    Scenario: A binding no longer given to a native object leaves its default
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root; objectName: "root"
          Pill { objectName: "pill"; color: "red"; width: 40 }
          Text {
            objectName: "label"; text: "a"; font.pixelSize: 30; font.bold: true
            Layout.preferredWidth: 30; property real pw: Layout.preferredWidth
          }
          Rectangle { objectName: "first"; color: "red"; border.color: "blue" }
          Rectangle { objectName: "second"; radius: 4 }
          Rectangle {
            objectName: "box"; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "width" of "box" is 0
      And the property "x" of "box" is at least 40

    Scenario: A member of a value no longer given goes back and the rest of the value stays
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root; objectName: "root"
          Pill { objectName: "pill"; color: "red"; width: 40 }
          Text {
            objectName: "label"; text: "a"; font.pixelSize: 30
            Layout.preferredWidth: 30; property real pw: Layout.preferredWidth
          }
          Rectangle { objectName: "first"; color: "red"; border.color: "blue" }
          Rectangle { objectName: "second"; radius: 4 }
          Rectangle {
            objectName: "box"; width: 5 + 5; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "font.bold" of "label" is false
      And the property "font.pixelSize" of "label" is 30
      And the property "x" of "box" is at least 40

    Scenario: A member of an attached object no longer given goes back
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root; objectName: "root"
          Pill { objectName: "pill"; color: "red"; width: 40 }
          Text {
            objectName: "label"; text: "a"; font.pixelSize: 30; font.bold: true
            property real pw: Layout.preferredWidth
          }
          Rectangle { objectName: "first"; color: "red"; border.color: "blue" }
          Rectangle { objectName: "second"; radius: 4 }
          Rectangle {
            objectName: "box"; width: 5 + 5; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "pw" of "label" is -1
      And the property "x" of "box" is at least 40

    Scenario: A member of a group no longer given goes back
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root; objectName: "root"
          Pill { objectName: "pill"; color: "red"; width: 40 }
          Text {
            objectName: "label"; text: "a"; font.pixelSize: 30; font.bold: true
            Layout.preferredWidth: 30; property real pw: Layout.preferredWidth
          }
          Rectangle { objectName: "first"; color: "red" }
          Rectangle { objectName: "second"; radius: 4 }
          Rectangle {
            objectName: "box"; width: 5 + 5; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "border.color" of "first" is "#000000"
      And the property "color" of "first" is "#ff0000"
      And the property "x" of "box" is at least 40

    Scenario: Two siblings of one type in each other's places each have what is now written
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root; objectName: "root"
          Pill { objectName: "pill"; color: "red"; width: 40 }
          Text {
            objectName: "label"; text: "a"; font.pixelSize: 30; font.bold: true
            Layout.preferredWidth: 30; property real pw: Layout.preferredWidth
          }
          Rectangle { objectName: "second"; radius: 4 }
          Rectangle { objectName: "first"; color: "red"; border.color: "blue" }
          Rectangle {
            objectName: "box"; width: 5 + 5; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "radius" of "second" is 4
      And the property "color" of "second" is "#ffffff"
      And the property "border.color" of "second" is "#000000"
      And the property "radius" of "first" is 0
      And the property "color" of "first" is "#ff0000"
      And the property "x" of "box" is at least 40

    Scenario: An attached member no longer given follows what its type gives it again
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          objectName: "root"
          Button { objectName: "b"; text: "add"; Accessible.name: "plus" }
        }
        """
      And a reload happens
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          objectName: "root"
          Button { objectName: "b"; text: "add" }
        }
        """
      And a reload happens
      Then the property "Accessible.name" of "b" is "add"
      When the property "text" of "b" is set to "sum"
      Then the property "Accessible.name" of "b" is "sum"

    Scenario: A side margin no longer given follows the margins again
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Item { objectName: "a"; anchors.margins: 2; anchors.bottomMargin: 6 }
        }
        """
      And a reload happens
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Item { objectName: "a"; anchors.margins: 2 }
        }
        """
      And a reload happens
      Then the property "anchors.bottomMargin" of "a" is 2
      When the property "anchors.margins" of "a" is set to 5
      Then the property "anchors.bottomMargin" of "a" is 5

  Rule: An element added to a model, removed from it or moved in it is the model's own insert, remove or move

    A view keeps the delegates of the rows that stay, as when a script
    changes the model; an element given other values changes its row.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; property string rows: ""; property string shown: ""
          ListModel { id: fruit
            ListElement { name: "a"; n: 1 }
            ListElement { name: "b"; n: 2 }
            ListElement { name: "c"; n: 3 }
          }
          Repeater { id: rep; model: fruit; Item { objectName: "row-" + name; property int taps: 0 } }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
          function look() {
            var r = [], s = []
            for (var i = 0; i < fruit.count; i++) r.push(fruit.get(i).name + fruit.get(i).n)
            for (var j = 0; j < rep.count; j++) s.push(rep.itemAt(j).objectName)
            rows = r.join(","); shown = s.join(",")
          }
        }
        """
      When the property "taps" of "row-b" is set to 4
      And the property "taps" of "row-c" is set to 7
      And the method "go" of "root" is called
      And 500 milliseconds pass

    Scenario: An element removed takes its row and its delegate, and the others stay
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; property string rows: ""; property string shown: ""
          ListModel { id: fruit
            ListElement { name: "b"; n: 2 }
            ListElement { name: "c"; n: 3 }
          }
          Repeater { id: rep; model: fruit; Item { objectName: "row-" + name; property int taps: 0 } }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
          function look() {
            var r = [], s = []
            for (var i = 0; i < fruit.count; i++) r.push(fruit.get(i).name + fruit.get(i).n)
            for (var j = 0; j < rep.count; j++) s.push(rep.itemAt(j).objectName)
            rows = r.join(","); shown = s.join(",")
          }
        }
        """
      And a reload happens
      And the method "look" of "root" is called
      Then the property "rows" of "root" is "b2,c3"
      And the property "shown" of "root" is "row-b,row-c"
      And the property "taps" of "row-b" is 4
      And the object "row-a" does not exist
      And the property "x" of "box" is at least 40

    Scenario: An element added between two is a row inserted there
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; property string rows: ""; property string shown: ""
          ListModel { id: fruit
            ListElement { name: "a"; n: 1 }
            ListElement { name: "z"; n: 9 }
            ListElement { name: "b"; n: 2 }
            ListElement { name: "c"; n: 3 }
          }
          Repeater { id: rep; model: fruit; Item { objectName: "row-" + name; property int taps: 0 } }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
          function look() {
            var r = [], s = []
            for (var i = 0; i < fruit.count; i++) r.push(fruit.get(i).name + fruit.get(i).n)
            for (var j = 0; j < rep.count; j++) s.push(rep.itemAt(j).objectName)
            rows = r.join(","); shown = s.join(",")
          }
        }
        """
      And a reload happens
      And the method "look" of "root" is called
      Then the property "rows" of "root" is "a1,z9,b2,c3"
      And the property "shown" of "root" is "row-a,row-z,row-b,row-c"
      And the property "taps" of "row-b" is 4
      And the property "taps" of "row-z" is 0
      And the property "x" of "box" is at least 40

    Scenario: Elements moved are rows moved, each with its delegate
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; property string rows: ""; property string shown: ""
          ListModel { id: fruit
            ListElement { name: "a"; n: 1 }
            ListElement { name: "c"; n: 3 }
            ListElement { name: "b"; n: 2 }
          }
          Repeater { id: rep; model: fruit; Item { objectName: "row-" + name; property int taps: 0 } }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
          function look() {
            var r = [], s = []
            for (var i = 0; i < fruit.count; i++) r.push(fruit.get(i).name + fruit.get(i).n)
            for (var j = 0; j < rep.count; j++) s.push(rep.itemAt(j).objectName)
            rows = r.join(","); shown = s.join(",")
          }
        }
        """
      And a reload happens
      And the method "look" of "root" is called
      Then the property "rows" of "root" is "a1,c3,b2"
      And the property "shown" of "root" is "row-a,row-c,row-b"
      And the property "taps" of "row-b" is 4
      And the property "taps" of "row-c" is 7
      And the property "x" of "box" is at least 40

    Scenario: An element given another value changes its row, which keeps its delegate
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; property string rows: ""; property string shown: ""
          ListModel { id: fruit
            ListElement { name: "a"; n: 1 }
            ListElement { name: "b"; n: 5 }
            ListElement { name: "c"; n: 3 }
          }
          Repeater { id: rep; model: fruit; Item { objectName: "row-" + name; property int taps: 0 } }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
          function look() {
            var r = [], s = []
            for (var i = 0; i < fruit.count; i++) r.push(fruit.get(i).name + fruit.get(i).n)
            for (var j = 0; j < rep.count; j++) s.push(rep.itemAt(j).objectName)
            rows = r.join(","); shown = s.join(",")
          }
        }
        """
      And a reload happens
      And the method "look" of "root" is called
      Then the property "rows" of "root" is "a1,b5,c3"
      And the property "taps" of "row-b" is 4
      And the property "x" of "box" is at least 40

    Scenario: An element removed takes the model it holds with it
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; property var held: null
          ListModel { id: fruit
            ListElement { name: "a"; parts: [ ListElement { p: 1 }, ListElement { p: 2 } ] }
            ListElement { name: "b"; parts: [ ListElement { p: 3 } ] }
          }
          function hold() { held = fruit.get(0).parts }
        }
        """
      And a reload happens
      And the method "hold" of "root" is called
      Then the property "held" of "root" is not null
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; property var held: null
          ListModel { id: fruit
            ListElement { name: "b"; parts: [ ListElement { p: 3 } ] }
          }
          function hold() { held = fruit.get(0).parts }
        }
        """
      And a reload happens
      Then the property "held" of "root" is null

  Rule: A property renamed goes in place when no other file sets either name

    The property is given what the file now gives it, as a new object
    would be. What another file binds to either name is bound again: a
    read of the old name finds nothing, as on a fresh load. A user that
    declares members of its own has them still. When another file sets
    the name, or a script file reads it, the roots are made again.

    Background:
      Given the file "Pill.qml":
        """qml
        import QtQuick
        Rectangle { width: 10; height: 10; property real seen: parent ? parent.shade : 0 }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int count: 3
          property real shade: 1
          property string shown: "n" + count
          property int heard: 0
          onCountChanged: heard++
          Pill { objectName: "pill" }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
          function bump() { count++ }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass

    Scenario: A property renamed with its uses in the file goes in place
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int total: 3
          property real shade: 1
          property string shown: "n" + total
          property int heard: 0
          onTotalChanged: heard++
          Pill { objectName: "pill" }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
          function bump() { total++ }
        }
        """
      And a reload happens
      Then the property "total" of "root" is 3
      And the property "shown" of "root" is "n3"
      And the property "x" of "box" is at least 40
      When the method "bump" of "root" is called
      Then the property "shown" of "root" is "n4"
      And the property "heard" of "root" is 1

    Scenario: A property another file reads is renamed in place, and its reads are bound again
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int count: 3
          property real tint: 1
          property string shown: "n" + count
          property int heard: 0
          onCountChanged: heard++
          Pill { objectName: "pill" }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
          function bump() { count++ }
        }
        """
      And a reload happens
      Then the property "tint" of "root" is 1
      And the property "seen" of "pill" is 0
      And the property "x" of "box" is at least 40
      When the file "Pill.qml" is changed to:
        """qml
        import QtQuick
        Rectangle { width: 10; height: 10; property real seen: parent ? parent.tint : 0 }
        """
      And a reload happens
      Then the property "seen" of "pill" is 1
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int count: 3
          property real shade: 1
          property string shown: "n" + count
          property int heard: 0
          onCountChanged: heard++
          Pill { objectName: "pill" }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
          function bump() { count++ }
        }
        """
      And a reload happens
      Then the property "seen" of "pill" is 0
      And the property "x" of "box" is at least 40

    Scenario: A property of a component file renamed goes in place for its users
      When the file "Pill.qml" is changed to:
        """qml
        import QtQuick
        Rectangle { width: 10; height: 10; property real looked: parent ? parent.shade : 0 }
        """
      And a reload happens
      Then the property "looked" of "pill" is 1
      And the property "x" of "box" is at least 40

    Scenario: An alias of a component file renamed goes in place for its users
      When the file "Pill.qml" is changed to:
        """qml
        import QtQuick
        Rectangle { width: 10; height: 10; property alias label: t.text
          Text { id: t; text: "a" }
          Text { objectName: "echo"; text: label + "!" }
        }
        """
      And a reload happens
      And the method "go" of "root" is called
      And 500 milliseconds pass
      And the file "Pill.qml" is changed to:
        """qml
        import QtQuick
        Rectangle { width: 10; height: 10; property alias caption: t.text
          Text { id: t; text: "a" }
          Text { objectName: "echo"; text: caption + "!" }
        }
        """
      And a reload happens
      Then the property "caption" of "pill" is "a"
      And the property "text" of "echo" is "a!"
      And the property "x" of "box" is at least 40
      When the property "caption" of "pill" is set to "b"
      Then the property "text" of "echo" is "b!"

    Scenario: A property renamed under members a user declares goes in place
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int count: 3
          property real shade: 1
          property string shown: "n" + count
          property int heard: 0
          onCountChanged: heard++
          Pill { objectName: "pill"; property int mine: 2 }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
          function bump() { count++ }
        }
        """
      And a reload happens
      And the method "go" of "root" is called
      And 500 milliseconds pass
      And the file "Pill.qml" is changed to:
        """qml
        import QtQuick
        Rectangle { width: 10; height: 10; property real looked: parent ? parent.shade : 0 }
        """
      And a reload happens
      Then the property "looked" of "pill" is 1
      And the property "mine" of "pill" is 2
      And the property "x" of "box" is at least 40
      When the property "mine" of "pill" is set to 5
      Then the property "mine" of "pill" is 5

  Rule: An edit to a singleton's file goes in place

    The singleton is the object it was, with the edit applied, as any
    other object a reload keeps is; what the files around it read of it
    is what it now gives.

    Background:
      Given the file "qmldir":
        """
        singleton Look Look.qml
        """
      And the file "Look.qml":
        """qml
        pragma Singleton
        import QtQuick
        QtObject {
          property string tone: "red"
          property int bumps: 0
          function bump() { bumps++ }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string shade: Look.tone
          property int heard: Look.bumps
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start(); Look.bump() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass

    Scenario: A value of a singleton edited is read where it is used, and the singleton goes on
      When the file "Look.qml" is changed to:
        """qml
        pragma Singleton
        import QtQuick
        QtObject {
          property string tone: "blue"
          property int bumps: 0
          function bump() { bumps++ }
        }
        """
      And a reload happens
      Then the property "shade" of "root" is "blue"
      And the property "heard" of "root" is 1
      And the property "x" of "box" is at least 40
      When the file "Look.qml" is changed to:
        """qml
        pragma Singleton
        import QtQuick
        QtObject {
          property string tone: "green"
          property int bumps: 0
          function bump() { bumps += 2 }
        }
        """
      And a reload happens
      And the method "go" of "root" is called
      Then the property "shade" of "root" is "green"
      And the property "heard" of "root" is 3

  Rule: An object given to a member of its own that is removed goes in place

    The member goes back to what a fresh object of its type has, as when
    a value no longer given does; the object goes with the others removed.

    Scenario: A transition a positioner is given removed leaves it with none
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Column {
            objectName: "col"
            move: Transition { NumberAnimation { properties: "y"; duration: 100 } }
            Rectangle { width: 10; height: 10 }
          }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass
      Then the property "move" of "col" is not null
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Column {
            objectName: "col"
            Rectangle { width: 10; height: 10 }
          }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "move" of "col" is null
      And the property "x" of "box" is at least 40

  Rule: An alias of an object moved among its siblings goes in place

    An object moved among its siblings is made again; an alias of it, or
    of a member of it, stands for the new one, as in a fresh load, and
    what reads through the alias reads that. Where a state or a Binding
    writes through that object's alias, or a user gives an object through
    it, the roots are made again; an alias of the same name on another
    object is not it.

    Scenario: A user reads through aliases of an object moved before its sibling
      Given the file "Viewer.qml":
        """qml
        import QtQuick
        Item {
          property alias current: shown
          property alias words: shown.text
          Text { id: shown; objectName: "first"; text: "a" }
          Item { }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Viewer { id: viewer }
          property string seen: viewer.current.objectName + "/" + viewer.words
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass
      Then the property "seen" of "root" is "first/a"
      When the file "Viewer.qml" is changed to:
        """qml
        import QtQuick
        Item {
          property alias current: shown
          property alias words: shown.text
          Item { }
          Text { id: shown; objectName: "second"; text: "b" }
        }
        """
      And a reload happens
      Then the property "seen" of "root" is "second/b"
      And the property "x" of "box" is at least 40

    Scenario: What a user gives through an alias goes to the object made again
      Given the file "Viewer.qml":
        """qml
        import QtQuick
        Item {
          property alias current: shown
          property alias words: shown.text
          Text { id: shown; objectName: "first"; text: "a" }
          Item { }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Viewer { id: viewer; words: "mine" }
          property string seen: viewer.current.objectName + "/" + viewer.current.text
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass
      Then the property "seen" of "root" is "first/mine"
      When the file "Viewer.qml" is changed to:
        """qml
        import QtQuick
        Item {
          property alias current: shown
          property alias words: shown.text
          Item { }
          Text { id: shown; objectName: "second"; text: "b" }
        }
        """
      And a reload happens
      Then the property "seen" of "root" is "second/mine"
      And the property "x" of "box" is at least 40

    Scenario: A state that holds the object made again through an alias holds the new one
      Given the file "Viewer.qml":
        """qml
        import QtQuick
        Item {
          property alias current: shown
          Text { id: shown; objectName: "first"; text: "a" }
          Item { }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Viewer { id: viewer }
          states: State {
            name: "faded"; when: true
            PropertyChanges { target: viewer; current.opacity: 0.5 }
          }
        }
        """
      Then the property "opacity" of "first" is 0.5
      When the file "Viewer.qml" is changed to:
        """qml
        import QtQuick
        Item {
          property alias current: shown
          Item { }
          Text { id: shown; objectName: "second"; text: "a" }
        }
        """
      And a reload happens
      Then the property "opacity" of "second" is 0.5

    Scenario: An object a user gives through an alias goes to the object made again
      Given the file "Viewer.qml":
        """qml
        import QtQuick
        Item {
          property alias current: shown
          Text { id: shown; objectName: "first"; text: "a" }
          Item { }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Viewer {
            id: viewer
            current.states: State {
              name: "wide"; when: true
              PropertyChanges { target: viewer.current; width: 30 }
            }
          }
        }
        """
      Then the property "width" of "first" is 30
      When the file "Viewer.qml" is changed to:
        """qml
        import QtQuick
        Item {
          property alias current: shown
          Item { }
          Text { id: shown; objectName: "second"; text: "a" }
        }
        """
      And a reload happens
      Then the property "width" of "second" is 30

    Scenario: A state that writes through another object's alias of the same name goes in place
      Given the file "Viewer.qml":
        """qml
        import QtQuick
        Item {
          property alias current: shown
          Text { id: shown; objectName: "first"; text: "a" }
          Item { }
        }
        """
      And the file "Other.qml":
        """qml
        import QtQuick
        Item {
          property alias current: label
          Text { id: label; objectName: "label"; text: "b" }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Viewer { id: viewer }
          Other { id: other }
          states: State {
            name: "faded"; when: true
            PropertyChanges { target: other; current.opacity: 0.5 }
          }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass
      Then the property "opacity" of "label" is 0.5
      When the file "Viewer.qml" is changed to:
        """qml
        import QtQuick
        Item {
          property alias current: shown
          Item { }
          Text { id: shown; objectName: "second"; text: "a" }
        }
        """
      And a reload happens
      Then the property "opacity" of "label" is 0.5
      And the property "opacity" of "second" is 1
      And the property "x" of "box" is at least 40

  Rule: A value a state's PropertyChanges gives, edited, goes in place

    The PropertyChanges takes the value as now written. A state in force
    puts it on its target at once, as a fresh load enters it; one that is
    not gives it when it is entered.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property bool wide: true
          Text { id: label; objectName: "label"; text: "a"; width: 10; font.pixelSize: 12 }
          states: [
            State {
              name: "wide"; when: root.wide
              PropertyChanges { target: label; width: 30; font.pixelSize: 20 }
            },
            State {
              name: "tall"; when: !root.wide
              PropertyChanges { label.width: 5 }
            }
          ]
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass
      Then the property "width" of "label" is 30

    Scenario: A value of the state in force edited is on its target at once
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property bool wide: true
          Text { id: label; objectName: "label"; text: "a"; width: 10; font.pixelSize: 12 }
          states: [
            State {
              name: "wide"; when: root.wide
              PropertyChanges { target: label; width: 40; font.pixelSize: 22 }
            },
            State {
              name: "tall"; when: !root.wide
              PropertyChanges { label.width: 5 }
            }
          ]
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "width" of "label" is 40
      And the property "font.pixelSize" of "label" is 22
      And the property "x" of "box" is at least 40
      When the property "wide" of "root" is set to false
      Then the property "width" of "label" is 5
      When the property "wide" of "root" is set to true
      Then the property "width" of "label" is 40

    Scenario: A value of a state not in force edited is given when the state is entered
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property bool wide: true
          Text { id: label; objectName: "label"; text: "a"; width: 10; font.pixelSize: 12 }
          states: [
            State {
              name: "wide"; when: root.wide
              PropertyChanges { target: label; width: 30; font.pixelSize: 20 }
            },
            State {
              name: "tall"; when: !root.wide
              PropertyChanges { label.width: 8 * 2 }
            }
          ]
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the property "width" of "label" is 30
      And the property "x" of "box" is at least 40
      When the property "wide" of "root" is set to false
      Then the property "width" of "label" is 16

  Rule: A value a state gives, edited, reaches every object made from its file

    Each object made from the file keeps its own state and is given the
    value as now written, as a fresh load makes each of them.

    Scenario: A value edited in a state is given to each object made from the file
      Given the file "Holder.qml":
        """qml
        import QtQuick
        Item { property int n; Badge { n: parent.n } }
        """
      And the file "Badge.qml":
        """qml
        import QtQuick
        Item {
          id: badge
          property int n
          width: inner.size
          objectName: "badge" + n
          QtObject { id: inner; property int size: 1 }
          states: State {
            name: "big"; when: true
            PropertyChanges { target: inner; size: 10 }
          }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Repeater { model: 3; Holder { n: index } }
        }
        """
      Then the property "width" of "badge2" is 10
      When the file "Badge.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: badge
          property int n
          width: inner.size
          objectName: "badge" + n
          QtObject { id: inner; property int size: 1 }
          states: State {
            name: "big"; when: true
            PropertyChanges { target: inner; size: 20 }
          }
        }
        """
      And a reload happens
      Then the property "width" of "badge0" is 20
      And the property "width" of "badge1" is 20
      And the property "width" of "badge2" is 20

  Rule: A member edited that its object does not have goes in place

    A member given through a name the object does not have (an alias
    since renamed, say) is told of again, as a fresh load tells of it,
    and the rest goes on.

    Scenario: A member through an alias that is not there edited is told of again
      Given the file "Viewer.qml":
        """qml
        import QtQuick
        Item {
          property alias shownNow: shown
          Text { id: shown; text: "a" }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Viewer { current.text: "b" }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      When the method "go" of "root" is called
      And 500 milliseconds pass
      Then the log has 1 warnings matching "non-existent property .current.text"
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          Viewer { current.text: "c" }
          Rectangle { objectName: "box"; width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 1000 }
          }
          function go() { slide.start() }
        }
        """
      And a reload happens
      Then the log has 2 warnings matching "non-existent property .current.text"
      And the property "x" of "box" is at least 40

  Rule: A window's colours after a reload are the ones a fresh load gives

    The style's palette is the application's while it makes an object of
    the style's module. A reload that removes the last such object, or
    makes the first, gives every colour group of the window what a fresh
    load of the new files gives it.

    Scenario: A window whose last Fusion control is removed has Basic's colours again
      Given the application starts with the controls configuration:
        """
        [Controls]
        Style=Fusion
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Window {
          objectName: "root"
          Text { text: "first" }
          Button { text: "close" }
        }
        """
      Then the property "palette.button" of "root" is "#efefef"
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Window {
          objectName: "root"
          Text { text: "first" }
        }
        """
      And a reload happens
      Then the property "palette.button" of "root" is "#e0e0e0"
      And the property "palette.active.button" of "root" is "#e0e0e0"
      And the property "palette.inactive.button" of "root" is "#e0e0e0"
      And the property "palette.disabled.button" of "root" is "#e0e0e0"
      And the property "palette.active.highlight" of "root" is "#0066ff"

    Scenario: A window given its first Fusion control has Fusion's colours
      Given the application starts with the controls configuration:
        """
        [Controls]
        Style=Fusion
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Window {
          objectName: "root"
          Text { text: "first" }
        }
        """
      Then the property "palette.button" of "root" is "#e0e0e0"
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Window {
          objectName: "root"
          Text { text: "first" }
          Button { text: "close" }
        }
        """
      And a reload happens
      Then the property "palette.button" of "root" is "#efefef"
      And the property "palette.active.button" of "root" is "#efefef"
      And the property "palette.inactive.button" of "root" is "#efefef"
      And the property "palette.disabled.button" of "root" is "#efefef"
      And the property "palette.active.highlight" of "root" is "#308cc6"

    Scenario: A window made from another file keeps that file's style when a control in it goes
      Given the application starts with the controls configuration:
        """
        [Controls]
        Style=Fusion
        """
      And the file "Shell.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        ApplicationWindow {
          objectName: "root"
          Button { text: "close" }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Shell {}
        """
      Then the property "palette.button" of "root" is "#efefef"
      When the file "Shell.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        ApplicationWindow {
          objectName: "root"
        }
        """
      And a reload happens
      Then the property "palette.button" of "root" is "#efefef"
      And the property "palette.active.highlight" of "root" is "#308cc6"

  Rule: A delegate written inline that a reload removes takes its Component with it

    A fresh load of the new file never makes the `Component` the view's
    delegate was; one held elsewhere is null after the reload.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          ListModel { id: things; ListElement { name: "a" } ListElement { name: "b" } }
          ListView {
            id: first; objectName: "first"; width: 50; height: 100; model: things
            delegate: Rectangle { objectName: "one" + name; width: 10; height: 10 }
          }
          ListView {
            objectName: "second"; width: 50; height: 100; model: things
            delegate: Rectangle {
              objectName: "line" + name; width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            }
          }
          property QtObject held
          function grab() { held = first.delegate }
        }
        """
      When 500 milliseconds pass

    Scenario: A view's delegate removed takes its Component with it
      When the method "grab" of "root" is called
      Then the property "held" of "root" is not null
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          ListModel { id: things; ListElement { name: "a" } ListElement { name: "b" } }
          ListView { id: first; objectName: "first"; width: 50; height: 100; model: things }
          ListView {
            objectName: "second"; width: 50; height: 100; model: things
            delegate: Rectangle {
              objectName: "line" + name; width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 1000 }
            }
          }
          property QtObject held
          function grab() { held = first.delegate }
        }
        """
      And a reload happens
      Then the property "held" of "root" is null
      And the object "onea" does not exist
      And the property "x" of "linea" is at least 40

  Rule: A read-only value a reload changes is the new one to what reads it

    Scenario: A binding that reads a read-only value has the new one
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          readonly property int r: 3
          property int twice: r * 2
        }
        """
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          readonly property int r: 4
          property int twice: r * 2
        }
        """
      And a reload happens
      Then the property "r" of "root" is 4
      And the property "twice" of "root" is 8
