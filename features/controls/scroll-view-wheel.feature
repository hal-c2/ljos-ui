# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollview.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#wheelEnabled-prop
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
# The Qt oracle cannot send wheel events (see items/flickable-wheel.feature),
# so these scenarios run on Ljós only. The distances and who gets the wheel
# were measured on Qt 6.11.2 with Qt Test's mouseWheel. A notch glides there,
# as items/flickable-wheel.feature says, so the scenarios wait for it.

@ljos-only
Feature: Scrolling a ScrollView with the wheel
  The wheel scrolls the view under the pointer, as it scrolls a Flickable,
  for as long as the view takes the wheel.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 420; height: 320
        ScrollView {
          objectName: "view"
          width: 200; height: 100
          Rectangle { implicitWidth: 500; implicitHeight: 1000 }
        }
        ScrollView {
          objectName: "deaf"
          y: 100; width: 200; height: 100; wheelEnabled: false
          Rectangle { implicitWidth: 500; implicitHeight: 1000 }
        }
        Flickable {
          objectName: "outer"
          x: 200; width: 200; height: 300
          contentWidth: 200; contentHeight: 2000
          ScrollView {
            objectName: "inner"
            width: 200; height: 100; wheelEnabled: false
            Rectangle { implicitWidth: 200; implicitHeight: 1000 }
          }
        }
      }
      """

  Scenario Outline: A notch of the wheel scrolls a view as far as a Flickable
    When the user turns the wheel <notches> notches <direction> at 100, 50
    And 400 milliseconds pass
    Then the property "contentItem.contentX" of "view" is <x>
    And the property "contentItem.contentY" of "view" is <y>

    Examples:
      | notches | direction | x  | y   |
      | 1       | down      | 0  | 72  |
      | 3       | down      | 0  | 216 |
      | 1       | right     | 72 | 0   |
      | 1       | up        | 0  | 0   |

  Scenario: The bars follow the wheel
    When the user turns the wheel 1 notch down at 100, 50
    And 400 milliseconds pass
    Then the property "ScrollBar.vertical.position" of "view" is within 0.0001 of 0.072

  Scenario: The wheel scrolls the view from over one of its bars
    When the user turns the wheel 1 notch down at 197, 50
    And 400 milliseconds pass
    Then the property "contentItem.contentY" of "view" is 72

  Scenario: Both bars are active while the wheel moves the content
    When the user turns the wheel 1 notch down at 100, 50
    Then the property "ScrollBar.vertical.active" of "view" is true
    And the property "ScrollBar.horizontal.active" of "view" is true
    When 1 second passes
    Then the property "ScrollBar.vertical.active" of "view" is false
    And the property "ScrollBar.horizontal.active" of "view" is false

  @api-QtQuick.Controls.Control.wheelEnabled
  Scenario: A view that does not take the wheel is not scrolled by it
    When the user turns the wheel 1 notch down at 100, 150
    And 400 milliseconds pass
    Then the property "contentItem.contentY" of "deaf" is 0

  @api-QtQuick.Controls.Control.wheelEnabled
  Scenario: A view that takes the wheel again is scrolled again
    When the property "wheelEnabled" of "deaf" is set to true
    And the user turns the wheel 1 notch down at 100, 150
    And 400 milliseconds pass
    Then the property "contentItem.contentY" of "deaf" is 72

  Scenario: The wheel a view does not take goes to what the view is in
    When the user turns the wheel 1 notch down at 300, 50
    And 400 milliseconds pass
    Then the property "contentItem.contentY" of "inner" is 0
    And the property "contentY" of "outer" is 72

  Scenario: A view that is not enabled is not scrolled
    When the property "enabled" of "view" is set to false
    And the user turns the wheel 1 notch down at 100, 50
    And 400 milliseconds pass
    Then the property "contentItem.contentY" of "view" is 0
