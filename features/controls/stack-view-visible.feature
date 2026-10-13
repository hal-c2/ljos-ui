# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-stackview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: StackView.visible
  A stack view hides the pages under the top one. A page can say it is to
  be seen all the same, and taking that back leaves it to the view again.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 300; height: 200; property string seen: ""
        Component { id: note; Rectangle { } }
        StackView { id: stack; objectName: "stack"; anchors.fill: parent; initialItem: Rectangle { id: first; objectName: "first" } }
        function push() { stack.push(note, {}, StackView.Immediate) }
        function show() { first.StackView.visible = true }
        function forget() { first.StackView.visible = undefined; seen = first.visible + " " + first.StackView.visible }
        function forgetTop() { var t = stack.currentItem; t.StackView.visible = false; t.StackView.visible = undefined; seen = t.visible + " " + t.StackView.visible }
      }
      """

  @api-QtQuick.Controls.StackView.visible
  Scenario: A covered page that asks to be seen is seen
    When the method "push" of "root" is called
    And the method "show" of "root" is called
    Then the property "visible" of "first" is true

  Scenario: A covered page that takes its say back is hidden again
    When the method "push" of "root" is called
    And the method "show" of "root" is called
    And the method "forget" of "root" is called
    Then the property "seen" of "root" is "false false"

  Scenario: The top page that takes its say back is shown again
    When the method "push" of "root" is called
    And the method "forgetTop" of "root" is called
    Then the property "seen" of "root" is "true true"
