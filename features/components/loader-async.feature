# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#asynchronous-prop
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#status-prop
#   docs/ARCHITECTURE.md §6.3 (time-sliced creation)

# Qt's offscreen platform finishes an asynchronous item at once, so the
# oracle never shows the Loading status; Qt documents it.
@ljos-only
Feature: An asynchronous Loader while it loads
  An asynchronous Loader makes its item after the file around it is
  complete, and reports Loading until then.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int atStart: -1
        property bool emptyAtStart: false
        property string events: ""
        Component { id: comp; Item { objectName: "kid"; width: 30 } }
        Loader {
          id: loader; objectName: "loader"; asynchronous: true; sourceComponent: comp
          onStatusChanged: root.events += "status" + status + " "
          onLoaded: root.events += "loaded "
        }
        Component.onCompleted: { atStart = loader.status; emptyAtStart = loader.item === null }
        function restart() { loader.active = false; loader.active = true; loader.active = false }
      }
      """

  Scenario: It is still loading when the file around it completes
    Then the property "atStart" of "root" is 2
    And the property "emptyAtStart" of "root" is true

  Scenario: It says Loading, then Ready, then loaded
    Then the property "events" of "root" is "status2 status1 loaded "
    And the object "loader/kid" exists

  Scenario: Switching it off while it loads makes nothing
    When the method "restart" of "root" is called
    Then the object "kid" does not exist
    And the property "status" of "loader" is 0
    And the property "item" of "loader" is null
    And the property "events" of "root" is "status2 status1 loaded status0 status2 status0 "
