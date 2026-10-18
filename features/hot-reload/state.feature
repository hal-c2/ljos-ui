# Sources:
#   docs/ARCHITECTURE.md (section 12.1)
#   docs/research/03-runtime-concurrency-hot-reload.md
# Hot reload is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: What survives a reload
  An object that is the same object after a reload keeps what the running
  application gave it: values it assigned, the rows of a model, what a Loader
  shows, a running timer, the focus, a checked control. What the file declares
  comes from the new file.

  Rule: A value the application assigned survives

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int n: 1
          property int count: 0
          width: n * 10
          function more() { count += 1 }
        }
        """

    Scenario: A value a function assigned is kept
      When the method "more" of "root" is called
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int n: 1
          property int count: 0
          width: n * 10
          height: 5
          function more() { count += 1 }
        }
        """
      And a reload happens
      Then the property "count" of "root" is 1
      And the property "height" of "root" is 5

    Scenario: A binding the application replaced by a value stays replaced
      When the property "width" of "root" is set to 99
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int n: 1
          property int count: 0
          width: n * 10
          height: 5
          function more() { count += 1 }
        }
        """
      And a reload happens
      And the property "n" of "root" is set to 3
      Then the property "width" of "root" is 99

    Scenario: A declaration that was edited wins over an assigned value
      When the property "count" of "root" is set to 7
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int n: 1
          property int count: 40
          width: n * 10
          function more() { count += 1 }
        }
        """
      And a reload happens
      Then the property "count" of "root" is 40

    Scenario: A value the file declares and nobody assigned follows the file
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int n: 4
          property int count: 0
          width: n * 10
          function more() { count += 1 }
        }
        """
      And a reload happens
      Then the property "width" of "root" is 40

  Rule: An object the application stored is the same object after the reload

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property Item chosen
          property var held
          readonly property string chosenName: chosen ? chosen.objectName : "none"
          readonly property string heldName: held ? held.objectName : "none"
          function choose() { chosen = two; held = two }
          function tap() { held.taps += 1 }
          Item { id: one; objectName: "one" }
          Item { id: two; objectName: "two"; property int taps: 0 }
        }
        """

    Scenario: A stored object is the new object at its place
      When the method "choose" of "root" is called
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          property Item chosen
          property var held
          readonly property string chosenName: chosen ? chosen.objectName : "none"
          readonly property string heldName: held ? held.objectName : "none"
          function choose() { chosen = two; held = two }
          function tap() { held.taps += 1 }
          Item { id: one; objectName: "one" }
          Item { id: two; objectName: "two"; property int taps: 0 }
        }
        """
      And a reload happens
      And the method "tap" of "root" is called
      Then the property "chosenName" of "root" is "two"
      And the property "heldName" of "root" is "two"
      And the property "taps" of "two" is 1
      And the log has no warnings

    Scenario: An object gone from the file is no longer stored
      When the method "choose" of "root" is called
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property Item chosen
          property var held
          readonly property string chosenName: chosen ? chosen.objectName : "none"
          readonly property string heldName: held ? held.objectName : "none"
          function choose() { chosen = one; held = one }
          Item { id: one; objectName: "one" }
        }
        """
      And a reload happens
      Then the property "chosenName" of "root" is "none"
      And the property "heldName" of "root" is "none"

  Rule: Objects a script made under an object that goes on are made again under it

    Background:
      Given the file "Tile.qml":
        """qml
        import QtQuick
        Rectangle {
          property int kind: 0
          property int taps: 0
          width: 10
        }
        """
      And the file "board.js":
        """
        .pragma library
        var tiles = []
        var tile = null
        function fill(parent) {
          if (!tile)
            tile = Qt.createComponent("Tile.qml")
          for (let i = 0; i < 3; i++)
            tiles.push(tile.createObject(parent, { kind: i }))
        }
        function tap(i) { tiles[i].taps += 1 }
        function kinds() { return tiles.map(t => t.kind + ":" + t.taps).join(",") }
        """
      And the QML:
        """qml
        import QtQuick
        import "board.js" as Board
        Item {
          objectName: "root"
          property string seen
          function fill() { Board.fill(area) }
          function tap(i) { Board.tap(i) }
          function look() {
            let widths = []
            for (let i = 0; i < area.children.length; i++)
              widths.push(area.children[i].width)
            seen = Board.kinds() + "|" + widths.join(",")
          }
          Item { id: area; objectName: "area" }
        }
        """

    Scenario: They keep their order and what they held, and a script that holds them reaches them
      When the method "fill" of "root" is called
      And the method "tap" of "root" is called with 1
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import "board.js" as Board
        Item {
          objectName: "root"
          width: 5
          property string seen
          function fill() { Board.fill(area) }
          function tap(i) { Board.tap(i) }
          function look() {
            let widths = []
            for (let i = 0; i < area.children.length; i++)
              widths.push(area.children[i].width)
            seen = Board.kinds() + "|" + widths.join(",")
          }
          Item { id: area; objectName: "area" }
        }
        """
      And a reload happens
      And the method "tap" of "root" is called with 2
      And the method "look" of "root" is called
      Then the property "seen" of "root" is "0:0,1:1,2:1|10,10,10"
      And the log has no warnings

    Scenario: They are made from their file as it is now
      When the method "fill" of "root" is called
      And the method "tap" of "root" is called with 1
      And the file "Tile.qml" is changed to:
        """qml
        import QtQuick
        Rectangle {
          property int kind: 0
          property int taps: 0
          width: 20
        }
        """
      And a reload happens
      And the method "look" of "root" is called
      Then the property "seen" of "root" is "0:0,1:1,2:0|20,20,20"
      And the log has no warnings

    Scenario: The component a script holds makes objects from the file as it is now
      When the method "fill" of "root" is called
      And the file "Tile.qml" is changed to:
        """qml
        import QtQuick
        Rectangle {
          property int kind: 0
          property int taps: 0
          width: 30
        }
        """
      And a reload happens
      And the method "fill" of "root" is called
      And the method "look" of "root" is called
      Then the property "seen" of "root" is "0:0,1:0,2:0,0:0,1:0,2:0|30,30,30,30,30,30"

  Rule: The rows of a ListModel survive

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int rows: names.count
          property string last: ""
          ListModel {
            id: names
            ListElement { name: "Anna" }
          }
          function add(n) { names.append({ name: n }) }
          function look() { last = names.get(names.count - 1).name }
        }
        """
      When the method "add" of "root" is called with "Bjarni"

    Scenario: Rows the application appended are still there
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          property int rows: names.count
          property string last: ""
          ListModel {
            id: names
            ListElement { name: "Anna" }
          }
          function add(n) { names.append({ name: n }) }
          function look() { last = names.get(names.count - 1).name }
        }
        """
      And a reload happens
      And the method "look" of "root" is called
      Then the property "rows" of "root" is 2
      And the property "last" of "root" is "Bjarni"

    Scenario: Rows the file declares come from the file when it changes them
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int rows: names.count
          property string last: ""
          ListModel {
            id: names
            ListElement { name: "Anna" }
            ListElement { name: "Einar" }
            ListElement { name: "Dagur" }
          }
          function add(n) { names.append({ name: n }) }
          function look() { last = names.get(names.count - 1).name }
        }
        """
      And a reload happens
      And the method "look" of "root" is called
      Then the property "rows" of "root" is 3
      And the property "last" of "root" is "Dagur"

  Rule: A Loader keeps showing what the application told it to show

    Background:
      Given the file "First.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "first" }
        """
      And the file "Second.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "second"; width: 10 }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { id: loader; objectName: "loader"; source: "First.qml" }
          function show(url) { loader.setSource(url) }
        }
        """
      When the method "show" of "root" is called with "Second.qml"

    Scenario: The Loader's source survives a change to the file that holds the Loader
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          Loader { id: loader; objectName: "loader"; source: "First.qml" }
          function show(url) { loader.setSource(url) }
        }
        """
      And a reload happens
      Then the object "loader/second" exists
      And the object "loader/first" does not exist

    Scenario: The Loader shows the new version of the file it loaded
      When the file "Second.qml" is changed to:
        """qml
        import QtQuick
        Rectangle { objectName: "second"; width: 20 }
        """
      And a reload happens
      Then the property "width" of "loader/second" is 20

  Rule: A StackView keeps the pages the application pushed

    Background:
      Given the file "Page.qml":
        """qml
        import QtQuick
        Rectangle { property string tag: "page"; width: 10 }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root
          objectName: "root"
          width: 300; height: 200
          Component {
            id: page
            Rectangle { property string tag: "made"; property Item flow }
          }
          StackView {
            id: stack
            objectName: "stack"
            anchors.fill: parent
            initialItem: Rectangle { objectName: "first"; property string tag: "first" }
          }
          function open() { stack.push(page, { tag: "pushed", flow: root }) }
          function openFile() { stack.push("Page.qml") }
        }
        """

    Scenario: A page pushed with properties is still on top, holding the new objects
      When the method "open" of "root" is called
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root
          objectName: "root"
          width: 300; height: 200
          property int edited: 1
          Component {
            id: page
            Rectangle { property string tag: "made"; property Item flow }
          }
          StackView {
            id: stack
            objectName: "stack"
            anchors.fill: parent
            initialItem: Rectangle { objectName: "first"; property string tag: "first" }
          }
          function open() { stack.push(page, { tag: "pushed", flow: root }) }
          function openFile() { stack.push("Page.qml") }
        }
        """
      And a reload happens
      Then the property "depth" of "stack" is 2
      And the property "currentItem.tag" of "stack" is "pushed"
      And the property "currentItem.flow.edited" of "stack" is 1
      And the log has no warnings

    Scenario: Popping after the reload shows the first page again
      When the method "open" of "root" is called
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root
          objectName: "root"
          width: 300; height: 200
          property int edited: 1
          Component {
            id: page
            Rectangle { property string tag: "made"; property Item flow }
          }
          StackView {
            id: stack
            objectName: "stack"
            anchors.fill: parent
            initialItem: Rectangle { objectName: "first"; property string tag: "first" }
          }
          function open() { stack.push(page, { tag: "pushed", flow: root }) }
          function openFile() { stack.push("Page.qml") }
        }
        """
      And a reload happens
      And the method "pop" of "stack" is called
      Then the property "depth" of "stack" is 1
      And the property "currentItem.tag" of "stack" is "first"

    Scenario: A page pushed from a file shows the new version of that file
      When the method "openFile" of "root" is called
      And the file "Page.qml" is changed to:
        """qml
        import QtQuick
        Rectangle { property string tag: "page"; width: 20 }
        """
      And a reload happens
      Then the property "depth" of "stack" is 2
      And the property "currentItem.tag" of "stack" is "page"

  Rule: An open popup stays open

    Scenario: A popup the application opened is open after its file changes
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          objectName: "root"
          width: 300; height: 200
          Popup { id: p; objectName: "p"; width: 100; height: 50 }
        }
        """
      When the method "open" of "p" is called
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          objectName: "root"
          width: 300; height: 200
          Popup { id: p; objectName: "p"; width: 120; height: 50 }
        }
        """
      And a reload happens
      Then the property "opened" of "p" is true
      And the property "width" of "p" is 120

  Rule: A binding the application gave an object that outlives the reload follows the new objects

    Background:
      Given the file "qmldir":
        """
        singleton Look Look.qml
        """
      And the file "Look.qml":
        """qml
        pragma Singleton
        import QtQuick
        QtObject { property int size: 0 }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 100
          property int seen: Look.size
          Component.onCompleted: Look.size = Qt.binding(() => root.width * 2)
        }
        """

    Scenario: A singleton's binding to an object of the file reads the object at its place
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 100
          height: 5
          property int seen: Look.size
          Component.onCompleted: Look.size = Qt.binding(() => root.width * 2)
        }
        """
      And a reload happens
      And the property "width" of "root" is set to 30
      Then the property "seen" of "root" is 60
      And the log has no warnings

  Rule: A running Timer keeps running

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int ticks: 0
          Timer { id: timer; interval: 100; repeat: true; onTriggered: root.ticks += 1 }
          function go() { timer.start() }
          function halt() { timer.stop() }
        }
        """
      When the method "go" of "root" is called
      And 250 milliseconds pass

    Scenario: A started timer still triggers, with its new handler
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int ticks: 0
          Timer { id: timer; interval: 100; repeat: true; onTriggered: root.ticks += 10 }
          function go() { timer.start() }
          function halt() { timer.stop() }
        }
        """
      And a reload happens
      And 100 milliseconds pass
      Then the property "ticks" of "root" is 12

    Scenario: A stopped timer stays stopped
      When the method "halt" of "root" is called
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int ticks: 0
          Timer { id: timer; interval: 50; repeat: true; onTriggered: root.ticks += 10 }
          function go() { timer.start() }
          function halt() { timer.stop() }
        }
        """
      And a reload happens
      And 200 milliseconds pass
      Then the property "ticks" of "root" is 2

  Rule: A running animation a script started keeps running

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int ticks: 0
          SequentialAnimation {
            id: seq; objectName: "seq"; loops: Animation.Infinite
            ScriptAction { script: root.ticks += 1 }
            PauseAnimation { duration: 100 }
          }
          function go() { seq.start() }
        }
        """
      When the method "go" of "root" is called
      And 250 milliseconds pass

    Scenario: A started animation still runs after a reload that makes objects again
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int ticks: 0
          property int extra: 1
          SequentialAnimation {
            id: seq; objectName: "seq"; loops: Animation.Infinite
            ScriptAction { script: root.ticks += 10 }
            PauseAnimation { duration: 100 }
          }
          function go() { seq.start() }
        }
        """
      And a reload happens
      And 250 milliseconds pass
      Then the property "running" of "seq" is true
      And the property "ticks" of "root" is at least 13

  Rule: The item with the keyboard keeps it

    Scenario: Active focus stays on the same item
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Item { id: one; objectName: "one"; focus: true }
          Item { id: two; objectName: "two" }
        }
        """
      When the method "forceActiveFocus" of "two" is called
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          Item { id: one; objectName: "one"; focus: true }
          Item { id: two; objectName: "two" }
        }
        """
      And a reload happens
      Then the property "activeFocus" of "two" is true
      And the property "activeFocus" of "one" is false

  Rule: A checked control stays checked

    Scenario: A check box the user checked stays checked when its text changes
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          objectName: "root"
          width: 200; height: 100
          CheckBox { id: option; objectName: "option"; text: "Option" }
        }
        """
      When the user clicks the "checkbox" "Option"
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          objectName: "root"
          width: 200; height: 100
          CheckBox { id: option; objectName: "option"; text: "Choice" }
        }
        """
      And a reload happens
      Then the property "checked" of "option" is true
      And the property "text" of "option" is "Choice"

  Rule: A view stays scrolled where the user left it

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 200; height: 100
          ListView {
            id: view
            objectName: "view"
            anchors.fill: parent
            orientation: ListView.Horizontal
            model: 7
            delegate: Rectangle { required property int index; width: 160; height: 100 }
          }
        }
        """

    Scenario: A list scrolled by a method is still scrolled there
      When the method "positionViewAtIndex" of "view" is called with 3, 0
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          width: 200; height: 100
          ListView {
            id: view
            objectName: "view"
            anchors.fill: parent
            orientation: ListView.Horizontal
            model: 7
            delegate: Rectangle { required property int index; width: 160; height: 100; color: "red" }
          }
        }
        """
      And a reload happens
      Then the property "contentX" of "view" is 480

    Scenario: A list scrolled away from its current row stays where it was
      When the property "currentIndex" of "view" is set to 1
      And the method "positionViewAtIndex" of "view" is called with 4, 0
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          width: 200; height: 100
          ListView {
            id: view
            objectName: "view"
            anchors.fill: parent
            orientation: ListView.Horizontal
            model: 7
            delegate: Rectangle { required property int index; width: 160; height: 100; color: "red" }
          }
        }
        """
      And a reload happens
      Then the property "contentX" of "view" is 640
      And the property "currentIndex" of "view" is 1

  Rule: A view whose rows are as long as the view makes them stays where it was

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 640; height: 100
          ListView {
            id: view
            objectName: "view"
            anchors.fill: parent
            orientation: ListView.Horizontal
            cacheBuffer: 2000
            snapMode: ListView.SnapOneItem
            highlightRangeMode: ListView.ApplyRange
            model: 7
            delegate: Rectangle {
              height: 100
              width: ListView.view && ListView.view.width >= 200 ? ListView.view.width / Math.floor(ListView.view.width / 200.0) : 200
            }
          }
        }
        """

    Scenario: A list brought to its current row is still there
      When the method "positionViewAtIndex" of "view" is called with 3, 0
      And the property "currentIndex" of "view" is set to 3
      And 1000 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          width: 640; height: 100
          ListView {
            id: view
            objectName: "view"
            anchors.fill: parent
            orientation: ListView.Horizontal
            cacheBuffer: 2000
            snapMode: ListView.SnapOneItem
            highlightRangeMode: ListView.ApplyRange
            model: 7
            delegate: Rectangle {
              height: 100
              color: "red"
              width: ListView.view && ListView.view.width >= 200 ? ListView.view.width / Math.floor(ListView.view.width / 200.0) : 200
            }
          }
        }
        """
      And a reload happens
      And 100 milliseconds pass
      Then the property "contentX" of "view" is 640
      And the property "originX" of "view" is 0
      And the property "currentIndex" of "view" is 3

  Rule: What a script library holds survives a change to the library
    A library is made again from its new text, and each of its top-level
    variables takes the value the old library held, unless the value the
    file starts it with changed. The objects that use it are kept and do not
    run Component.onCompleted again, so they cannot set it up a second time.

    Background:
      Given the file "logic.js":
        """js
        .pragma library
        var game
        var step = 1
        function start() { game = { count: 5 } }
        function next() { game.count += step; return game.count }
        """
      And the QML:
        """qml
        import QtQuick
        import "logic.js" as Logic
        Item {
          objectName: "root"
          property int count: 0
          Component.onCompleted: Logic.start()
          function tick() { count = Logic.next() }
        }
        """

    Scenario: A variable the library set while running keeps its value
      When the method "tick" of "root" is called
      And the file "logic.js" is changed to:
        """js
        .pragma library
        var game
        var step = 1
        function start() { game = { count: 5 } }
        function next() { game.count += step * 10; return game.count }
        """
      And a reload happens
      And the method "tick" of "root" is called
      Then the property "count" of "root" is 16
      And the log has no warning matching "TypeError"

    Scenario: A variable whose starting value was edited takes the new value
      When the method "tick" of "root" is called
      And the file "logic.js" is changed to:
        """js
        .pragma library
        var game
        var step = 3
        function start() { game = { count: 5 } }
        function next() { game.count += step; return game.count }
        """
      And a reload happens
      And the method "tick" of "root" is called
      Then the property "count" of "root" is 9

  Rule: What a script of an object holds survives a reload

    Scenario: A script without ".pragma library" keeps its variables
      Given the file "counter.js":
        """js
        var hits = 0
        function hit() { hits += 1; return hits }
        """
      And the QML:
        """qml
        import QtQuick
        import "counter.js" as Counter
        Item {
          objectName: "root"
          property int hits: 0
          function hit() { hits = Counter.hit() }
        }
        """
      When the method "hit" of "root" is called
      And the method "hit" of "root" is called
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import "counter.js" as Counter
        Item {
          objectName: "root"
          width: 10
          property int hits: 0
          function hit() { hits = Counter.hit() }
        }
        """
      And a reload happens
      And the method "hit" of "root" is called
      Then the property "hits" of "root" is 3

  Rule: An object a singleton holds still fits a property typed by its file

    Background:
      Given the file "qmldir":
        """
        singleton Rooms Rooms.qml
        Room Room.qml
        """
      And the file "Room.qml":
        """qml
        import QtQuick
        QtObject { property string name }
        """
      And the file "Rooms.qml":
        """qml
        pragma Singleton
        import QtQuick
        QtObject {
          property list<Room> all: [ Room { name: "Kitchen" }, Room { name: "Study" } ]
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property Room first: Rooms.all[0]
          property string names: ""
          Repeater {
            model: Rooms.all
            Item {
              required property Room modelData
              Component.onCompleted: root.names += modelData.name + " "
            }
          }
        }
        """

    Scenario: A property typed by the file still holds the singleton's object
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 10
          property Room first: Rooms.all[0]
          property string names: ""
          property string firstName: first ? first.name : "none"
          Repeater {
            model: Rooms.all
            Item {
              required property Room modelData
              Component.onCompleted: root.names += modelData.name + " "
            }
          }
        }
        """
      And a reload happens
      Then the property "firstName" of "root" is "Kitchen"
      And the log has no warnings

    Scenario: A delegate's required property typed by the file takes the singleton's object
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 10
          property Room first: Rooms.all[0]
          property string names: ""
          Repeater {
            model: Rooms.all
            Item {
              objectName: "made"
              required property Room modelData
              Component.onCompleted: root.names += modelData.name + " "
            }
          }
        }
        """
      And a reload happens
      Then the log has no warnings

  Rule: A page given a singleton's objects keeps them across a reload

    Background:
      Given the file "qmldir":
        """
        singleton Rooms Rooms.qml
        Room Room.qml
        """
      And the file "Room.qml":
        """qml
        import QtQuick
        QtObject { property string name }
        """
      And the file "Rooms.qml":
        """qml
        pragma Singleton
        import QtQuick
        QtObject {
          property list<Room> all: [ Room { name: "Kitchen" }, Room { name: "Study" } ]
        }
        """
      And the file "Page.qml":
        """qml
        import QtQuick
        Item {
          required property list<Room> rooms
          property string names: ""
          Repeater {
            model: rooms
            Item {
              required property Room modelData
              property string seen: modelData.name
            }
          }
        }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root
          objectName: "root"
          width: 300; height: 200
          StackView {
            id: stack
            objectName: "stack"
            anchors.fill: parent
            initialItem: Item {}
          }
          function open() { stack.replace("Page.qml", { rooms: Rooms.all }, StackView.Immediate) }
        }
        """

    Scenario: The page still has the objects after its file changes
      When the method "open" of "root" is called
      And the file "Page.qml" is changed to:
        """qml
        import QtQuick
        Item {
          width: 10
          required property list<Room> rooms
          property string names: ""
          Repeater {
            model: rooms
            Item {
              required property Room modelData
              property string seen: modelData.name
            }
          }
        }
        """
      And a reload happens
      Then the property "currentItem.rooms.length" of "stack" is 2
      And the property "currentItem.width" of "stack" is 10
      And the log has no warnings

    Scenario: The page still has the objects after the file that pushed it changes
      When the method "open" of "root" is called
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root
          objectName: "root"
          width: 300; height: 201
          StackView {
            id: stack
            objectName: "stack"
            anchors.fill: parent
            initialItem: Item {}
          }
          function open() { stack.replace("Page.qml", { rooms: Rooms.all }, StackView.Immediate) }
        }
        """
      And a reload happens
      Then the property "currentItem.rooms.length" of "stack" is 2
      And the log has no warnings
