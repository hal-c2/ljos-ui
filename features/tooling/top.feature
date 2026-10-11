# Sources:
#   docs/ARCHITECTURE.md sections 3.4, 9.1 and 19.10
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#asynchronous-prop
# The top overlay is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: The top overlay says where a frame's time went
  A development build can show, over the application, how long its last
  frame took, the time by lane, and who was charged for the work.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        Component { id: part; Rectangle { objectName: "part"; width: 30; height: 30 } }
        Loader {
          id: loader; objectName: "loader"
          asynchronous: true; active: false; sourceComponent: part
        }
        function build() { loader.active = true }
      }
      """

  Rule: It is there only when it is asked for

    Scenario: Nothing is shown until it is asked for
      Then the top overlay shows nothing

    Scenario: Shown before the application ran a frame, it says so
      When the top overlay is shown
      Then the top overlay's title is "no frame yet"

    Scenario: Hidden again, it shows nothing and keeps nothing
      When the top overlay is shown
      And the method "build" of "root" is called
      And the top overlay is hidden
      Then the top overlay shows nothing
      When the top overlay is shown
      Then the top overlay's title is "no frame yet"
      And the top overlay has charged nobody

  Rule: It shows the frames the application ran anyway

    Scenario: Every lane is listed, the most urgent first
      When the top overlay is shown
      Then the top overlay lists the lanes "input, critical, visible, prefetch, deferred, idle"

    Scenario: A frame is counted and timed
      When the top overlay is shown
      And the method "build" of "root" is called
      Then the top overlay's title matches "^frame \d+  \d+\.\d\d ms  \(max \d+\.\d\d ms\)$"

    Scenario: The component whose work a frame did is named
      When the top overlay is shown
      And the method "build" of "root" is called
      Then the object "loader/part" exists
      And the top overlay has charged "Loader \"loader\""

    Scenario: A frame without scheduled work charges nobody
      When the top overlay is shown
      And the property "width" of "root" is set to 300
      Then the top overlay's title matches "^frame \d+ "
      And the top overlay has charged nobody

    Scenario: Work done while it was hidden is not counted
      When the method "build" of "root" is called
      And the top overlay is shown
      And the property "width" of "root" is set to 300
      Then the top overlay has charged nobody

  Rule: It never keeps the application awake

    Scenario: Shown over an application at rest, it asks for no frame
      When the top overlay is shown
      Then the top overlay keeps no frame coming

    Scenario: When the work it showed is done, it asks for no frame
      When the top overlay is shown
      And the method "build" of "root" is called
      Then the top overlay keeps no frame coming
