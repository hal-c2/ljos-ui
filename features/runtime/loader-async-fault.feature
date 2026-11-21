# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#status-prop
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#asynchronous-prop
#   docs/ARCHITECTURE.md §6.3 (time-sliced creation)
#   docs/ARCHITECTURE.md §9.4 (Supervisor: UI boundaries, Loader)

# Qt has no time limit on JavaScript, and its offscreen platform finishes an asynchronous item at
# once, so the oracle shows neither the fault nor the Loading status.
@ljos-only
Feature: An asynchronous Loader says its item failed in the frame it failed
  An asynchronous Loader makes its item in the frame's incubation step. When a script under the
  item fails there, the Loader's `status` changes in that same frame, as a plain Loader's does:
  no frame shows an item that has already failed. Each try that fails is followed by
  `Loader.Error` before anything the item's `Loader.Ready` queued for later runs.

  Background:
    Given JavaScript is timed by a clock on which native work a script asks for takes 0 seconds
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property bool wanted: false
        property int tries: 0
        property int errors: 0
        property int late: 0
        property string events: ""
        function check() { if (root.errors < root.tries) root.late += 1 }
        Component {
          id: page
          Item { objectName: "page"; Component.onCompleted: { root.tries += 1; for (;;) { } } }
        }
        Loader {
          id: loader; objectName: "loader"; asynchronous: true
          active: root.wanted; sourceComponent: page
          onStatusChanged: {
            root.events += "status" + status + " "
            if (status === Loader.Error) root.errors += 1
            if (status === Loader.Ready) Qt.callLater(root.check)
          }
        }
      }
      """

  Scenario: Each interrupted try is an error in the frame it was interrupted
    When the property "wanted" of "root" is set to true
    Then the property "tries" of "root" is 2
    And the property "errors" of "root" is 2
    And the property "late" of "root" is 0
    And the property "events" of "root" is "status2 status1 status3 status2 status1 status3 "
    And the property "status" of "loader" is 3
    And the property "item" of "loader" is null
