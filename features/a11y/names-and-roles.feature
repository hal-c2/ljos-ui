# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qtquick-accessibility.html
#   docs/research/09-bdd-feature-files.md (section 3.3)
# The Qt oracle cannot read accessible names through QML alone, so these scenarios
# run on Ljós only (a QAccessible helper would be needed to verify them on Qt).

@ljos-only @a11y
Feature: Accessible names and roles
  Items that declare Accessible.name and Accessible.role appear in the accessibility tree.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        Rectangle {
          objectName: "save"
          width: 80; height: 30
          Accessible.role: Accessible.Button
          Accessible.name: "Save"
        }
      }
      """

  Scenario: A named button is found by role and name
    Then the accessibility tree has a "button" named "Save"

  Scenario: The author's objectName maps to the same node
    Then the object "save" is exposed as a "button" named "Save"

  Scenario: Renaming updates the tree
    When the property "Accessible.name" of "save" is set to "Store"
    Then the accessibility tree has a "button" named "Store"
    And the accessibility tree has no "button" named "Save"

  @qt-diverges
  # Qt exposes a bare Rectangle with no accessible role at all; Ljós also hides it
  # so that the tree stays small, but the reason is ours, not Qt's.
  Scenario: An item without Accessible.role is not exposed
    Then the accessibility tree has no "button" named ""
