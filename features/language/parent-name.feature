# Sources:
#   https://doc.qt.io/qt-6/qtqml-documents-scope.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#parent-prop
#   https://doc.qt.io/qt-6/qml-qtqml-timer.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The name parent in objects that are not items
  A Timer has a parent property of its own: the object it is declared in. A QtObject has none, so
  inside one the name is found further out, on the root object of the file.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string timerParent: timer.parent.objectName
        Item {
          id: panel
          objectName: "panel"
          property int step: 3
          property int ticks: 0
          Timer {
            id: timer
            objectName: "timer"
            property int twice: parent.step * 2
            interval: 50; running: true
            onTriggered: parent.ticks += parent.step
          }
          QtObject {
            objectName: "plain"
            property bool rootsParent: parent === root.parent
            property bool enclosing: parent === panel
          }
        }
      }
      """

  Scenario: A binding in a Timer reads the enclosing object through parent
    Then the property "twice" of "timer" is 6

  Scenario: A signal handler in a Timer writes the enclosing object through parent
    When 100 milliseconds pass
    Then the property "ticks" of "panel" is 3

  Scenario: The parent of a Timer can be read from outside it
    Then the property "timerParent" of "root" is "panel"

  Scenario: The binding of a Timer follows the enclosing object
    When the property "step" of "panel" is set to 5
    Then the property "twice" of "timer" is 10

  Scenario: Inside a QtObject, parent is the parent of the file's root object
    Then the property "rootsParent" of "plain" is true
    And the property "enclosing" of "plain" is false
