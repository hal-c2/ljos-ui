# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#linkActivated-signal
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   docs/ARCHITECTURE.md (sections 7.5 and 14)
# The Qt oracle cannot read the accessibility tree through QML alone, so
# these scenarios run on Ljós only.

@ljos-only @a11y
Feature: The links of a Text in the accessibility tree
  Each link of a Text that is in the accessibility tree is a node of its
  own inside the Text's node, named by the words it shows, so a screen
  reader can list the links and activate one.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 100
        property string opened: ""
        Text {
          objectName: "note"
          Accessible.role: Accessible.StaticText
          Accessible.name: "Note"
          textFormat: Text.MarkdownText
          text: "See [the manual](manual) or [ask](help)."
          onLinkActivated: link => root.opened = link
        }
        Text { objectName: "silent"; y: 40; text: "<a href=\"x\">unheard</a>" }
      }
      """
    And an assistive technology is listening

  Scenario: A link is a node inside the Text's node
    Then the accessibility tree has a "link" named "the manual"
    And the "link" named "the manual" is inside the "label" named "Note"
    And the "link" named "ask" is inside the "label" named "Note"

  Scenario: Asking a link to press activates it
    When an assistive technology asks the "link" named "ask" to press
    Then the property "opened" of "root" is "help"

  Scenario: A link that goes away with the text leaves the tree
    When the property "text" of "note" is set to "See [the manual](manual)."
    Then the accessibility tree has a "link" named "the manual"
    And the accessibility tree has no "link" named "ask"

  Scenario: A link that comes with the text joins the tree
    When the property "text" of "note" is set to "[one](1) [two](2) [three](3)"
    Then the "link" named "three" is inside the "label" named "Note"
    And the accessibility tree has no "link" named "ask"

  Scenario: Text that is plain again has no links
    When the property "textFormat" of "note" is set to 0
    Then the accessibility tree has no "link" named "ask"

  Scenario: The links of a Text that is not in the tree are not in it
    Then the accessibility tree has no "link" named "unheard"
