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
  a click on it sets them off, not loading it again, so the Loader loads its item again however
  often someone clicks. Only interrupts of scripts no input set off, the item setting them off
  again each time it is made, still make the Loader give up, whatever else is clicked.

  Background:
    Given JavaScript is timed by a clock on which native work a script asks for takes 0 seconds
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property bool wanted: false
        property bool atOnce: false
        property bool slowRow: false
        property bool timed: false
        property int work: 0; property int step: 0
        property int made: 0; property int gap: 1
        onWorkChanged: { for (let k = 1; k <= 30; k++) step = k }
        onStepChanged: { let s = 0; for (let i = 0; i < 1000000; i++) s += i }
        Loader {
          id: loader; objectName: "loader"; active: root.wanted
          sourceComponent: Item {
            objectName: "row"; width: 100; height: 100
            Accessible.role: Accessible.Button; Accessible.name: "Row"
            Component.onCompleted: { root.made += 1; if (root.atOnce) root.work += 1 }
            Timer { interval: root.gap; running: root.timed; onTriggered: root.work += 1 }
            TapHandler { onTapped: parent.tap() }
            function tap() { if (root.slowRow) for (;;) { } root.work += 1 }
          }
        }
        Item { x: 100; width: 100; height: 100; TapHandler { }
          Accessible.role: Accessible.Button; Accessible.name: "Other" }
      }
      """

  Scenario: An item whose handler sets off long scripts elsewhere is made again each time
    When the property "wanted" of "root" is set to true
    And the user clicks the "button" "Row"
    And 100 milliseconds pass
    And the user clicks the "button" "Row"
    And 100 milliseconds pass
    Then the property "status" of "loader" is 1
    And the object "loader/row" exists
    And the property "made" of "root" is 3
    And the log has a warning matching "interrupted"
    And the log has no warning matching "Loader: .*interrupted again"
    And the application has no fault

  Scenario: An item clicked more times than an item may set the scripts off by itself is kept
    When the property "wanted" of "root" is set to true
    And the user clicks the "button" "Row"
    And 100 milliseconds pass
    And the user clicks the "button" "Row"
    And 100 milliseconds pass
    And the user clicks the "button" "Row"
    And 100 milliseconds pass
    And the user clicks the "button" "Row"
    And 100 milliseconds pass
    And the user clicks the "button" "Row"
    And 100 milliseconds pass
    And the user clicks the "button" "Row"
    And 100 milliseconds pass
    And the user clicks the "button" "Row"
    And 100 milliseconds pass
    And the user clicks the "button" "Row"
    And 100 milliseconds pass
    And the user clicks the "button" "Row"
    And 100 milliseconds pass
    And the user clicks the "button" "Row"
    And 100 milliseconds pass
    Then the property "status" of "loader" is 1
    And the object "loader/row" exists
    And the property "made" of "root" is 11
    And the log has no warning matching "Loader: .*(interrupted again|times in a row)"
    And the application has no fault

  Scenario: An item whose own handler is interrupted twice stays unloaded
    When the property "slowRow" of "root" is set to true
    And the property "wanted" of "root" is set to true
    And the user clicks the "button" "Row"
    And 100 milliseconds pass
    And the user clicks the "button" "Row"
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

  Scenario: Clicks elsewhere do not keep an item whose own timer sets off long scripts elsewhere
    When the property "gap" of "root" is set to 500
    And the property "timed" of "root" is set to true
    And the property "wanted" of "root" is set to true
    And 1 second passes
    And the user clicks the "button" "Other"
    And 1 second passes
    And the user clicks the "button" "Other"
    And 1 second passes
    And the user clicks the "button" "Other"
    And 1 second passes
    And the user clicks the "button" "Other"
    And 1 second passes
    And the user clicks the "button" "Other"
    And 1 second passes
    And the user clicks the "button" "Other"
    And 1 second passes
    And the user clicks the "button" "Other"
    And 1 second passes
    And the user clicks the "button" "Other"
    And 1 second passes
    And the user clicks the "button" "Other"
    And 2 seconds pass
    Then the property "status" of "loader" is 3
    And the property "made" of "root" is 8
    And the log has a warning matching "Loader: .*interrupted 8 times in a row.*stays at Loader.Error"
