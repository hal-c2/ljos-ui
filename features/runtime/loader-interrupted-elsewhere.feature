# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#status-prop
#   docs/ARCHITECTURE.md §6.1 (nested runs: a dispatch's JavaScript as a whole may take 4 hard limits)
#   docs/ARCHITECTURE.md §9.4 (Supervisor: UI boundaries, Loader)
#   https://github.com/hal-c2/ljos-ui/issues/524

# Qt has no time limit on JavaScript: both taps run to their end and the item stays.
@ljos-only
Feature: A Loader gives up on its item only for scripts its item could set off again
  A Loader that loads again after an interrupted script gives up when the next script is
  interrupted soon after, because loading again would only be interrupted again. That holds for
  a script that runs while the item is made, and for the item's own code. A short handler of an
  item that sets off long scripts of other objects after the item was made is not such a case:
  a click on it sets them off, not loading it again, so the Loader loads its item again, until
  such interrupts keep coming back on their own.

  Background:
    Given JavaScript is timed by a clock on which native work a script asks for takes 0 seconds
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property bool wanted: false
        property bool atOnce: false
        property bool slowRow: false
        property bool timed: false
        property int work: 0
        property int step: 0
        property int made: 0
        onWorkChanged: { for (let k = 1; k <= 30; k++) step = k }
        onStepChanged: { let s = 0; for (let i = 0; i < 1000000; i++) s += i }
        Loader {
          id: loader; objectName: "loader"; active: root.wanted
          sourceComponent: Item {
            objectName: "row"
            Component.onCompleted: { root.made += 1; if (root.atOnce) root.work += 1 }
            Timer { interval: 1; running: root.timed; onTriggered: root.work += 1 }
            function tap() {
              if (root.slowRow) for (;;) { }
              root.work += 1
            }
          }
        }
      }
      """

  Scenario: An item whose handler sets off long scripts elsewhere is made again each time
    When the property "wanted" of "root" is set to true
    And the method "tap" of "loader/row" is called
    And 100 milliseconds pass
    And the method "tap" of "loader/row" is called
    And 100 milliseconds pass
    Then the property "status" of "loader" is 1
    And the object "loader/row" exists
    And the property "made" of "root" is 3
    And the log has a warning matching "interrupted"
    And the log has no warning matching "Loader: .*interrupted again"
    And the application has no fault

  Scenario: An item whose own handler is interrupted twice stays unloaded
    When the property "slowRow" of "root" is set to true
    And the property "wanted" of "root" is set to true
    And the method "tap" of "loader/row" is called
    And 100 milliseconds pass
    And the method "tap" of "loader/row" is called
    And 100 milliseconds pass
    Then the property "status" of "loader" is 3
    And the property "item" of "loader" is null
    And the property "made" of "root" is 2
    And the log has a warning matching "Loader: .*interrupted again.*stays at Loader.Error"

  Scenario: An item whose making sets off long scripts elsewhere stays unloaded the second time
    When the property "atOnce" of "root" is set to true
    And the property "wanted" of "root" is set to true
    And 100 milliseconds pass
    Then the property "status" of "loader" is 3
    And the property "item" of "loader" is null
    And the property "made" of "root" is 2
    And the log has a warning matching "Loader: .*interrupted again.*stays at Loader.Error"

  Scenario: An item whose own timer sets off long scripts elsewhere each time it is made gives up
    When the property "timed" of "root" is set to true
    And the property "wanted" of "root" is set to true
    And 2 seconds pass
    Then the property "status" of "loader" is 3
    And the property "item" of "loader" is null
    And the property "made" of "root" is 8
    And the log has a warning matching "Loader: .*interrupted 8 times in a row.*stays at Loader.Error"
    And the application has no fault
