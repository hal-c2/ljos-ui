# Sources:
#   docs/ARCHITECTURE.md sections 12.1, 19.4 and 19.9
#   docs/research/10-mcp.md
#   https://doc.qt.io/qt-6/qml-qtqml-qtobject.html#destroy-method
# The inspector and hot reload are Ljós extensions: Qt has nothing to compare
# them with.

@ljos-only @headless
Feature: Inspector references
  A reference names one item of one snapshot. When the item is gone the
  agent is told so, and is never handed another item in its place.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        function drop() { children[0].destroy() }
        Rectangle {
          objectName: "save"
          width: 80; height: 30
          Accessible.role: Accessible.Button
          Accessible.name: "Save"
        }
        Rectangle {
          objectName: "other"
          y: 40; width: 80; height: 30
          Accessible.role: Accessible.Button
          Accessible.name: "Other"
        }
      }
      """
    And the inspector is connected
    And the agent has requested a snapshot

  Scenario: A reference leads back to its item
    When the agent inspects the reference of the "button" named "Save"
    Then the answer's "/object_name" is "save"

  Scenario: A reference to a destroyed item is stale
    When the method "drop" of "root" is called
    And the agent inspects the reference of the "button" named "Save"
    Then the agent is told the reference is stale
    And the refusal mentions "app_snapshot"

  Scenario: A reference to an item that is no longer shown is stale
    When the property "visible" of "save" is set to false
    And the agent inspects the reference of the "button" named "Save"
    Then the agent is told the reference is stale

  Scenario: A reference the inspector never gave is stale
    When the agent inspects "ref=e99"
    Then the agent is told the reference is stale

  Scenario: A reference survives a change that keeps the item
    When the property "width" of "save" is set to 120
    And the agent inspects the reference of the "button" named "Save"
    Then the answer's "/geometry/width" is 120

  Scenario: A locator survives a reload that makes the item anew
    When the file "Scenario.qml" is changed to:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        Rectangle {
          objectName: "save"
          width: 95; height: 30
          Accessible.role: Accessible.Button
          Accessible.name: "Save"
        }
      }
      """
    And a reload happens
    And the agent inspects "role=button[name=\"Save\"]"
    Then the answer's "/geometry/width" is 95

  Scenario: An item says which object of which file it is
    When the file "Scenario.qml" is changed to:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Rectangle {
          objectName: "save"
          Accessible.role: Accessible.Button
          Accessible.name: "Save"
        }
      }
      """
    And a reload happens
    And the agent inspects "#save"
    Then the answer's "/key" matches "'save'$"
