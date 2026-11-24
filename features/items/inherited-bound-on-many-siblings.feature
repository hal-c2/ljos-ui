# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#enabled-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
# Verified on Qt 6.12.0 (qml6, offscreen).

@qt-parity
Feature: Many siblings bound to one value pass it on side by side
  A palette, a list or a toolbar may hold hundreds of items whose `enabled`
  or `visible` binds to one value of the page, as `panel.target !== ""`.
  When that value moves, each item is told one after the other, not one
  inside the last: how many items there are costs no stack. An item whose
  bound result stays the same is not told it changed.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string target: "a"
        property int enabledChanges: 0; property int visibleChanges: 0
        property int lit: 0; property int shown: 0
        Component { id: tile
          Item {
            enabled: root.target !== ""
            visible: root.target !== "none"
            onEnabledChanged: root.enabledChanges++
            onVisibleChanged: root.visibleChanges++
            Item { property bool lit: enabled; property bool shown: visible }
          }
        }
        Component.onCompleted: { for (let i = 0; i < 400; i++) tile.createObject(root) }
        function count() {
          lit = 0; shown = 0
          for (let i = 0; i < children.length; i++) {
            if (children[i].children[0].lit) lit++
            if (children[i].children[0].shown) shown++
          }
        }
      }
      """

  Scenario: A move that keeps every item enabled tells none of them
    When the property "target" of "root" is set to "b"
    And the method "count" of "root" is called
    Then the property "enabledChanges" of "root" is 0
    And the property "lit" of "root" is 400
    And the log has no warnings

  Scenario: A move that disables every item tells each once
    When the property "target" of "root" is set to "b"
    And the property "target" of "root" is set to ""
    And the method "count" of "root" is called
    Then the property "enabledChanges" of "root" is 400
    And the property "lit" of "root" is 0
    And the log has no warnings

  Scenario: Enabling them again tells each once more
    When the property "target" of "root" is set to ""
    And the property "target" of "root" is set to "c"
    And the method "count" of "root" is called
    Then the property "enabledChanges" of "root" is 800
    And the property "lit" of "root" is 400
    And the log has no warnings

  Scenario: A move that hides every item tells each once
    When the property "target" of "root" is set to "b"
    And the property "target" of "root" is set to "none"
    And the method "count" of "root" is called
    Then the property "visibleChanges" of "root" is 400
    And the property "shown" of "root" is 0
    And the log has no warnings

  Scenario: Showing them again tells each once more
    When the property "target" of "root" is set to "none"
    And the property "target" of "root" is set to "d"
    And the method "count" of "root" is called
    Then the property "visibleChanges" of "root" is 800
    And the property "shown" of "root" is 400
    And the log has no warnings
