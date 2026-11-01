# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#enabled-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What is inherited is told deepest first, and read as it stands then
  A change reaches every value below an item before any handler hears of it
  in that subtree, and handlers run from the deepest up, one subtree after the
  other. A binding on a value hears of its change only when that value's
  handler has run: until then a handler reads it as it was.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        function clear() { log = "" }
        function hide() { clear(); a.visible = false }
        function show() { clear(); a.visible = true }
        function disable() { clear(); a.enabled = false }
        function enable() { clear(); a.enabled = true }
        function note(s) { log += s + ";" }
        Item {
          id: a
          property bool on: enabled
          onEnabledChanged: root.note("a")
          onVisibleChanged: root.note("a:" + right.visible + b.visible)
          Item {
            id: b
            property bool shown: visible
            onEnabledChanged: root.note("b")
            onVisibleChanged: root.note("b:" + parent.visible + leaf.visible + right.visible)
            Item {
              id: leaf
              property bool parentOn: parent.enabled
              onEnabledChanged: root.note("leaf:" + a.on + parentOn)
              onVisibleChanged: root.note("leaf:" + parent.visible + right.visible + b.shown)
            }
          }
          Item {
            id: right
            onEnabledChanged: root.note("right:" + a.on + leaf.parentOn)
            onVisibleChanged: root.note("right:" + b.shown + a.visible)
          }
        }
      }
      """

  Scenario: Enabled is told deepest first, one subtree after the other
    When the method "disable" of "root" is called
    Then the property "log" of "root" is "leaf:truetrue;b;right:truefalse;a;"

  Scenario: Enabling again is told in the same order
    When the method "disable" of "root" is called
    And the method "enable" of "root" is called
    Then the property "log" of "root" is "leaf:falsefalse;b;right:falsetrue;a;"

  Scenario: A handler reads an ancestor changed, a later subtree not yet, and a binding on a value not yet told as it was
    When the method "hide" of "root" is called
    Then the property "log" of "root" is "leaf:falsetruetrue;b:falsefalsetrue;right:falsefalse;a:falsefalse;"

  Scenario: Showing again reads the same way
    When the method "hide" of "root" is called
    And the method "show" of "root" is called
    Then the property "log" of "root" is "leaf:truefalsefalse;b:truetruefalse;right:truetrue;a:truetrue;"
