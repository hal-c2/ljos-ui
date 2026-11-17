# Sources:
#   docs/ARCHITECTURE.md (principle 9: what a host builds on is general)
#   features/items/grab-to-image.feature (what a script's grab gives)

@ljos-only
Feature: A picture of an item taken for the program's own Rust
  A host that checks or keeps what an item shows (a test of what a view
  draws, a thumbnail) takes a picture of it with no script to ask. The
  item's scene draws it at its next frame as Item.grabToImage would, and
  the picture goes to the host's callback after that frame; nothing is
  called back before a frame runs. A grab the item cannot give is
  refused at once, with the reason a script's grab gives, and never
  calls back.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: root; objectName: "root"
        width: 120; height: 40; color: "#00ff00"
        Item {
          objectName: "src"
          width: 20; height: 10
          opacity: 0.5
          Rectangle { width: 10; height: 10; color: "#ff0000" }
          Rectangle { x: 15; width: 10; height: 10; color: "#0000ff" }
        }
        Item { objectName: "flat"; width: 0; height: 10 }
        Item { id: leaving; objectName: "leaving"; width: 10; height: 10 }
        function leave() { leaving.parent = null }
      }
      """

  Scenario: The host's picture holds the item's pixels
    When the host asks for a picture of "src"
    And 200 milliseconds pass
    Then the host's picture is 20 by 10 pixels
    And the pixel at 5, 5 of the host's picture is "#ff0000"
    And the pixel at 17, 5 of the host's picture is "#0000ff"

  Scenario: An item of no size is refused at once
    When the host asks for a picture of "flat"
    Then the host is refused a picture because "item's width and/or height are less than 1: QSize(0, 10)"

  Scenario: An item that leaves its window before the frame gives no picture
    When the host asks for a picture of "leaving"
    And the method "leave" of "root" is called
    And 200 milliseconds pass
    Then the host is called back with no picture
