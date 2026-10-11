# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   docs/ARCHITECTURE.md (section 14)
# Accessible has no property for the place of a row among its rows; the model
# is what knows it. The Qt oracle cannot read the accessibility tree through
# QML alone, so these scenarios run on Ljós only.

@ljos-only @a11y
Feature: Rows made from a model say which row they are
  A row that a Repeater makes tells an assistive technology its place among
  the rows of the model and how many there are, so that "2 of 3" can be read
  without counting.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 300; height: 300
        property int rows: 3
        Column {
          Repeater {
            model: root.rows
            Rectangle {
              width: 100; height: 30
              Accessible.role: Accessible.ListItem
              Accessible.name: "Row " + (index + 1)
            }
          }
          Repeater {
            model: 2
            Rectangle {
              width: 100; height: 30
              Accessible.role: Accessible.Button
              Accessible.name: "Button " + (index + 1)
            }
          }
        }
      }
      """
    And an assistive technology is listening

  Scenario Outline: A row says its place and the number of rows
    Then the "listitem" named "<row>" is item <place> of 3

    Examples:
      | row   | place |
      | Row 1 | 1     |
      | Row 2 | 2     |
      | Row 3 | 3     |

  Scenario: The rows follow the model when it grows
    When the property "rows" of "root" is set to 5
    Then the "listitem" named "Row 2" is item 2 of 5
    And the "listitem" named "Row 5" is item 5 of 5

  Scenario: The rows follow the model when it shrinks
    When the property "rows" of "root" is set to 2
    Then the "listitem" named "Row 2" is item 2 of 2

  Scenario: What is not a row has no place to tell
    Then the "button" named "Button 1" is not an item of a set
