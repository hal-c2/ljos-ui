# Sources:
#   docs/ARCHITECTURE.md §6.1 (a hard limit on one run of JavaScript)
#   docs/ARCHITECTURE.md §9.3 (faults: JsInterrupted)
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html

# Qt has no time limit on JavaScript to compare with.
@ljos-only
Feature: The time limit on a script counts only the script's own time
  On a busy machine an application loses the processor while it runs. That time is nobody's
  script, so it does not count against the time limit on JavaScript, nor does the native work of
  making an object. A script that never returns is still stopped.

  Background:
    Given JavaScript is timed by a clock on which the application loses the processor for 2 seconds at every poll
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool wanted: false
        property bool spin: false
        property int completed: 0
        Component {
          id: page
          Item {
            objectName: "page"
            Component.onCompleted: {
              let s = 0
              for (let i = 0; i < 30000 || root.spin; i++) s += i
              root.completed += 1
            }
          }
        }
        Loader { id: loader; objectName: "loader"; active: root.wanted; sourceComponent: page }
      }
      """

  Scenario: A Loader makes its item though the application lost the processor
    When the property "wanted" of "root" is set to true
    Then the property "completed" of "root" is 1
    And the property "status" of "loader" is 1
    And the object "loader/page" exists
    And the log has no warning matching "interrupted"

  Scenario: A script that never returns is interrupted all the same
    When the property "spin" of "root" is set to true
    And the property "wanted" of "root" is set to true
    Then the log has a warning matching "interrupted"
    And the property "completed" of "root" is 0
