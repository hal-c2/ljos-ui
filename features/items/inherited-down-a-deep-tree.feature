# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#enabled-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a child inherits reaches the bottom of a deep tree
  An item tree may be many levels deep, as one of pages, panes, layouts and
  controls inside each other is. Hiding or disabling its top reaches every
  level, and each level's bindings and handlers run as they would near the
  top: how far down they are costs them nothing.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var first; property var leaf
        property int enabledChanges: 0; property int visibleChanges: 0; property int lit: 0
        property string log: ""
        Component { id: level
          Item {
            property bool lit: enabled
            onEnabledChanged: root.enabledChanges++
            onVisibleChanged: root.visibleChanges++
          }
        }
        Component.onCompleted: {
          first = level.createObject(root); leaf = first
          for (let i = 0; i < 99; i++) leaf = level.createObject(leaf)
        }
        function countLit() { lit = 0; for (let i = first; i; i = i.children[0]) if (i.lit) lit++ }
        function disable() { first.enabled = false; log += leaf.enabled + " "; countLit() }
        function enable() { first.enabled = true; log += leaf.enabled + " "; countLit() }
        function hide() { first.visible = false; log += leaf.visible + " " }
        function show() { first.visible = true; log += leaf.visible + " " }
      }
      """

  Scenario: Disabling the top reaches the bottom at once
    When the method "disable" of "root" is called
    Then the property "log" of "root" is "false "
    And the property "enabledChanges" of "root" is 100

  Scenario: Enabling the top again reaches the bottom at once
    When the method "disable" of "root" is called
    And the method "enable" of "root" is called
    Then the property "log" of "root" is "false true "
    And the property "enabledChanges" of "root" is 200

  Scenario: Hiding the top reaches the bottom at once
    When the method "hide" of "root" is called
    Then the property "log" of "root" is "false "
    And the property "visibleChanges" of "root" is 100

  Scenario: Showing the top again reaches the bottom at once
    When the method "hide" of "root" is called
    And the method "show" of "root" is called
    Then the property "log" of "root" is "false true "
    And the property "visibleChanges" of "root" is 200

  Scenario: A binding at every level sees the top disabled
    When the method "disable" of "root" is called
    Then the property "lit" of "root" is 0

  Scenario: A binding at every level sees the top enabled again
    When the method "disable" of "root" is called
    And the method "enable" of "root" is called
    Then the property "lit" of "root" is 100
