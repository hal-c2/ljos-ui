# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#enabled-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An item's own visible or enabled written while its parent is off
  A hidden parent makes its children read not visible, and a disabled one
  makes them read not enabled. Writing `true` to such a child, from its
  own change handler, a binding or anywhere else, sets its own value: it
  still reads false, nothing is told of a change, and it reads true once
  the parent is on again.

  Scenario: A child that shows itself when its parent hides is told once
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int told: 0
        Item {
          objectName: "parent"
          Item {
            objectName: "child"
            onVisibleChanged: { root.told++; visible = true }
          }
        }
      }
      """
    When the property "visible" of "parent" is set to false
    Then the property "visible" of "child" is false
    And the property "told" of "root" is 1
    And the log has no warnings
    When the property "visible" of "parent" is set to true
    Then the property "visible" of "child" is true
    And the property "told" of "root" is 2
    And the log has no warnings

  Scenario: A child that enables itself when its parent is disabled is told once
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int told: 0
        Item {
          objectName: "parent"
          Item {
            objectName: "child"
            onEnabledChanged: { root.told++; enabled = true }
          }
        }
      }
      """
    When the property "enabled" of "parent" is set to false
    Then the property "enabled" of "child" is false
    And the property "told" of "root" is 1
    And the log has no warnings
    When the property "enabled" of "parent" is set to true
    Then the property "enabled" of "child" is true
    And the property "told" of "root" is 2
    And the log has no warnings

  Scenario: A child shown by a later write while its parent is hidden stays hidden until the parent shows
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int told: 0
        function show() { child.visible = true }
        Item {
          objectName: "parent"
          visible: false
          Item {
            id: child
            objectName: "child"
            visible: false
            onVisibleChanged: root.told++
          }
        }
      }
      """
    When the method "show" of "root" is called
    Then the property "visible" of "child" is false
    And the property "told" of "root" is 0
    When the property "visible" of "parent" is set to true
    Then the property "visible" of "child" is true
    And the property "told" of "root" is 1
    And the log has no warnings

  Scenario: A child whose binding turns true while its parent is hidden stays hidden until the parent shows
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool wanted: false
        property int told: 0
        Item {
          objectName: "parent"
          visible: false
          Item {
            objectName: "child"
            visible: root.wanted
            onVisibleChanged: root.told++
          }
        }
      }
      """
    When the property "wanted" of "root" is set to true
    Then the property "visible" of "child" is false
    And the property "told" of "root" is 1
    When the property "visible" of "parent" is set to true
    Then the property "visible" of "child" is true
    And the property "told" of "root" is 2
    And the log has no warnings

  Scenario: A child enabled by a later write while its parent is disabled stays disabled until the parent is enabled
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int told: 0
        function enable() { child.enabled = true }
        Item {
          objectName: "parent"
          enabled: false
          Item {
            id: child
            objectName: "child"
            enabled: false
            onEnabledChanged: root.told++
          }
        }
      }
      """
    When the method "enable" of "root" is called
    Then the property "enabled" of "child" is false
    And the property "told" of "root" is 0
    When the property "enabled" of "parent" is set to true
    Then the property "enabled" of "child" is true
    And the property "told" of "root" is 1
    And the log has no warnings
