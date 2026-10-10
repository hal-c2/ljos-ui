# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-row.html
#   https://doc.qt.io/qt-6/qml-qtquick-column.html
#   https://doc.qt.io/qt-6/qtquick-positioning-layouts.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Row and Column
  Positioners place their visible, non-empty children one after another.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Row {
          objectName: "row"
          spacing: 5; padding: 2
          layoutDirection: Qt.RightToLeft
          Item { objectName: "r1"; width: 10; height: 10 }
          Item { objectName: "r2"; width: 20; height: 20 }
          Item { objectName: "hidden"; x: 100; width: 30; height: 10; visible: false }
          Item { objectName: "empty"; x: 100; width: 0; height: 10 }
          Item { objectName: "r3"; width: 5; height: 5 }
        }
        Column {
          objectName: "col"
          spacing: 3; topPadding: 1
          Item { objectName: "c1"; width: 10; height: 10 }
          Item { objectName: "c2"; width: 20; height: 20 }
        }
      }
      """
    When 100 milliseconds pass

  Scenario: A Row sizes itself to its children, spacing and padding
    Then the property "width" of "row" is 49
    And the property "height" of "row" is 24

  Scenario Outline: A right-to-left Row places children from the right
    Then the property "x" of "<child>" is <x>
    And the property "y" of "<child>" is 2

    Examples:
      | child | x  |
      | r1    | 37 |
      | r2    | 12 |
      | r3    | 2  |

  Scenario Outline: Invisible and zero-sized children are left where they are
    Then the property "x" of "<child>" is 100

    Examples:
      | child  |
      | hidden |
      | empty  |

  Scenario: A Column stacks children with spacing after its top padding
    Then the property "y" of "c1" is 1
    And the property "y" of "c2" is 14
    And the property "width" of "col" is 20
    And the property "height" of "col" is 34

  Scenario: A Column lays out again when a child grows
    When the property "height" of "c1" is set to 15
    And 100 milliseconds pass
    Then the property "y" of "c2" is 19
    And the property "height" of "col" is 39
