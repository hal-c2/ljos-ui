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
