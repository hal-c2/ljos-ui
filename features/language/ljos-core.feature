# Sources:
#   docs/DIALECT.md (§4.1 the modules, §4.3 globals, §6 the compat pass)
#   https://doc.qt.io/qt-6/qtqml-index.html
#   https://doc.qt.io/qt-6/qml-qtqml-timer.html
#   https://doc.qt.io/qt-6/qml-qtqml-connections.html
#   https://doc.qt.io/qt-6/qml-qtqml-binding.html
#   https://doc.qt.io/qt-6/qml-qtqml-component.html
#   https://doc.qt.io/qt-6/qml-qtqml-qtobject.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Core` is Ljós's own module: what a file says that is not an item. The file as written in
# Qt's QML is the oracle, and the same file in Ljós's API, and the file lowered, are held to its
# numbers. The timer fires once, 100 milliseconds after the root is complete; it adds 3 to the
# counter, which the binding doubles and the connection hears.

Feature: Ljos.Core: timers, connections, bindings and components
  What a file says that is not an item, a timer, a handler for another
  object's signals, a binding set from outside, a component and a plain
  object of properties, is `Ljos.Core`'s. A file in Ljós's API imports it
  beside `Ljos.Ui`, and `Ljos.Ui` alone does not have it. A file in Qt's
  QML has it from `QtQml`, which `QtQuick` brings, and is lowered to
  import `Ljos.Core` in its place.

  Rule: Ljos.Core does what QtQml does

    @qt-parity
    Scenario: A timer, a connection, a binding and a component, as written in Qt's QML
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int ticks: 0
          property int heard: 0
          property int doubled: -1
          property bool ready: false
          QtObject { id: counter; objectName: "counter"; property int count: 0; signal bumped() }
          Timer {
            interval: 100; running: root.ready
            onTriggered: { root.ticks += 1; counter.count += 3; counter.bumped() }
          }
          Connections { target: counter; function onBumped() { root.heard += 1 } }
          Binding { target: root; property: "doubled"; value: counter.count * 2; when: root.ready }
          Component { id: dot; Item { objectName: "dot"; width: 7 } }
          Component.onCompleted: { ready = true; dot.createObject(root) }
        }
        """
      Then the property "doubled" of "root" is 0
      And the property "width" of "dot" is 7
      When 150 milliseconds pass
      Then the property "ticks" of "root" is 1
      And the property "heard" of "root" is 1
      And the property "count" of "counter" is 3
      And the property "doubled" of "root" is 6
      When 200 milliseconds pass
      Then the property "ticks" of "root" is 1

    @ljos-only
    Scenario: The same file in Ljós's API imports Ljos.Core beside Ljos.Ui
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property int ticks: 0
          property int heard: 0
          property int doubled: -1
          property bool ready: false
          QtObject { id: counter; objectName: "counter"; property int count: 0; signal bumped() }
          Timer {
            interval: 100; running: root.ready
            onTriggered: { root.ticks += 1; counter.count += 3; counter.bumped() }
          }
          Connections { target: counter; function onBumped() { root.heard += 1 } }
          Binding { target: root; property: "doubled"; value: counter.count * 2; when: root.ready }
          Component { id: dot; Item { objectName: "dot"; width: 7 } }
          Component.onCompleted: { ready = true; dot.createObject(root) }
        }
        """
      Then the property "doubled" of "root" is 0
      And the property "width" of "dot" is 7
      When 150 milliseconds pass
      Then the property "ticks" of "root" is 1
      And the property "heard" of "root" is 1
      And the property "count" of "counter" is 3
      And the property "doubled" of "root" is 6
      When 200 milliseconds pass
      Then the property "ticks" of "root" is 1

    @ljos-only @headless
    Scenario: A file in Ljós's API that does not import Ljos.Core has no Timer
      Given the file "Pulse.qml":
        """qml
        import Ljos.Ui
        Item {
          Timer { interval: 100 }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Pulse.qml:3:11: error: `Timer` is not a type of anything the file imports [unknown-type]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

  Rule: Qt's QtQml is lowered to Ljos.Core

    @ljos-only
    Scenario: The file as written in Qt's QML, lowered, holds the same numbers
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int ticks: 0
          property int heard: 0
          property int doubled: -1
          property bool ready: false
          QtObject { id: counter; objectName: "counter"; property int count: 0; signal bumped() }
          Timer {
            interval: 100; running: root.ready
            onTriggered: { root.ticks += 1; counter.count += 3; counter.bumped() }
          }
          Connections { target: counter; function onBumped() { root.heard += 1 } }
          Binding { target: root; property: "doubled"; value: counter.count * 2; when: root.ready }
          Component { id: dot; Item { objectName: "dot"; width: 7 } }
          Component.onCompleted: { ready = true; dot.createObject(root) }
        }
        """
      Then the property "doubled" of "root" is 0
      And the property "width" of "dot" is 7
      When 150 milliseconds pass
      Then the property "ticks" of "root" is 1
      And the property "heard" of "root" is 1
      And the property "count" of "counter" is 3
      And the property "doubled" of "root" is 6
      When 200 milliseconds pass
      Then the property "ticks" of "root" is 1

    @ljos-only @headless
    Scenario: A file that names what QtQml has imports Ljos.Core when it is migrated
      Given the file "Pulse.qml":
        """qml
        import QtQuick
        Item {
          property int ticks: 0
          Timer { interval: 100; running: true; onTriggered: parent.ticks += 1 }
        }
        """
      And the file "Clock.qml":
        """qml
        import QtQml
        QtObject {
          property int hours: 0
        }
        """
      And the file "Dial.qml":
        """qml
        import QtQuick
        import QtQml
        Rectangle {
          color: "#336699"
          Component.onCompleted: color = "#993366"
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Clock.qml (1 edits)
        lowered ./Dial.qml (4 edits)
        lowered ./Pulse.qml (1 edits)
        3 files, 3 lowered, 0 kept
        """
      And the file "Pulse.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core
        Item {
          property int ticks: 0
          Timer { interval: 100; running: true; onTriggered: parent.ticks += 1 }
        }
        """
      And the file "Clock.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Core
        QtObject {
          property int hours: 0
        }
        """
      And the file "Dial.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui
        import Ljos.Core
        Rectangle {
          fill: "#336699"
          Component.onCompleted: fill = "#993366"
        }
        """

    @ljos-only @headless
    Scenario: A file that writes a type of QtQml's imports Ljos.Core when it is migrated
      Given the file "Holder.qml":
        """qml
        import QtQuick
        Item {
          property QtObject o
          property Component c
          property list<QtObject> xs
          signal picked(QtObject o)
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """
      When the author runs "ljos migrate --write ."
      Then the file "Holder.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core
        Item {
          property QtObject o
          property Component c
          property list<QtObject> xs
          signal picked(QtObject o)
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    @ljos-only @headless
    Scenario: A file that names a model of QtQml's has it from Ljos.Core until Ljos.Models has it
      Given the file "Names.qml":
        """qml
        import QtQuick
        Item {
          ListModel { id: names; ListElement { name: "a" } }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Names.qml (1 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Names.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core
        Item {
          ListModel { id: names; ListElement { name: "a" } }
        }
        """

    @ljos-only @headless
    Scenario: A file that names nothing of QtQml's is lowered without Ljos.Core
      Given the file "Swatch.qml":
        """qml
        import QtQuick
        Rectangle { width: 40; height: 40; color: "#336699" }
        """
      When the author runs "ljos migrate --write ."
      Then the file "Swatch.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui
        Rectangle { width: 40; height: 40; fill: "#336699" }
        """
