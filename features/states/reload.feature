# Sources:
#   docs/ARCHITECTURE.md (section 12.1)
#   https://doc.qt.io/qt-6/qml-qtquick-state.html
# Hot reload is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: A reload while a state is active
  The state an item is in is something the running application gave it, so
  it survives a reload. What the state changes comes from the new file, and
  so does what the item goes back to when it leaves the state.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool wide: false
        width: 100
        height: 40
        states: [
          State { name: "big"; PropertyChanges { target: root; width: 200 } },
          State { name: "tall"; when: root.wide; PropertyChanges { target: root; height: 80 } }
        ]
      }
      """

  Scenario: The item is still in the state it was given
    When the property "state" of "root" is set to "big"
    And the file "Scenario.qml" is changed to:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool wide: false
        width: 100
        height: 41
        states: [
          State { name: "big"; PropertyChanges { target: root; width: 200 } },
          State { name: "tall"; when: root.wide; PropertyChanges { target: root; height: 80 } }
        ]
      }
      """
    And a reload happens
    Then the property "state" of "root" is "big"
    And the property "width" of "root" is 200

  Scenario: The state's values come from the new file
    When the property "state" of "root" is set to "big"
    And the file "Scenario.qml" is changed to:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool wide: false
        width: 100
        height: 40
        states: [
          State { name: "big"; PropertyChanges { target: root; width: 300 } },
          State { name: "tall"; when: root.wide; PropertyChanges { target: root; height: 80 } }
        ]
      }
      """
    And a reload happens
    Then the property "width" of "root" is 300

  Scenario: Leaving the state after a reload gives the new file's value
    When the property "state" of "root" is set to "big"
    And the file "Scenario.qml" is changed to:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool wide: false
        width: 120
        height: 40
        states: [
          State { name: "big"; PropertyChanges { target: root; width: 200 } },
          State { name: "tall"; when: root.wide; PropertyChanges { target: root; height: 80 } }
        ]
      }
      """
    And a reload happens
    And the property "state" of "root" is set to ""
    Then the property "width" of "root" is 120

  Scenario: A state held by its condition is still held
    When the property "wide" of "root" is set to true
    And the file "Scenario.qml" is changed to:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool wide: false
        width: 100
        height: 40
        states: [
          State { name: "big"; PropertyChanges { target: root; width: 200 } },
          State { name: "tall"; when: root.wide; PropertyChanges { target: root; height: 90 } }
        ]
      }
      """
    And a reload happens
    Then the property "state" of "root" is "tall"
    And the property "height" of "root" is 90

  Scenario: A state the new file no longer has changes nothing
    When the property "state" of "root" is set to "big"
    And the file "Scenario.qml" is changed to:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool wide: false
        width: 100
        height: 40
        states: [
          State { name: "tall"; when: root.wide; PropertyChanges { target: root; height: 80 } }
        ]
      }
      """
    And a reload happens
    Then the property "width" of "root" is 100

  Scenario: A value written while the state held the property is not what the item goes back to
    When the property "state" of "root" is set to "big"
    And the property "width" of "root" is set to 250
    And the file "Scenario.qml" is changed to:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool wide: false
        width: 100
        height: 41
        states: [
          State { name: "big"; PropertyChanges { target: root; width: 200 } },
          State { name: "tall"; when: root.wide; PropertyChanges { target: root; height: 80 } }
        ]
      }
      """
    And a reload happens
    And the property "state" of "root" is set to ""
    Then the property "width" of "root" is 100
