# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#role-prop
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#announce-method
#   https://doc.qt.io/qt-6/qaccessible.html#Role-enum
#   docs/ARCHITECTURE.md (section 14, live regions)
#   docs/research/05-accessibility-and-localisation.md (section 2.4, the role table)
# What Qt hands an assistive technology cannot be read from QML, so these
# scenarios run on Ljós only. A live region is a node whose name is said
# again when it changes, without the user going to it: a status politely,
# an alert at once.

@ljos-only @a11y
Feature: Live regions
  A text whose role is a status bar, a notification or an alert message
  tells an assistive technology each new thing it says, and so does a
  message given to Accessible.announce.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string message: "Saving"
        function finish() { message = "Saved" }
        function say(text) { plain.Accessible.announce(text) }
        function alert(text) { plain.Accessible.announce(text, Accessible.Assertive) }
        Column {
          Label {
            objectName: "label"; text: root.message
            Accessible.role: Accessible.StatusBar; Accessible.name: text
          }
          Text {
            objectName: "count"; text: "3 new"
            Accessible.role: Accessible.Notification; Accessible.name: text
          }
          Text {
            objectName: "error"; text: "Disk full"
            Accessible.role: Accessible.AlertMessage; Accessible.name: text
          }
          Text { id: plain; objectName: "plain"; text: "Notes"; Accessible.name: text }
        }
      }
      """
    And an assistive technology is listening

  Scenario: A status label is a status that is live
    Then the object "label" is exposed as a "status" named "Saving"
    And the "status" named "Saving" is live
    And the "status" named "Saving" is not assertive

  Scenario: What a status label says next is what the status is named
    When the method "finish" of "root" is called
    Then the accessibility tree has a "status" named "Saved"
    And the accessibility tree has no "status" named "Saving"
    And the "status" named "Saved" is live

  Scenario: A notification is a polite status
    Then the object "count" is exposed as a "status" named "3 new"
    And the "status" named "3 new" is live
    And the "status" named "3 new" is not assertive

  Scenario: An alert message is an alert that interrupts
    Then the object "error" is exposed as an "alert" named "Disk full"
    And the "alert" named "Disk full" is live
    And the "alert" named "Disk full" is assertive

  Scenario: A text with no such role is not live
    Then the object "plain" is exposed as a "label" named "Notes"
    And the "label" named "Notes" is not live

  Scenario: A polite announcement is a live status
    When the method "say" of "root" is called with "Copied"
    Then the "status" named "Copied" is live
    And the "status" named "Copied" is not assertive

  Scenario: An assertive announcement is a live alert that interrupts
    When the method "alert" of "root" is called with "Offline"
    Then the "alert" named "Offline" is live
    And the "alert" named "Offline" is assertive
