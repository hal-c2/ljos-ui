# Sources:
#   docs/ARCHITECTURE.md sections 3.3 (0 % CPU when idle) and 19.5
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: What will wake the application
  `app_processes` with the kind `waker` lists everything that will wake
  the UI thread: running Timers with their interval and next firing,
  running animations, answers it waits for (a file read, a fetch, a
  dialog), pictures loading and messages posted, each with the object it
  is for and where that is declared. What only listens for news that may
  never come, a file watcher, is listed apart. An application at rest
  has nothing scheduled: the 0 % CPU rule made visible.

  Scenario: An application at rest has nothing to wake it
    Given the QML:
      """qml
      import QtQuick
      Item {
        Timer { interval: 250; running: false }
        Rectangle { width: 10; height: 10 }
      }
      """
    And the inspector is connected
    When the agent calls the tool "app_processes" with:
      """json
      {"kind": "waker"}
      """
    Then the answer's "/scheduled" is 0
    And the answer's "/processes" has 0 entries
    And the answer's text contains "nothing will wake the UI thread"

  Scenario: A running Timer is listed with where it is declared and when it fires next
    Given the QML:
      """qml
      import QtQuick
      Item {
        Timer { id: tick; interval: 250; repeat: true; running: true }
      }
      """
    And the inspector is connected
    When the agent calls the tool "app_processes" with:
      """json
      {"kind": "waker"}
      """
    Then the answer's "/scheduled" is 1
    And the answer's "/processes/0/source" is "timer"
    And the answer's "/processes/0/locator" is "id=tick"
    And the answer's "/processes/0/at" matches "Scenario\.qml:3:[0-9]+$"
    And the answer's "/processes/0/interval_ms" is 250
    And the answer's "/processes/0/due_in_us" is at least 1
    And the answer's text contains "every 250 ms, next in"

  Scenario: A Timer that fired once or was stopped wakes nothing
    Given the QML:
      """qml
      import QtQuick
      Item {
        Timer { id: tick; interval: 250; repeat: true; running: true }
        Timer { id: once; interval: 50; running: true; onTriggered: tick.stop() }
      }
      """
    And the inspector is connected
    When 500 milliseconds pass
    And the agent calls the tool "app_processes" with:
      """json
      {"kind": "waker"}
      """
    Then the answer's "/scheduled" is 0
    And the answer's "/processes" has 0 entries

  Scenario: A running animation is listed, a paused one is not
    Given the QML:
      """qml
      import QtQuick
      Item {
        NumberAnimation on x { id: slide; to: 100; duration: 1000; loops: Animation.Infinite }
        NumberAnimation on y { id: held; to: 100; duration: 1000; loops: Animation.Infinite }
        Component.onCompleted: held.pause()
      }
      """
    And the inspector is connected
    When the agent calls the tool "app_processes" with:
      """json
      {"kind": "waker"}
      """
    Then the answer's "/scheduled" is 1
    And the answer's "/processes/0/source" is "animation"
    And the answer's "/processes/0/locator" is "id=slide"

  Scenario: A file being read is an answer the application waits for, gone once it came
    Given the text file "feed.xml":
      """xml
      <rss><item><title>First</title></item></rss>
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        XmlListModel {
          id: feed; source: "feed.xml"; query: "/rss/item"
          XmlListModelRole { name: "title"; elementName: "title" }
        }
      }
      """
    And the inspector is connected
    When the agent calls the tool "app_processes" with:
      """json
      {"kind": "waker"}
      """
    Then the answer's "/scheduled" is at least 1
    And the answer's "/processes/0/source" is "answer"
    And the answer's "/processes/0/what" matches "^read file:.*feed\.xml$"
    And the answer's "/processes/0/locator" is "id=feed"
    And the answer's "/processes/0/at" matches "Scenario\.qml:4:[0-9]+$"
    When 500 milliseconds pass
    And the agent calls the tool "app_processes" with:
      """json
      {"kind": "waker"}
      """
    Then the answer's "/scheduled" is 0
