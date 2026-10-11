# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#announce-method
#   docs/ARCHITECTURE.md (section 14, live regions)
# The Qt oracle cannot hear what is announced, so these scenarios run on Ljós
# only. An announcement is told through the accessibility tree as a status
# (polite) or an alert (assertive) whose name is the message.

@ljos-only @a11y
Feature: Announcing a message to an assistive technology
  Accessible.announce has a screen reader read a message that is not the
  text of any item.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        function say(text) { it.Accessible.announce(text) }
        function politely(text) { it.Accessible.announce(text, Accessible.Polite) }
        function alert(text) { it.Accessible.announce(text, Accessible.Assertive) }
        Rectangle {
          id: it; objectName: "it"
          width: 80; height: 30
          Accessible.role: Accessible.Button; Accessible.name: "Save"
        }
      }
      """

  Rule: While an assistive technology listens

    Background:
      Given an assistive technology is listening

    Scenario: A message is announced as a status
      When the method "say" of "root" is called with "Saved"
      Then the accessibility tree has a "status" named "Saved"

    Scenario: A polite message is a status
      When the method "politely" of "root" is called with "Saved"
      Then the accessibility tree has a "status" named "Saved"
      And the accessibility tree has no "alert" named "Saved"

    Scenario: An assertive message is an alert
      When the method "alert" of "root" is called with "Disk full"
      Then the accessibility tree has an "alert" named "Disk full"
      And the accessibility tree has no "status" named "Disk full"

    Scenario: The next message takes the place of the last
      When the method "say" of "root" is called with "Saved"
      And the method "say" of "root" is called with "Sent"
      Then the accessibility tree has a "status" named "Sent"
      And the accessibility tree has no "status" named "Saved"

    Scenario: An alert does not take the place of a status
      When the method "say" of "root" is called with "Saved"
      And the method "alert" of "root" is called with "Disk full"
      Then the accessibility tree has a "status" named "Saved"
      And the accessibility tree has an "alert" named "Disk full"

    Scenario: A message is not part of the item that announced it
      When the method "say" of "root" is called with "Saved"
      Then the "status" named "Saved" is inside the "window" named ""

    Scenario: An empty message announces nothing
      When the method "say" of "root" is called with ""
      Then the accessibility tree has no "status" named ""

    Scenario: The item that announced stays what it was
      When the method "say" of "root" is called with "Saved"
      Then the object "it" is exposed as a "button" named "Save"

  Rule: While nothing listens

    Scenario: A message nobody heard is not kept for later
      When the method "say" of "root" is called with "Early"
      Then the accessibility tree has no "status" named "Early"

    Scenario: An announcement is made without complaint
      When the method "say" of "root" is called with "Early"
      Then the log has no warnings
