# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qml-qtquick-text.html
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot read the accessibility tree through QML alone, so these
# scenarios run on Ljós only. Whether Qt gives a Text that sets nothing a node of
# its own was not checked; in Ljós a Text is a node when it asks to be one.

@ljos-only @a11y
Feature: Text in the accessibility tree
  A Text that has an accessible role is read out as its text.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        Text {
          objectName: "status"
          text: "Ready"
          Accessible.role: Accessible.StaticText
        }
        Text {
          objectName: "plain"
          y: 30
          text: "Plain"
        }
      }
      """
    And an assistive technology is listening

  Scenario: A Text with a role is named by its text
    Then the object "status" is exposed as a "label" named "Ready"

  Scenario: Changing the text renames the node
    When the property "text" of "status" is set to "Saved"
    Then the object "status" is exposed as a "label" named "Saved"

  Scenario: An accessible name wins over the text
    When the property "Accessible.name" of "status" is set to "Status"
    Then the object "status" is exposed as a "label" named "Status"

  Scenario: A Text that sets nothing accessible is not a node
    Then the accessibility tree has no "label" named "Plain"

  Scenario: A Text with only an accessible name is a label
    When the property "Accessible.name" of "plain" is set to "Hint"
    Then the object "plain" is exposed as a "label" named "Hint"
