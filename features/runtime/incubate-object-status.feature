# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#incubateObject-method
#   docs/ARCHITECTURE.md §6.3 (time-sliced creation)

# Qt's offscreen platform finishes an asynchronous incubation at once, so
# the oracle never shows the Loading status and never calls
# onStatusChanged; Qt documents both.
@ljos-only
Feature: An incubator while its object is made
  An asynchronous incubation makes its object a little at a time, between
  other work. Until it is done the incubator says `Component.Loading`;
  then it calls the function assigned to its `onStatusChanged`.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var inc: null
        property int atCall: -1
        property bool emptyAtCall: false
        property string events: ""
        Component { id: comp; Item { objectName: "kid"; Component.onCompleted: root.events += "made " } }
        Component { id: needy; Item { required property int n } }
        function start(c) {
          inc = c.incubateObject(root)
          atCall = inc.status; emptyAtCall = inc.object === null
          inc.onStatusChanged = function(status) { root.events += "status" + status + " " }
        }
        function good() { start(comp) }
        function bad() { start(needy) }
        function forced() { start(comp); inc.forceCompletion(); events += "returned " }
      }
      """

  Scenario: It is still loading when incubateObject returns
    When the method "good" of "root" is called
    Then the property "atCall" of "root" is 2
    And the property "emptyAtCall" of "root" is true

  Scenario: onStatusChanged is called with Ready once the object is made
    When the method "good" of "root" is called
    Then the property "events" of "root" is "made status1 "
    And the object "root/kid" exists

  Scenario: onStatusChanged is called with Error when it cannot be made
    When the method "bad" of "root" is called
    Then the property "events" of "root" is "status3 "

  Scenario: forceCompletion calls onStatusChanged before it returns
    When the method "forced" of "root" is called
    Then the property "events" of "root" is "made status1 returned "

  Scenario: onStatusChanged is called once
    When the method "forced" of "root" is called
    And 1 second passes
    Then the property "events" of "root" is "made status1 returned "
