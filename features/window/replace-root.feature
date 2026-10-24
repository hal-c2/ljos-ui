# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
#   docs/ARCHITECTURE.md §7.3 (one platform window for the root)

# Qt has no way to show another object in a window a root already has: an
# application there makes a second window and hides the first.
@ljos-only
Feature: The application puts another root in its window
  An application that makes its whole interface again, because a setting
  switched layouts or a file of the user's appeared, makes a new root and
  puts it in the window of the old one. The new root takes the old one's
  window, at the size it has, and the old root goes. A window that is
  never shown twice has nothing to put back.

  Background:
    Given the file "Next.qml":
      """qml
      import QtQuick
      Window {
        objectName: "next"
        width: 200; height: 100
        visible: true
      }
      """
    And the file "First.qml":
      """qml
      import QtQuick
      Window {
        objectName: "first"
        width: 300; height: 240
        visible: true
        Item { objectName: "inner" }
        Component.onDestruction: console.info("first gone")
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item { objectName: "root" }
      """
    When the file "First.qml" is opened in a window of its own

  Scenario: The new root takes the window at the size it has
    When the application puts an object of "Next.qml" in the window of "first"
    Then the property "width" of "next" is 300
    And the property "height" of "next" is 240

  Scenario: The old root goes once the new one is in its window
    When the application puts an object of "Next.qml" in the window of "first"
    Then the object "first" does not exist
    And the log has an info matching "first gone"

  Scenario: The root that took a window can give it to another
    When the application puts an object of "Next.qml" in the window of "first"
    And the application puts an object of "First.qml" in the window of "next"
    Then the property "width" of "first" is 300
    And the object "next" does not exist

  Scenario: An object that is not in a window of its own has no window to give
    Then the application cannot put an object of "Next.qml" in the window of "inner"
    And the object "inner" exists
