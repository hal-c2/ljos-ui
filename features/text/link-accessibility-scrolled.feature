# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#linkActivated-signal
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
#   docs/ARCHITECTURE.md (sections 7.5 and 14)
# The Qt oracle cannot read the accessibility tree through QML alone, so
# these scenarios run on Ljós only.

@ljos-only @a11y
Feature: The links of a Text that scrolls
  A link of a Text inside a Flickable is where the text is after a scroll:
  it leaves the screen with the text, comes back with it, and a press on
  it activates the link that was asked.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 300
        property string opened: ""
        Flickable {
          objectName: "view"
          y: 40; width: 200; height: 120; clip: true
          contentHeight: 2400
          Text {
            y: 2000
            Accessible.role: Accessible.StaticText
            Accessible.name: "Note"
            textFormat: Text.MarkdownText
            text: "See [the manual](manual) or [ask](help)."
            onLinkActivated: link => root.opened = link
          }
        }
      }
      """
    And an assistive technology is listening

  Scenario: A link below the view is off screen
    Then the "link" named "ask" is off screen

  Scenario: A link scrolled into the view is on screen
    When the property "contentY" of "view" is set to 1950
    Then the "link" named "ask" is not off screen
    And the "link" named "ask" is inside the "label" named "Note"

  Scenario: A link scrolled out of the view again is off screen
    When the property "contentY" of "view" is set to 1950
    And the property "contentY" of "view" is set to 0
    Then the "link" named "ask" is off screen

  # The press lands where the link is now, not where it was.
  Scenario: Asking a scrolled link to press activates it
    When the property "contentY" of "view" is set to 1950
    And an assistive technology asks the "link" named "ask" to press
    Then the property "opened" of "root" is "help"
