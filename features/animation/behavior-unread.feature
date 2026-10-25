# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A Behavior on a property nothing reads
  A Behavior animates each new value of its property, whether or not
  anything shows or reads that property: one of an object's own, or the
  x of a hidden rectangle, animates as a shown one does.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 200; height: 200
        property bool on: false
        property string own: ""
        property string hidden: ""
        Item {
          property real v: root.on ? 100 : 0
          Behavior on v {
            NumberAnimation { duration: 100; onRunningChanged: root.own += running + " " }
          }
        }
        Rectangle {
          visible: false; width: 10; height: 10
          x: root.on ? 100 : 0
          Behavior on x {
            NumberAnimation { duration: 100; onRunningChanged: root.hidden += running + " " }
          }
        }
      }
      """

  Scenario: The Behaviors run when the binding they follow changes
    When the property "on" of "root" is set to true
    And 500 milliseconds pass
    Then the property "own" of "root" is "true false "
    And the property "hidden" of "root" is "true false "

  # An application works a binding out when something reads it; here the
  # Behaviors run in one.
  @desktop
  Scenario: The Behaviors run in an application as in a test
    When the property "on" of "root" is set to true
    And 500 milliseconds pass
    Then the property "own" of "root" is "true false "
    And the property "hidden" of "root" is "true false "
