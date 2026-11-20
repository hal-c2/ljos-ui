# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html
#   docs/ARCHITECTURE.md §9.3 (faults: JsInterrupted, JsOutOfMemory)
#   docs/ARCHITECTURE.md §9.4 (Supervisor: content a resource fault took is made again)

# Qt has no time limit on JavaScript: a script that never returns hangs the application.
@ljos-only
Feature: A delegate whose script was interrupted is made again
  An interrupted script, like a full script heap, may not happen again, so a
  Repeater or a view makes the delegate of that row again by itself, without
  its model changing. A row that is interrupted again soon after waits
  1 second, then twice as long each time, up to a minute. A view never gives
  up on a row, and the fault is not the application's. Every interrupt is in
  the log.

  Background:
    Given JavaScript is timed by a clock on which native work a script asks for takes 0 seconds

  Rule: A Repeater makes the row again

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property int rows: 0
          property bool always: false
          property int tries: 0
          Column {
            Repeater {
              objectName: "repeater"
              model: root.rows
              delegate: Item {
                objectName: "row" + index
                width: 10; height: 10
                Component.onCompleted: {
                  root.tries += 1
                  if (root.always || root.tries === 1) for (;;) { }
                }
              }
            }
          }
        }
        """

    Scenario: A row whose script is interrupted once is made again
      When the property "rows" of "root" is set to 1
      Then the property "tries" of "root" is 2
      And the property "count" of "repeater" is 1
      And the object "row0" exists
      And the log has a warning matching "interrupted"
      And the log has a warning matching "the delegate of row 0 is made again at once"
      And the application has no fault

    Scenario: A row whose script is interrupted again waits, then is made again
      When the property "always" of "root" is set to true
      And the property "rows" of "root" is set to 1
      Then the property "tries" of "root" is 2
      And the log has a warning matching "the delegate of row 0 is made again in 1 s"
      And the application has no fault
      When 999 milliseconds pass
      Then the property "tries" of "root" is 2
      When 1 millisecond passes
      Then the property "tries" of "root" is 3
      And the log has a warning matching "the delegate of row 0 is made again in 2 s"

    Scenario: A row whose script stops looping comes back by itself
      When the property "always" of "root" is set to true
      And the property "rows" of "root" is set to 1
      And the property "always" of "root" is set to false
      And 1 second passes
      Then the property "tries" of "root" is 3
      And the object "row0" exists
      And the application has no fault

    Scenario: A row the model took while it waited is not made again
      When the property "always" of "root" is set to true
      And the property "rows" of "root" is set to 1
      And the property "rows" of "root" is set to 0
      And 5 seconds pass
      Then the property "tries" of "root" is 2
      And the application has nothing scheduled
      And the application has no fault

  Rule: A view makes the row again

    Scenario: A ListView row whose script is interrupted once is made again
      Given the QML:
        """qml
        import QtQuick
        ListView {
          id: root
          objectName: "root"
          width: 100; height: 100
          property int tries: 0
          model: 1
          delegate: Item {
            objectName: "row" + index
            width: 100; height: 10
            Component.onCompleted: {
              root.tries += 1
              if (root.tries === 1) for (;;) { }
            }
          }
        }
        """
      Then the property "tries" of "root" is 2
      And the object "row0" exists
      And the log has a warning matching "the delegate of row 0 is made again at once"
      And the application has no fault

    Scenario: A code bug in a row is not made again
      Given the QML:
        """qml
        import QtQuick
        ListView {
          id: root
          objectName: "root"
          width: 100; height: 100
          property int tries: 0
          model: 1
          delegate: Item {
            objectName: "row" + index
            width: 100; height: 10
            Component.onCompleted: {
              root.tries += 1
              throw new Error("broken row")
            }
          }
        }
        """
      Then the property "tries" of "root" is 1
      And the log has a warning matching "broken row"
      And the log has no warning matching "made again"
