# Sources:
#   docs/ARCHITECTURE.md (section 12.1)
#   docs/research/03-runtime-concurrency-hot-reload.md
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#completed-signal
# Hot reload is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: What runs at a reload
  An object that is kept across a reload is not completed a second time: it
  is told that it was reloaded instead. A file can ask to start afresh at
  every reload.

  Rule: Component.onCompleted does not run again on a kept object

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string events: ""
          Component.onCompleted: events += "completed "
        }
        """

    Scenario: A kept object is not completed again
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 10
          property string events: ""
          Component.onCompleted: events += "completed "
        }
        """
      And a reload happens
      Then the property "events" of "root" is "completed "
      And the property "width" of "root" is 10

    Scenario: An object the reload added is completed
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string events: ""
          Component.onCompleted: events += "completed "
          Item { Component.onCompleted: root.events += "child " }
        }
        """
      And a reload happens
      Then the property "events" of "root" is "completed child "

  Rule: Component.onReloaded runs on a kept object after each reload

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string events: ""
          Component.onCompleted: events += "completed "
          Component.onReloaded: events += "reloaded "
        }
        """

    Scenario: A new object is not told that it was reloaded
      Then the property "events" of "root" is "completed "

    Scenario: A kept object is told that it was reloaded
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 10
          property string events: ""
          Component.onCompleted: events += "completed "
          Component.onReloaded: events += "reloaded "
        }
        """
      And a reload happens
      Then the property "events" of "root" is "completed reloaded "

    Scenario: The handler sees the reloaded object
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 10
          property string events: ""
          Component.onCompleted: events += "completed "
          Component.onReloaded: events += "reloaded at " + width + " "
        }
        """
      And a reload happens
      Then the property "events" of "root" is "completed reloaded at 10 "

    Scenario: An object the reload added is completed and not told that it was reloaded
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string events: ""
          Component.onCompleted: events += "completed "
          Component.onReloaded: events += "reloaded "
          Item {
            Component.onCompleted: root.events += "child "
            Component.onReloaded: root.events += "child-reloaded "
          }
        }
        """
      And a reload happens
      Then the property "events" of "root" is "completed child reloaded "

  Rule: Component.onDestruction runs only on an object that is gone after the reload

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Component.onDestruction: console.warn("root destroyed")
          Item { id: child; Component.onDestruction: console.warn("child destroyed") }
        }
        """

    Scenario: A kept object is not told that it is destroyed
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          Component.onDestruction: console.warn("root destroyed")
          Item { id: child; Component.onDestruction: console.warn("child destroyed") }
        }
        """
      And a reload happens
      Then the log has no warning matching "destroyed"

    Scenario: An object the reload removed is told that it is destroyed
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Component.onDestruction: console.warn("root destroyed")
        }
        """
      And a reload happens
      Then the log has a warning matching "child destroyed"
      And the log has no warning matching "root destroyed"

  Rule: A file with "pragma LjosReload: reset" starts afresh at every reload

    Scenario: The objects of the file are new objects
      Given the QML:
        """qml
        pragma LjosReload: reset
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property int taps: 0
          property string events: ""
          Component.onCompleted: events += "completed "
        }
        """
      When the property "taps" of "root" is set to 3
      And the file "Scenario.qml" is changed to:
        """qml
        pragma LjosReload: reset
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 10
          property int taps: 0
          property string events: ""
          Component.onCompleted: events += "completed "
        }
        """
      And a reload happens
      Then the property "taps" of "root" is 0
      And the property "events" of "root" is "completed "

    Scenario: Only the objects of that file start afresh
      Given the file "Counter.qml":
        """qml
        pragma LjosReload: reset
        import QtQuick
        Item { property int count: 0 }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int taps: 0
          Counter { id: counter; objectName: "counter" }
        }
        """
      When the property "taps" of "root" is set to 3
      And the property "count" of "counter" is set to 5
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          property int taps: 0
          Counter { id: counter; objectName: "counter" }
        }
        """
      And a reload happens
      Then the property "taps" of "root" is 3
      And the property "count" of "counter" is 0
