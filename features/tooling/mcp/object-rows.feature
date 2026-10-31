# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.5
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Each row of the object tree says where the object comes from and lies
  An agent checks a page by its items, also those with no accessible role:
  a card made of a `Rectangle` with a `Text` in it. Each row says where the
  object was declared and, for an item, the box it takes up in the window.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: top
        width: 200; height: 200
        Timer { id: tick; interval: 100000 }
        Rectangle {
          id: card
          x: 10; y: 20; width: 100; height: 40
          Text { id: words; x: 5; y: 6; width: 50; height: 10; text: "A note" }
        }
        Rectangle { id: unseen; visible: false; x: 1; y: 2; width: 3; height: 4 }
      }
      """
    And the inspector is connected
    When the agent calls the tool "app_objects" with:
      """json
      {"root": "id=top"}
      """

  Scenario: An item without a role is listed with its box in the window
    Then the answer's "/objects/3/id" is "words"
    And the answer's "/objects/3/bounds" is [15, 26, 50, 10]
    And the answer's "/objects/2/bounds" is [10, 20, 100, 40]

  Scenario: An item that is hidden is listed all the same
    Then the answer's "/objects/4/id" is "unseen"
    And the answer's "/objects/4/visible" is false
    And the answer's "/objects/4/bounds" is [1, 2, 3, 4]

  Scenario: Every row says where it was declared
    Then the answer's "/objects/1/at" matches ":5:[0-9]+$"
    And the answer's "/objects/3/at" matches ":9:[0-9]+$"

  Scenario: An object that is not an item has no box
    Then the answer's "/objects/1/id" is "tick"
    And the answer's "/objects/1/bounds" is absent
