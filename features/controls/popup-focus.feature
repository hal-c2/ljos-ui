# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#focus-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#activeFocus-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#closePolicy-prop
#   https://doc.qt.io/qt-6/qtquickcontrols-focus.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Keyboard focus of a popup
  A popup that wants focus takes it when it opens and gives it back when
  it closes. While it has it, Tab stays inside and Escape closes it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string keys: ""
        Keys.onPressed: (e) => root.keys += "root "
        Item { objectName: "outside"; focus: true; activeFocusOnTab: true }
        Item { objectName: "other"; activeFocusOnTab: true }
        Popup {
          objectName: "p"
          focus: true
          Item { objectName: "one"; focus: true; activeFocusOnTab: true
                 Keys.onPressed: (e) => root.keys += "one " }
          Item { objectName: "two"; activeFocusOnTab: true }
        }
        Popup {
          objectName: "plain"
          Item { objectName: "inPlain"; focus: true }
        }
      }
      """

  Scenario: The content has the focus while no popup is open
    Then the property "activeFocus" of "outside" is true
    And the property "activeFocus" of "p" is false
    And the property "activeFocus" of "one" is false

  @api-QtQuick.Controls.Popup.focus @api-QtQuick.Controls.Popup.activeFocus
  Scenario: A popup that wants focus takes it when it opens
    When the method "open" of "p" is called
    Then the property "activeFocus" of "p" is true
    And the property "activeFocus" of "one" is true
    And the property "activeFocus" of "outside" is false

  Scenario: A popup gives the focus back when it closes
    When the method "open" of "p" is called
    And the method "close" of "p" is called
    Then the property "activeFocus" of "outside" is true
    And the property "activeFocus" of "p" is false

  Scenario: A popup that does not want focus leaves it where it is
    When the method "open" of "plain" is called
    Then the property "activeFocus" of "outside" is true
    And the property "activeFocus" of "plain" is false
    And the property "activeFocus" of "inPlain" is false

  Scenario: A modal popup that does not want focus leaves it where it is too
    When the property "modal" of "plain" is set to true
    And the method "open" of "plain" is called
    Then the property "activeFocus" of "outside" is true

  Scenario: Tab moves to the next item in the popup
    When the method "open" of "p" is called
    And the user presses "Tab"
    Then the property "activeFocus" of "two" is true

  Scenario: Tab stays in the popup
    When the method "open" of "p" is called
    And the user presses "Tab"
    And the user presses "Tab"
    Then the property "activeFocus" of "one" is true
    And the property "activeFocus" of "other" is false

  Scenario: Shift+Tab stays in the popup
    When the method "open" of "p" is called
    And the user presses "Shift+Tab"
    Then the property "activeFocus" of "two" is true

  Scenario: Keys pressed in a popup do not reach the content
    When the method "open" of "p" is called
    And the user presses "A"
    Then the property "keys" of "root" is "one "

  Scenario: Keys reach the content while a popup without focus is open
    When the method "open" of "plain" is called
    And the user presses "A"
    Then the property "keys" of "root" is "root "

  Scenario: Escape closes the popup that has the focus
    When the method "open" of "p" is called
    And the user presses "Escape"
    Then the property "visible" of "p" is false
    And the property "activeFocus" of "outside" is true

  Scenario: Escape does not close a popup that has no focus
    When the method "open" of "plain" is called
    And the user presses "Escape"
    Then the property "visible" of "plain" is true

  @api-QtQuick.Controls.Popup.closePolicy
  Scenario: Escape does not close a popup whose policy leaves it out
    When the property "closePolicy" of "p" is set to 1
    And the method "open" of "p" is called
    And the user presses "Escape"
    Then the property "visible" of "p" is true

  Scenario: A popup that starts to want focus while it is open does not take it
    When the method "open" of "plain" is called
    And the property "focus" of "plain" is set to true
    Then the property "activeFocus" of "plain" is false
    And the property "activeFocus" of "outside" is true

  Scenario: A press beside the popup that closes it gives the focus back
    When the method "open" of "p" is called
    And the user touches 300, 300
    Then the property "visible" of "p" is false
    And the property "activeFocus" of "outside" is true

  Scenario: The second popup to open takes the focus from the first
    When the method "open" of "p" is called
    And the property "focus" of "plain" is set to true
    And the method "open" of "plain" is called
    Then the property "activeFocus" of "inPlain" is true
    And the property "activeFocus" of "p" is false

  Scenario: Closing the second popup gives the focus back to the first
    When the method "open" of "p" is called
    And the property "focus" of "plain" is set to true
    And the method "open" of "plain" is called
    And the method "close" of "plain" is called
    Then the property "activeFocus" of "one" is true
