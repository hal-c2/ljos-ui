# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html#deleting-objects-dynamically
#   https://doc.qt.io/qt-6/qtqml-documents-scope.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: destroy() called by its bare name
  A script may call destroy() without saying whose, as it calls the
  object's own methods: it is the object the script belongs to. A
  function a type declares does it for every object of a type built on
  that one, and a delay given to it is kept.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string events: ""
        component Base: Item {
          function dieLater() { destroy(50) }
          function dieNow() { destroy() }
        }
        component Built: Base {
          property string name
          function die() { dieLater() }
          Component.onDestruction: root.events += name + " "
          Item {
            objectName: "inner"
            function drop() { destroy() }
            Component.onDestruction: root.events += "inner "
          }
        }
        Component { id: comp; Built { } }
        property var made: comp.createObject(root, { name: "made", objectName: "made" })
        function die() { made.die() }
        function dieNow() { made.dieNow() }
        function dropInner() { made.children[0].drop() }
      }
      """

  Scenario: A function of the base type destroys the object after its delay
    When the method "die" of "root" is called
    And 10 milliseconds pass
    Then the property "events" of "root" is ""
    When 100 milliseconds pass
    Then the property "events" of "root" is "inner made "

  Scenario: Without a delay the object goes once the script is done
    When the method "dieNow" of "root" is called
    And 10 milliseconds pass
    Then the property "events" of "root" is "inner made "

  Scenario: The object destroyed is the one whose script calls it
    When the method "dropInner" of "root" is called
    And 10 milliseconds pass
    Then the object "root/inner" does not exist
    And the object "root/made" exists
