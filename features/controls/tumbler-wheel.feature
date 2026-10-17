# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tumbler.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#wheelEnabled-prop
# The Qt oracle cannot send wheel events (see items/flickable-wheel.feature),
# so these scenarios run on Ljós only. What the wheel does was measured on
# Qt 6.11.2 with Qt Test's mouseWheel.

@ljos-only
Feature: The wheel over a Tumbler
  The wheel does not turn a tumbler, whether it wraps or not. A tumbler
  that takes the wheel keeps it; one that does not, as is the default,
  leaves it to what it is in, so a page of tumblers still scrolls.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 300
        Flickable {
          objectName: "page"; width: 200; height: 300; contentHeight: 1000
          Tumbler { objectName: "t"; model: 10 }
          Tumbler { objectName: "s"; x: 100; model: 3 }
        }
        Flickable {
          objectName: "keeps"; x: 200; width: 200; height: 300; contentHeight: 1000
          Tumbler { objectName: "w"; model: 10; wheelEnabled: true }
        }
      }
      """

  Scenario Outline: A notch <direction> over a tumbler of <rows> rows does not turn it
    When the user turns the wheel 1 notch <direction> at <x>, 100
    And 1 second passes
    Then the property "currentIndex" of "<tumbler>" is 0

    Examples:
      | tumbler | rows | direction | x   |
      | t       | 10   | down      | 30  |
      | t       | 10   | up        | 30  |
      | s       | 3    | down      | 130 |
      | w       | 10   | down      | 230 |

  @api-QtQuick.Controls.Control.wheelEnabled
  Scenario: The wheel a tumbler does not take scrolls what it is in
    When the user turns the wheel 1 notch down at 30, 100
    And 1 second passes
    Then the property "contentY" of "page" is 72

  @api-QtQuick.Controls.Control.wheelEnabled
  Scenario: A tumbler that takes the wheel keeps it from what it is in
    When the user turns the wheel 1 notch down at 230, 100
    And 1 second passes
    Then the property "contentY" of "keeps" is 0
