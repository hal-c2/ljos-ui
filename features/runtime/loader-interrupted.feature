# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#status-prop
#   docs/ARCHITECTURE.md §9.3 (faults: JsInterrupted)
#   docs/ARCHITECTURE.md §9.4 (Supervisor: UI boundaries, Loader)

# Qt has no time limit on JavaScript: a script that never returns hangs the application.
@ljos-only
Feature: A Loader whose script was interrupted loads again, and never gives up
  What a script did before it was interrupted may not happen again, so a Loader loads the item
  again rather than lose it for good. A script that is interrupted again soon after leaves the
  Loader at `Loader.Error` for a while: it waits 1 second, then twice as long each time it is
  interrupted again soon after, up to a minute, and loads again, so an item whose script stops
  looping comes back by itself. Told to load, it loads at once. Every interrupt is in the log.

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

  Scenario: An item whose script is interrupted again waits, then loads again
    When the property "always" of "root" is set to true
    And the property "wanted" of "root" is set to true
    Then the property "tries" of "root" is 2
    And the property "status" of "loader" is 3
    And the property "item" of "loader" is null
    And the log has a warning matching "Loader: .*interrupted again.*stays at Loader.Error and loads again in 1 s"
    And the application has no fault
    When 999 milliseconds pass
    Then the property "tries" of "root" is 2
    When 1 millisecond passes
    Then the property "tries" of "root" is 3
    And the property "status" of "loader" is 3
    And the log has a warning matching "Loader: .*loads again in 2 s"

  Scenario: An item whose script stops looping comes back by itself
    When the property "always" of "root" is set to true
    And the property "wanted" of "root" is set to true
    And the property "always" of "root" is set to false
    And 1 second passes
    Then the property "tries" of "root" is 3
    And the property "status" of "loader" is 1
    And the object "loader/page" exists
    And the application has no fault

  Scenario: Told to load again soon after, it loads at once and waits longer after
    When the property "always" of "root" is set to true
    And the property "wanted" of "root" is set to true
    And the property "wanted" of "root" is set to false
    And the property "wanted" of "root" is set to true
    Then the property "tries" of "root" is 3
    And the property "status" of "loader" is 3
    And the log has a warning matching "Loader: .*loads again in 2 s"

  Scenario: Told to load again much later, it loads again at once after a fault again
    When the property "always" of "root" is set to true
    And the property "wanted" of "root" is set to true
    And the property "wanted" of "root" is set to false
    And 11 seconds pass
    And the property "wanted" of "root" is set to true
    Then the property "tries" of "root" is 4
    And the property "status" of "loader" is 3
