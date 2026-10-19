# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-stackview.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# ARCHITECTURE.md §8.3: Ljós freezes the page a StackView covers, and with
# it what the page owns. A page declared inside it and pushed over it is
# shown, so it must stay awake.

@qt-parity
Feature: A page pushed over the page that holds it is awake
  A page can be declared inside another and pushed over it. The page under
  it is covered, but the page on top is shown: its timers fire and it
  answers the pointer.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        StackView {
          id: stack; objectName: "stack"; anchors.fill: parent
          initialItem: Item {
            id: home; objectName: "home"
            property int ticks: 0
            Timer { interval: 100; repeat: true; running: true; onTriggered: home.ticks++ }
            Item {
              id: inner; objectName: "inner"; visible: false
              property int ticks: 0
              Timer { interval: 100; repeat: true; running: true; onTriggered: inner.ticks++ }
            }
          }
        }
        Component { id: other; Item {} }
        function open() { stack.push(inner, {}, StackView.Immediate) }
        function cover() { stack.push(other, {}, StackView.Immediate) }
        function back() { stack.pop(StackView.Immediate) }
      }
      """

  @api-QtQuick.Controls.StackView.push
  Scenario: The timer of a page pushed over its holder fires
    When the method "open" of "root" is called
    And the property "ticks" of "inner" is set to 0
    And 450 milliseconds pass
    Then the property "ticks" of "inner" is 4

  @api-QtQuick.Controls.StackView.pop
  Scenario: Covered and shown again, the page wakes
    When the method "open" of "root" is called
    And the method "cover" of "root" is called
    And the method "back" of "root" is called
    And the property "ticks" of "inner" is set to 0
    And 450 milliseconds pass
    Then the property "ticks" of "inner" is 4
