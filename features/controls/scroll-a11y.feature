# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollindicator.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollview.html
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot see the accessibility tree through QML alone, so these
# scenarios run on Ljós only.

@ljos-only @a11y
Feature: Scrolling with an assistive technology
  A scroll bar and a scroll view say what they are, and move when a screen
  reader asks them to.

  Rule: A scroll bar

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          Flickable {
            objectName: "f"
            width: 150; height: 100
            contentWidth: 300; contentHeight: 400
            ScrollBar.vertical: ScrollBar {
              objectName: "v"
              Accessible.name: "Down"
            }
            ScrollBar.horizontal: ScrollBar {
              objectName: "h"
              stepSize: 0.25
              Accessible.name: "Across"
            }
            ScrollIndicator.vertical: ScrollIndicator {
              objectName: "mark"
              Accessible.name: "Mark"
            }
          }
        }
        """

    Scenario: A scroll bar is a scroll bar to a screen reader
      Then the accessibility tree has a "scrollbar" named "Down"
      And the object "h" is exposed as a "scrollbar" named "Across"

    Scenario: A scroll indicator is a meter
      Then the object "mark" is exposed as a "meter" named "Mark"

    @api-QtQuick.Controls.ScrollBar.increase
    Scenario: An increase moves a bar one step, and the content with it
      When an assistive technology asks the "scrollbar" named "Down" to increase
      Then the property "position" of "v" is within 0.0001 of 0.1
      And the property "contentY" of "f" is within 0.01 of 40

    @api-QtQuick.Controls.ScrollBar.stepSize
    Scenario: The step of an increase is the bar's own
      When an assistive technology asks the "scrollbar" named "Across" to increase
      Then the property "position" of "h" is 0.25
      And the property "contentX" of "f" is 75

    @api-QtQuick.Controls.ScrollBar.decrease
    Scenario: A decrease moves a bar back and stops at the beginning
      When the property "contentY" of "f" is set to 60
      And an assistive technology asks the "scrollbar" named "Down" to decrease
      Then the property "contentY" of "f" is within 0.01 of 20
      When an assistive technology asks the "scrollbar" named "Down" to decrease
      Then the property "contentY" of "f" is 0

    Scenario Outline: A bar takes a value and keeps its handle on the track
      When an assistive technology asks the "scrollbar" named "Down" to take the value <asked>
      Then the property "position" of "v" is <position>
      And the property "contentY" of "f" is <content>

      Examples:
        | asked | position | content |
        | 0.5   | 0.5      | 200     |
        | 2     | 0.75     | 300     |
        | -1    | 0        | 0       |

    Scenario: A bar that is not enabled does not move
      When the property "enabled" of "v" is set to false
      And an assistive technology asks the "scrollbar" named "Down" to increase
      Then the property "position" of "v" is 0

    Scenario: An indicator shows and is not moved
      When an assistive technology asks the "meter" named "Mark" to increase
      Then the property "position" of "mark" is 0
      And the property "contentY" of "f" is 0

  Rule: A scroll view

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          ScrollView {
            objectName: "view"
            width: 200; height: 100
            Accessible.name: "Notes"
            Rectangle {
              implicitWidth: 500; implicitHeight: 300
              Rectangle {
                y: 280; width: 50; height: 20
                Accessible.role: Accessible.Button
                Accessible.name: "Last"
              }
            }
          }
        }
        """

    Scenario: A view is a scroll view to a screen reader
      Then the object "view" is exposed as a "scrollview" named "Notes"

    Scenario: A view that was given a role has that role
      When the property "Accessible.role" of "view" is set to 20
      Then the object "view" is exposed as a "group" named "Notes"

    Scenario Outline: A scroll goes one step, or a page, the way that was asked
      When an assistive technology asks the "scrollview" named "Notes" to <request>
      Then the property "contentItem.contentX" of "view" is <x>
      And the property "contentItem.contentY" of "view" is <y>

      Examples:
        | request             | x   | y   |
        | scroll down         | 0   | 72  |
        | scroll down a page  | 0   | 100 |
        | scroll right        | 72  | 0   |
        | scroll right a page | 200 | 0   |
        | scroll up           | 0   | 0   |
        | scroll left a page  | 0   | 0   |

    Scenario: A scroll stops where the content ends
      When an assistive technology asks the "scrollview" named "Notes" to scroll down a page
      And an assistive technology asks the "scrollview" named "Notes" to scroll down a page
      And an assistive technology asks the "scrollview" named "Notes" to scroll down a page
      Then the property "contentItem.contentY" of "view" is 200
      When an assistive technology asks the "scrollview" named "Notes" to scroll up
      Then the property "contentItem.contentY" of "view" is 128

    Scenario: The bars of a view follow a scroll that was asked for
      When an assistive technology asks the "scrollview" named "Notes" to scroll down a page
      Then the property "ScrollBar.vertical.position" of "view" is within 0.0001 of 0.3333

    Scenario: What is scrolled out of a view is still in the tree
      Then the "button" named "Last" is inside the "scrollview" named "Notes"
      When an assistive technology asks the "scrollview" named "Notes" to scroll down a page
      Then the "button" named "Last" is inside the "scrollview" named "Notes"

    Scenario: A view that is not enabled does not scroll
      When the property "enabled" of "view" is set to false
      And an assistive technology asks the "scrollview" named "Notes" to scroll down
      Then the property "contentItem.contentY" of "view" is 0
