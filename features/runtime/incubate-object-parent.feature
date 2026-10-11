# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#incubateObject-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#status-prop
#   docs/ARCHITECTURE.md §6.3 (time-sliced creation)

# Qt's offscreen platform finishes an asynchronous incubation at once, so
# on the oracle the parent can never go while the object is being made.
@ljos-only
Feature: An incubation whose parent goes away
  An asynchronous incubation makes its object over several frames. When
  the parent it was to have is destroyed in between, there is nowhere to
  put the object: the incubator ends in `Component.Error` with no object.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var inc: null
        property var home: null
        property string events: ""
        property string seen: ""
        Component { id: homes; Item { objectName: "home" } }
        Component { id: comp; Item { objectName: "kid"; Component.onCompleted: root.events += "made " } }
        function start() {
          home = homes.createObject(root)
          inc = comp.incubateObject(home)
          inc.onStatusChanged = function(status) { root.events += "status" + status + " " }
        }
        function gone() { start(); home.destroy() }
        function look() { seen = inc.status + "|" + (inc.object === null) }
      }
      """

  Scenario: With its parent alive the object is made
    When the method "start" of "root" is called
    And the method "look" of "root" is called
    Then the property "events" of "root" is "made status1 "
    And the property "seen" of "root" is "1|false"

  Scenario: The incubator ends in Error when its parent is destroyed
    When the method "gone" of "root" is called
    And the method "look" of "root" is called
    Then the property "seen" of "root" is "3|true"
    And the object "kid" does not exist

  Scenario: onStatusChanged is called with Error, once
    When the method "gone" of "root" is called
    And 1 second passes
    Then the property "events" of "root" is "status3 "
