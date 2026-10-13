# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-stackview.html
# ARCHITECTURE.md §8.3: a page under the top one is frozen. Qt 6.11.2 only
# hides it, so its timers and animations go on doing work nobody sees.
# Ljós keeps the page's objects and values but stops that work; when the
# page is shown again a timer that came due fires once and an animation
# is where it would have been.

@qt-diverges
Feature: A StackView freezes the pages it covers
  A page under the top one is hidden and does no work: its timers do not
  fire. Shown again, it catches up once on what it missed.

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
            objectName: "covered"; property int ticks: 0
            Timer { interval: 100; repeat: true; running: true; onTriggered: parent.ticks++ }
          }
        }
        Component { id: top; Rectangle { color: "red" } }
        function cover() { stack.push(top, {}, StackView.Immediate) }
        function uncover() { stack.pop(StackView.Immediate) }
      }
      """

  @api-QtQuick.Controls.StackView.visible @api-QtQuick.Controls.StackView.push
  Scenario: The timer of a covered page waits until the page is shown again
    When 250 milliseconds pass
    Then the property "ticks" of "covered" is 2
    When the method "cover" of "root" is called
    And 1 second passes
    Then the property "ticks" of "covered" is 2
    When the method "uncover" of "root" is called
    And 0 milliseconds pass
    Then the property "ticks" of "covered" is 3
    When 100 milliseconds pass
    Then the property "ticks" of "covered" is 4
