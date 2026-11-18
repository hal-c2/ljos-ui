# Sources:
#   docs/ARCHITECTURE.md (principle 9: what a host builds on is general)
#   features/items/grab-to-image.feature (what a script's grab gives)
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#grabToImage-method (what Item.grabToImage gives)

@ljos-only
Feature: A picture of an item taken for the program's own Rust
  A host that checks or keeps what an item shows (a test of what a view
  draws, a thumbnail) takes a picture of it with no script to ask. The
  item's scene draws it at its next frame as Item.grabToImage would, and
  the picture goes to the host's callback after that frame; nothing is
  called back before a frame runs. A grab the item cannot give is
  refused at once, with the reason a script's grab gives, and never
  calls back. Every grab asked is called back once, with no picture
  when its item leaves its window or is destroyed before the frame, so
  a host waiting on one is never left waiting.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Window
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
        Item { objectName: "doomed"; width: 10; height: 10 }
        Item { objectName: "own"; width: 10; height: 10; property int visibility: 0 }
        Window {
          visible: false; width: 40; height: 40
          Item { objectName: "unseen"; width: 10; height: 10 }
        }
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

  Scenario: An item in no window is refused at once
    When the method "leave" of "root" is called
    And the host asks for a picture of "leaving"
    Then the host is refused a picture because "item is not attached to a window"

  Scenario: An item of a window that is not shown is refused at once
    When the host asks for a picture of "unseen"
    Then the host is refused a picture because "item's window is not visible"

  Scenario: An item's own property called visibility does not hide its window
    When the host asks for a picture of "own"
    And 200 milliseconds pass
    Then the host's picture is 10 by 10 pixels

  Scenario: An item destroyed before the frame gives no picture
    When the host asks for a picture of "doomed"
    And the object "doomed" is destroyed
    And 200 milliseconds pass
    Then the host is called back with no picture
