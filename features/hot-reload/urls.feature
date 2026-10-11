# Sources:
#   docs/ARCHITECTURE.md (section 12.1)
#   docs/research/03-runtime-concurrency-hot-reload.md
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#source-prop
# Hot reload is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: Reloaded files and scripts are found again
  A development tool may tag the address of a file with the generation it
  wants. The tag does not change which file is meant. The script of a
  reloaded file is the new script.

  Rule: A query string on a file's address does not change which file it is

    Background:
      Given the file "Panel.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "panel"; width: 30 }
        """

    Scenario: A Loader finds a file whose address carries a generation
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { objectName: "loader"; source: "Panel.qml?generation=3" }
        }
        """
      Then the property "width" of "loader/panel" is 30
      And the log has no warnings

    # Waits for Qt.createComponent in scripts: an application can be given
    # the Component of a file, but a script cannot ask for one yet.
    @blocked
    Scenario: Qt.createComponent finds a file whose address carries a generation
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var made: Qt.createComponent("Panel.qml?generation=2")
          property bool ready: made.status === Component.Ready
        }
        """
      Then the property "ready" of "root" is true

    Scenario: Any other query string is ignored as well
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { objectName: "loader"; source: "Panel.qml?theme=dark&x=1#top" }
        }
        """
      Then the object "loader/panel" exists

  Rule: The script of a reloaded file is the new script

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int seen: 0
          function answer() { return 1 }
          function ask() { seen = answer() }
        }
        """

    Scenario: A function that changed returns its new answer
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int seen: 0
          function answer() { return 2 }
          function ask() { seen = answer() }
        }
        """
      And a reload happens
      And the method "ask" of "root" is called
      Then the property "seen" of "root" is 2

    Scenario: It does so again after the next reload
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int seen: 0
          function answer() { return 2 }
          function ask() { seen = answer() }
        }
        """
      And a reload happens
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int seen: 0
          function answer() { return 3 }
          function ask() { seen = answer() }
        }
        """
      And a reload happens
      And the method "ask" of "root" is called
      Then the property "seen" of "root" is 3

    Scenario: The script of a file that did not change still runs
      Given the file "Counter.qml":
        """qml
        import QtQuick
        Item {
          property int count: 0
          function more() { count += 1 }
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
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          Counter { id: counter; objectName: "counter" }
        }
        """
      And a reload happens
      And the method "more" of "counter" is called
      Then the property "count" of "counter" is 1
