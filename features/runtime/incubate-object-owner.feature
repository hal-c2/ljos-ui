# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#incubateObject-method
#   https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Who an incubator belongs to
  The incubator `incubateObject` returns belongs to the script that holds
  it, not to the component it came from: it can still be asked about its
  object when the component is gone.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var inc: null
        property var maker: null
        property var kept: null
        property string seen: ""
        Component {
          id: makers
          QtObject { property Component kid: Component { Item { objectName: "kid" } } }
        }
        function start() {
          maker = makers.createObject(root)
          inc = maker.kid.incubateObject(root)
          inc.forceCompletion()
          kept = function(status) {}
          inc.onStatusChanged = kept
        }
        function drop() { maker.destroy() }
        function look() { seen = (maker === null) + "|" + inc.status + "|" + (inc.object !== null) }
        function same() { seen = (inc === inc) + "|" + (inc.onStatusChanged === kept) }
      }
      """

  Scenario: The incubator is there while its component is
    When the method "start" of "root" is called
    And the method "look" of "root" is called
    Then the property "seen" of "root" is "false|1|true"

  Scenario: The incubator outlives the component it came from
    When the method "start" of "root" is called
    And the method "drop" of "root" is called
    And 1 second passes
    And the method "look" of "root" is called
    Then the property "seen" of "root" is "true|1|true"
    And the object "root/kid" exists

  Scenario: The incubator is one object and keeps the function it was given
    When the method "start" of "root" is called
    And the method "drop" of "root" is called
    And 1 second passes
    And the method "same" of "root" is called
    Then the property "seen" of "root" is "true|true"
    And the log has no warnings
