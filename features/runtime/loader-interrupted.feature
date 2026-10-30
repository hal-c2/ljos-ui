# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#status-prop
#   docs/ARCHITECTURE.md §9.3 (faults: JsInterrupted)
#   docs/ARCHITECTURE.md §9.4 (Supervisor: UI boundaries, Loader)

# Qt has no time limit on JavaScript: a script that never returns hangs the application.
@ljos-only
Feature: A Loader whose script was interrupted loads again once
  What a script did before it was interrupted may not happen again, so a Loader gives the item a
  second chance rather than lose it for good. A script that is interrupted again leaves the
  Loader at `Loader.Error`, and the log says so. Every interrupt is in the log.

  Background:
    Given JavaScript is timed by a clock on which native work a script asks for takes 0 seconds
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool wanted: false
        property bool always: false
        property int tries: 0
        Component {
          id: page
          Item {
            objectName: "page"
            Component.onCompleted: {
              root.tries += 1
              if (root.always || root.tries === 1) for (;;) { }
            }
          }
        }
        Loader { id: loader; objectName: "loader"; active: root.wanted; sourceComponent: page }
      }
      """

  Scenario: An item whose script is interrupted once is made again
    When the property "wanted" of "root" is set to true
    Then the property "tries" of "root" is 2
    And the property "status" of "loader" is 1
    And the object "loader/page" exists
    And the log has a warning matching "interrupted"
    And the log has a warning matching "Loader: .*loading again"
    And the application has no fault

  Scenario: An item whose script is interrupted again stays unloaded and the log says so
    When the property "always" of "root" is set to true
    And the property "wanted" of "root" is set to true
    Then the property "tries" of "root" is 2
    And the property "status" of "loader" is 3
    And the property "item" of "loader" is null
    And the log has a warning matching "Loader: .*interrupted again.*stays at Loader.Error"
    And the application has no fault

  Scenario: Told to load again soon after, it is not given a second chance
    When the property "always" of "root" is set to true
    And the property "wanted" of "root" is set to true
    And the property "wanted" of "root" is set to false
    And the property "wanted" of "root" is set to true
    Then the property "tries" of "root" is 3
    And the property "status" of "loader" is 3

  Scenario: Told to load again much later, it is given a second chance again
    When the property "always" of "root" is set to true
    And the property "wanted" of "root" is set to true
    And the property "wanted" of "root" is set to false
    And 11 seconds pass
    And the property "wanted" of "root" is set to true
    Then the property "tries" of "root" is 4
    And the property "status" of "loader" is 3
