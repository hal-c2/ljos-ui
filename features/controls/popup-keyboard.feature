# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#focus-prop
#   https://doc.qt.io/qt-6/qtquickcontrols-focus.html
# Verified on Qt 6.11.2 and 6.12.0 (qml6, offscreen).

@qt-parity
Feature: Where the keyboard goes when a popup that has it goes
  A popup that wants focus takes the keyboard when it opens. The keyboard
  can also be put in a popup that does not want it, by giving an item in
  it focus. Either way, when a popup that has the keyboard goes, the
  keyboard goes to the nearest popup still open that took it when it
  opened, or else back to what last had it in the window's content. It
  never stays in a popup that has gone, and never goes back into one
  that did not ask for it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        Item { objectName: "outside"; focus: true }
        Item { objectName: "other" }
        Popup {
          objectName: "p"
          focus: true
          Item { objectName: "one"; focus: true }
        }
        Popup {
          objectName: "plain"
          Item { objectName: "inPlain"; focus: true }
        }
        Popup {
          objectName: "third"
          focus: true
          Item { objectName: "inThird"; focus: true }
        }
      }
      """

  Rule: A popup that was given the keyboard gives it on when it goes

    Scenario: A popup that did not ask for the keyboard gives it back to the content
      When the method "open" of "plain" is called
      And the method "forceActiveFocus" of "inPlain" is called
      And the method "close" of "plain" is called
      Then the property "activeFocus" of "outside" is true
      And the property "activeFocus" of "inPlain" is false

    Scenario: It gives it to the popup beneath that took it
      When the method "open" of "p" is called
      And the method "open" of "plain" is called
      And the method "forceActiveFocus" of "inPlain" is called
      And the method "close" of "plain" is called
      Then the property "activeFocus" of "one" is true
      And the property "activeFocus" of "inPlain" is false

    Scenario: A popup that took the keyboard and closed beneath it is passed over
      When the method "open" of "p" is called
      And the method "open" of "plain" is called
      And the method "forceActiveFocus" of "inPlain" is called
      And the method "close" of "p" is called
      And the method "close" of "plain" is called
      Then the property "activeFocus" of "outside" is true
      And the property "activeFocus" of "inPlain" is false

  Rule: The keyboard never goes back into a popup that did not ask for it

    Scenario: A popup that took the keyboard from one that was given it gives it to the content
      When the method "open" of "plain" is called
      And the method "forceActiveFocus" of "inPlain" is called
      And the method "open" of "p" is called
      And the method "close" of "p" is called
      Then the property "activeFocus" of "outside" is true
      And the property "activeFocus" of "inPlain" is false

    Scenario: Nor into one that closed meanwhile
      When the method "open" of "plain" is called
      And the method "forceActiveFocus" of "inPlain" is called
      And the method "open" of "p" is called
      And the method "close" of "plain" is called
      And the method "close" of "p" is called
      Then the property "activeFocus" of "outside" is true
      And the property "activeFocus" of "inPlain" is false

    Scenario: The content gets back what had the keyboard there last
      When the method "forceActiveFocus" of "other" is called
      And the method "open" of "plain" is called
      And the method "forceActiveFocus" of "inPlain" is called
      And the method "open" of "p" is called
      And the method "close" of "p" is called
      Then the property "activeFocus" of "other" is true
      And the property "activeFocus" of "outside" is false

  Rule: Popups that took the keyboard give it back in the order they took it

    Scenario: The middle one of three closes, then the top one
      When the method "open" of "p" is called
      And the property "focus" of "plain" is set to true
      And the method "open" of "plain" is called
      And the method "open" of "third" is called
      And the method "close" of "plain" is called
      And the method "close" of "third" is called
      Then the property "activeFocus" of "one" is true

    Scenario: The bottom one closes, then the top one
      When the method "open" of "p" is called
      And the property "focus" of "plain" is set to true
      And the method "open" of "plain" is called
      And the method "open" of "third" is called
      And the method "close" of "p" is called
      And the method "close" of "third" is called
      Then the property "activeFocus" of "inPlain" is true

    # qml6 (6.11.2 and 6.12.0) gives the keyboard to the window's own root
    # item here, so that no item of the application has it; with two
    # popups closed the same way it goes back to the content. Ljós goes
    # back to the content in both.
    @qt-diverges
    Scenario: The two beneath close, then the top one
      When the method "open" of "p" is called
      And the property "focus" of "plain" is set to true
      And the method "open" of "plain" is called
      And the method "open" of "third" is called
      And the method "close" of "p" is called
      And the method "close" of "plain" is called
      And the method "close" of "third" is called
      Then the property "activeFocus" of "outside" is true
