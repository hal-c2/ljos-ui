# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html (the roles List and ListItem)
#   docs/ARCHITECTURE.md section 14 (what a view tells assistive technology)

@a11y @ljos-only
Feature: What a view tells assistive technology
  A list tells how many rows it has and each row its place among all of them, also the rows
  that have no delegate. While a screen reader listens the view keeps more rows around.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 100
        property int created: 0
        ListView {
          objectName: "list"
          anchors.fill: parent
          cacheBuffer: 0
          model: 500
          Accessible.role: Accessible.List
          Accessible.name: "Messages"
          delegate: Item {
            objectName: "r" + index; width: 200; height: 20
            Accessible.role: Accessible.ListItem
            Accessible.name: "Message " + index
            Component.onCompleted: root.created++
          }
        }
      }
      """

  Scenario: The view is a list and its rows are list items inside it
    Given an assistive technology is listening
    Then the accessibility tree has a "list" named "Messages"
    And the "listitem" named "Message 2" is inside the "list" named "Messages"

  Scenario: A row tells its place among all rows of the model
    Given an assistive technology is listening
    Then the "listitem" named "Message 0" is item 1 of 500
    And the "listitem" named "Message 3" is item 4 of 500

  Scenario: The place follows the model
    Given an assistive technology is listening
    When the property "model" of "list" is set to 40
    And 100 milliseconds pass
    Then the "listitem" named "Message 3" is item 4 of 40

  Scenario: A scrolled view tells the places of the rows it shows now
    Given an assistive technology is listening
    When the property "contentY" of "list" is set to 2000
    And 100 milliseconds pass
    Then the "listitem" named "Message 100" is item 101 of 500
    And the accessibility tree has no "listitem" named "Message 40"

  Scenario: Without a screen reader only the rows in the view have delegates
    When 100 milliseconds pass
    Then the property "created" of "root" is at most 7
    And the object "r12" does not exist

  Scenario: While a screen reader is active twenty more rows are kept on each side
    When the system setting "screen reader" changes to true
    And 100 milliseconds pass
    Then the object "r20" exists
    And the object "r24" exists
    And the object "r30" does not exist

  Scenario: The extra rows go away when the screen reader stops
    When the system setting "screen reader" changes to true
    And 100 milliseconds pass
    And the system setting "screen reader" changes to false
    And 100 milliseconds pass
    Then the object "r4" exists
    And the object "r20" does not exist

  Scenario: An assistive technology scrolls the list
    Given an assistive technology is listening
    When an assistive technology asks the "list" named "Messages" to scroll down
    And 100 milliseconds pass
    Then the property "contentY" of "list" is at least 1
