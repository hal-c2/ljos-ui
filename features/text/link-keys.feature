# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#linkActivated-signal
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#activeFocusOnTab-prop
#   docs/ARCHITECTURE.md (sections 7.5 and 14)
# Qt Quick's Text gives the keyboard no way to a link. In Ljós a Text that
# is in the Tab order takes Tab from link to link, so these scenarios run
# on Ljós only.

@ljos-only
Feature: Reaching the links of a Text with the keyboard
  A Text that is in the Tab order (activeFocusOnTab) lets Tab stop at each
  of its links in reading order before the focus moves on. Return, Enter
  and Space activate the link the keyboard is on.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 100
        property string opened: ""
        property int times: 0
        Item { objectName: "before"; focus: true; activeFocusOnTab: true }
        Text {
          id: links; objectName: "links"; y: 20; activeFocusOnTab: true
          textFormat: Text.StyledText
          text: "<a href=\"one\">first</a> and <a href=\"two\">second</a>"
          onLinkActivated: link => { root.opened = link; root.times += 1 }
        }
        Item { objectName: "after"; activeFocusOnTab: true }
        function take() { links.forceActiveFocus() }
      }
      """
    When 100 milliseconds pass

  Scenario: Tab comes to the first link
    When the user presses "Tab"
    Then the property "activeFocus" of "links" is true
    When the user presses "Return"
    Then the property "opened" of "root" is "one"

  Scenario: Tab goes on to the next link
    When the user presses "Tab"
    And the user presses "Tab"
    Then the property "activeFocus" of "links" is true
    When the user presses "Return"
    Then the property "opened" of "root" is "two"

  Scenario: Tab leaves the Text after its last link
    When the user presses "Tab"
    And the user presses "Tab"
    And the user presses "Tab"
    Then the property "activeFocus" of "after" is true
    And the property "activeFocus" of "links" is false

  Scenario: Shift+Tab comes to the last link
    When the property "focus" of "after" is set to true
    And the user presses "Shift+Tab"
    Then the property "activeFocus" of "links" is true
    When the user presses "Return"
    Then the property "opened" of "root" is "two"

  Scenario: Shift+Tab leaves the Text before its first link
    When the user presses "Tab"
    And the user presses "Shift+Tab"
    Then the property "activeFocus" of "before" is true

  Scenario: Space activates the link the keyboard is on
    When the user presses "Tab"
    And the user presses "Space"
    Then the property "opened" of "root" is "one"
    And the property "times" of "root" is 1

  Scenario: The keyboard is on no link until Tab brings it to one
    When the method "take" of "root" is called
    And the user presses "Return"
    Then the property "times" of "root" is 0
    When the user presses "Tab"
    And the user presses "Return"
    Then the property "opened" of "root" is "one"

  Scenario: A Text without links is left by one Tab
    When the property "text" of "links" is set to "no links here"
    And the user presses "Tab"
    Then the property "activeFocus" of "links" is true
    When the user presses "Tab"
    Then the property "activeFocus" of "after" is true
