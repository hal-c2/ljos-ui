# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#enabled-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a child inherits is there as soon as its parent changes
  A child's visible and enabled follow its parent's at once: a script that
  changes the parent and reads the child in the same breath reads what the
  child inherits, not what it had.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        property int childVisibleChanges: 0
        property int childEnabledChanges: 0
        function note(s) { log += s + " " }
        function hide() { panel.visible = false; note(child.visible); note(inner.visible) }
        function show() { panel.visible = true; note(child.visible); note(inner.visible) }
        function disable() { panel.enabled = false; note(child.enabled); note(inner.enabled) }
        function enable() { panel.enabled = true; note(child.enabled); note(inner.enabled) }
        function moveUnder() { loose.parent = panel; note(loose.visible); note(loose.enabled) }
        function moveOut() { loose.parent = root; note(loose.visible); note(loose.enabled) }
        function giveOwn() { child.visible = true; note(child.visible) }
        Item {
          id: panel
          Item { id: child
            onVisibleChanged: root.childVisibleChanges++
            onEnabledChanged: root.childEnabledChanges++
            Item { id: inner }
          }
        }
        Item { id: loose }
      }
      """

  Scenario: A child reads hidden right after its parent is hidden
    When the method "hide" of "root" is called
    Then the property "log" of "root" is "false false "
    And the property "childVisibleChanges" of "root" is 1

  Scenario: A child reads shown right after its parent is shown again
    When the method "hide" of "root" is called
    And the method "show" of "root" is called
    Then the property "log" of "root" is "false false true true "
    And the property "childVisibleChanges" of "root" is 2

  Scenario: A child reads disabled right after its parent is disabled
    When the method "disable" of "root" is called
    And the method "enable" of "root" is called
    Then the property "log" of "root" is "false false true true "
    And the property "childEnabledChanges" of "root" is 2

  Scenario: An item moved under a hidden, disabled parent reads both at once
    When the method "hide" of "root" is called
    And the method "disable" of "root" is called
    And the property "log" of "root" is set to ""
    And the method "moveUnder" of "root" is called
    And the method "moveOut" of "root" is called
    Then the property "log" of "root" is "false false true true "

  Scenario: A child given visible under a hidden parent still reads hidden
    When the method "hide" of "root" is called
    And the property "log" of "root" is set to ""
    And the method "giveOwn" of "root" is called
    And the method "show" of "root" is called
    Then the property "log" of "root" is "false true true "
