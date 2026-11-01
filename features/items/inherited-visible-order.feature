# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What is visible is told deepest first, and read as it stands then
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
        function hide() { log = ""; a.visible = false }
        function show() { log = ""; a.visible = true }
        Item {
          id: a
          onVisibleChanged: root.log += "a:" + right.visible + b.visible + ";"
          Item {
            id: b
            property bool shown: visible
            onVisibleChanged: root.log += "b:" + parent.visible + leaf.visible + right.visible + ";"
            Item {
              id: leaf
              onVisibleChanged: root.log += "leaf:" + parent.visible + right.visible + b.shown + ";"
            }
          }
          Item {
            id: right
            onVisibleChanged: root.log += "right:" + b.shown + a.visible + ";"
          }
        }
      }
      """

  Scenario: A handler reads an ancestor changed, a later subtree not yet, and a binding on a value not yet told as it was
    When the method "hide" of "root" is called
    Then the property "log" of "root" is "leaf:falsetruetrue;b:falsefalsetrue;right:falsefalse;a:falsefalse;"

  Scenario: Showing again reads the same way
    When the method "hide" of "root" is called
    And the method "show" of "root" is called
    Then the property "log" of "root" is "leaf:truefalsefalse;b:truetruefalse;right:truetrue;a:truetrue;"
