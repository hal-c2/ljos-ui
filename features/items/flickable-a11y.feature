# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot see the accessibility tree through QML alone, so these
# scenarios run on Ljós only.

@ljos-only @a11y
Feature: Flickable with an assistive technology
  A Flickable that was given a role or a name scrolls when a screen reader
  asks, and what it has scrolled out of view can still be found.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 250
        Flickable {
          objectName: "f"
          width: 150; height: 100
          contentWidth: 150; contentHeight: 400
          Accessible.role: Accessible.List; Accessible.name: "Rows"
          Rectangle {
            y: 380; width: 150; height: 20
            Accessible.role: Accessible.Button
            Accessible.name: "Last"
          }
        }
        Flickable {
          objectName: "bare"
          y: 120; width: 150; height: 100
          contentWidth: 150; contentHeight: 400
          Rectangle {
            width: 150; height: 20
            Accessible.role: Accessible.Button
            Accessible.name: "First"
          }
        }
      }
      """

  Scenario: A Flickable with a role is in the tree
    Then the object "f" is exposed as a "list" named "Rows"

  Scenario: A Flickable nobody gave a role or a name is not, and what is in it is
    Then the "button" named "First" is inside the "window" named ""

  Scenario Outline: A scroll goes one step, or a page, the way that was asked
    When an assistive technology asks the "list" named "Rows" to <request>
    Then the property "contentY" of "f" is <y>
    And the property "contentX" of "f" is 0

    Examples:
      | request            | y   |
      | scroll down        | 72  |
      | scroll down a page | 100 |
      | scroll up          | 0   |
      | scroll right       | 0   |

  Scenario: A scroll stops where the content ends
    When the property "contentY" of "f" is set to 250
    And an assistive technology asks the "list" named "Rows" to scroll down a page
    Then the property "contentY" of "f" is 300
    And the property "atYEnd" of "f" is true

  Scenario: What is scrolled out of view is still in the tree
    Then the "button" named "Last" is inside the "list" named "Rows"
    When an assistive technology asks the "list" named "Rows" to scroll down a page
    Then the "button" named "Last" is inside the "list" named "Rows"
