# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#enabled-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What is enabled is told deepest first, and read as it stands then
  A change reaches every value below an item before any handler hears of it
  in that subtree, and handlers run from the deepest up, one subtree after the
  other. A binding on a value hears of its change only when that value's
  handler has run: until then a handler reads it as it was.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        function disable() { log = ""; a.enabled = false }
        function enable() { log = ""; a.enabled = true }
        Item {
          id: a
          property bool on: enabled
          onEnabledChanged: root.log += "a;"
          Item {
            id: b
            onEnabledChanged: root.log += "b;"
            Item {
              id: leaf
              property bool parentOn: parent.enabled
              onEnabledChanged: root.log += "leaf:" + a.on + parentOn + ";"
            }
          }
          Item {
            onEnabledChanged: root.log += "right:" + a.on + leaf.parentOn + ";"
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
