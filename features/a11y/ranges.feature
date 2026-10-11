# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#role-prop
#   docs/ARCHITECTURE.md (section 14)
# The documentation of Accessible.role says that an item with the role of a
# slider, a spin box, a dial or a scroll bar provides value, minimumValue,
# maximumValue and stepSize. The Qt oracle cannot read the accessibility tree
# through QML alone, so these scenarios run on Ljós only.

@ljos-only @a11y
Feature: The range of an item that has a value
  An item with the role of a slider says where it is between its ends through
  properties of its own.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 300; height: 200
        Rectangle {
          objectName: "vol"
          width: 120; height: 20
          property real value: 30
          property real minimumValue: 10
          property real maximumValue: 110
          property real stepSize: 5
          Accessible.role: Accessible.Slider
          Accessible.name: "Volume"
          Accessible.onIncreaseAction: value += stepSize
          Accessible.onDecreaseAction: value -= stepSize
        }
        Rectangle {
          objectName: "bare"
          y: 30; width: 120; height: 20
          Accessible.role: Accessible.Slider
          Accessible.name: "Bare"
        }
      }
      """
    And an assistive technology is listening

  Scenario: The item's own properties are its range
    Then the "slider" named "Volume" is at 30 in the range 10 to 110

  Scenario: The range follows the value
    When the property "value" of "vol" is set to 45
    Then the "slider" named "Volume" is at 45 in the range 10 to 110

  Scenario: The range follows its ends
    When the property "maximumValue" of "vol" is set to 60
    Then the "slider" named "Volume" is at 30 in the range 10 to 60

  Scenario: An increase asked by a screen reader is the item's to do
    When an assistive technology asks the "slider" named "Volume" to increase
    Then the "slider" named "Volume" is at 35 in the range 10 to 110

  Scenario: A decrease asked by a screen reader is the item's to do
    When an assistive technology asks the "slider" named "Volume" to decrease
    Then the property "value" of "vol" is 25

  Scenario: An item without the properties has no range
    Then the "slider" named "Bare" has no range

  Scenario Outline: Every role that has a value takes the properties
    When the property "Accessible.role" of "vol" is set to <number>
    Then the "<role>" named "Volume" is at 30 in the range 10 to 110

    Examples:
      | number | role              |
      | 49     | slider            |
      | 52     | spinbutton        |
      | 3      | scrollbar         |
      | 48     | progressindicator |
      | 39     | meter             |

  Scenario: An item with another role has no range
    When the property "Accessible.role" of "vol" is set to 43
    Then the "button" named "Volume" has no range
