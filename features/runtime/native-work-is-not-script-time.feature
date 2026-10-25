# Sources:
#   docs/ARCHITECTURE.md §6.1 (a hard limit on one run of JavaScript)
#   docs/ARCHITECTURE.md §9.3 (faults: JsInterrupted)
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#createComponent-method

# Qt has no time limit on JavaScript to compare with.
@ljos-only
Feature: Native work a script asks for is not the script's time
  A handler that compiles a component and makes an object of it waits on native work, which can
  take longer than the time limit on the handler's JavaScript. That time does not count against
  the handler, and the new object's `Component.onCompleted` runs with a limit of its own. A
  script that never returns is still stopped, and the warning says which handler it was and
  where the interrupt came.

  Background:
    Given JavaScript is timed by a clock on which native work a script asks for takes 6 seconds
    And the file "Late.qml":
      """qml
      import QtQuick
      Item {
        objectName: "page"
        Component.onCompleted: { let s = 0; for (let i = 0; i < 1500000; i++) s += i; parent.completed += 1 }
      }
      """
    And the file "Spin.qml":
      """qml
      import QtQuick
      Item {
        objectName: "spin"
        Component.onCompleted: { for (;;) { } }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int completed: 0
        property string made: ""
        property string url: ""
        property int copies: 1
        onUrlChanged: make(url)
        function make(url) {
          const c = Qt.createComponent(url)
          for (let i = 0; i < copies; i++) made += c.createObject(root).objectName
        }
      }
      """

  Scenario: The new object's Component.onCompleted and the rest of the handler run
    When the property "url" of "root" is set to "Late.qml"
    Then the property "completed" of "root" is 1
    And the property "made" of "root" is "page"
    And the log has no warning matching "interrupted"

  # Each takes half the limit: together they are more than the handler's.
  Scenario: Each new object's Component.onCompleted has a time limit of its own
    When the property "copies" of "root" is set to 3
    And the property "url" of "root" is set to "Late.qml"
    Then the property "completed" of "root" is 3
    And the property "made" of "root" is "pagepagepage"

  Scenario: A Component.onCompleted that never returns stops the handler, which is named
    When the property "url" of "root" is set to "Spin.qml"
    Then the property "made" of "root" is ""
    And the application fault is a "JsInterrupted" matching "interrupted"
    And the log has a warning matching "interrupted after .* at [^ ]*\.qml:\d+:\d+ \(onUrlChanged\), in .signal handler. at [^ ]*Spin\.qml:\d+:\d+ \(Component\.onCompleted\)"
