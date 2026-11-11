# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   docs/DIALECT.md (section 4.11, creation order)
#
# Qt 6.11.2 and 6.12.0 (qml6, offscreen) write the initial properties one
# by one and tell each change as it lands, before the object's bindings
# run. A binding that such a handler reads and that has not run yet is run
# in that read, and its own change is told inside it. With the files below
# Qt logs "eval3 dbl6 w3/6 evtri tri8 completed3/6", and with a handler
# that writes what a binding reads, "eval3 dbl6 w3/6 evtri tri2
# completed3/6": the binding it made stale had not run yet, so it runs
# once. Ljós differs on purpose: the initial properties and the
# bindings settle together and the handlers run after, so each handler
# sees a settled object. A handler never reads a half-made one, and an
# append in a handler is never lost to a nested one. (#331)

@qt-diverges
Feature: An object's handlers run once its initial properties and bindings have settled
  An object made with initial properties is given them and has its
  bindings run before any of its change handlers is told. The handlers
  then run, each seeing every property as the object will have it, and
  the object is completed after them.

  Background:
    Given the file "Shown.qml":
      """qml
      import QtQuick
      Item {
        objectName: "shown"
        property bool writes: false
        property int w: 0
        property int dbl: { root.say("eval" + w); return w * 2 }
        property int other: 7
        property int tri: { root.say("evtri"); return other + 1 }
        onWChanged: {
          root.say("w" + w + "/" + dbl)
          if (writes) other = 1
        }
        onDblChanged: root.say("dbl" + dbl)
        onTriChanged: root.say("tri" + tri)
        Component.onCompleted: root.say("completed" + w + "/" + dbl)
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var seen: []
        property string log: ""
        function say(s) { seen.push(s) }
        Component { id: shown; Shown { } }
        Component { id: writer; Shown { writes: true } }
        function make(writes) {
          (writes ? writer : shown).createObject(root, { w: 3 })
          log = seen.join(" ")
        }
      }
      """

  Scenario: The bindings run before the initial property's handler
    When the method "make" of "root" is called with false
    Then the property "log" of "root" is "eval3 evtri w3/6 dbl6 tri8 completed3/6"

  Scenario: A binding a handler makes stale runs again after it
    When the method "make" of "root" is called with true
    Then the property "log" of "root" is "eval3 evtri w3/6 dbl6 evtri tri2 completed3/6"
