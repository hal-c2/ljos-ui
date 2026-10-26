# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-action.html#shortcut-prop
#   https://doc.qt.io/qt-6/qml-qtquick-shortcut.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The shortcut of an Action
  An action's shortcut triggers it from anywhere in its window, as a
  Shortcut would, with the action itself as the source. Actions that
  share a sequence are triggered in turn, one each press, in the order
  they were given it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        width: 200; height: 200
        property string log: ""
        Action {
          id: k; objectName: "k"; shortcut: "Ctrl+K"; checkable: true
          onTriggered: (source) => root.log += (source === k ? "self " : "other ")
        }
        Action {
          objectName: "j"; shortcut: "Ctrl+J"; enabled: false
          onTriggered: root.log += "j "
        }
        Action {
          objectName: "s"; shortcut: StandardKey.Copy
          onTriggered: root.log += "s "
        }
        Action { objectName: "later"; onTriggered: root.log += "later " }
        Action { objectName: "blank"; shortcut: ""; onTriggered: root.log += "blank " }
        Shortcut { sequence: "Ctrl+P"; onActivatedAmbiguously: root.log += "told " }
      }
      """

  @api-QtQuick.Controls.Action.shortcut
  Scenario: The shortcut triggers the action, which is its own source
    When the user presses "Ctrl+K"
    Then the property "log" of "root" is "self "
    And the property "checked" of "k" is true

  Scenario: A disabled action is not triggered by its shortcut
    When the user presses "Ctrl+J"
    Then the property "log" of "root" is ""

  Scenario: An action enabled later is triggered by its shortcut
    When the property "enabled" of "j" is set to true
    And the user presses "Ctrl+J"
    Then the property "log" of "root" is "j "

  Scenario: A changed shortcut takes the place of the old one
    When the property "shortcut" of "k" is set to "Ctrl+L"
    And the user presses "Ctrl+K"
    Then the property "log" of "root" is ""
    When the user presses "Ctrl+L"
    Then the property "log" of "root" is "self "

  Scenario: A standard key is a shortcut
    When the user presses "Ctrl+C"
    Then the property "log" of "root" is "s "

  Scenario: An action given a shortcut later is triggered by it
    When the property "shortcut" of "later" is set to "Ctrl+M"
    And the user presses "Ctrl+M"
    Then the property "log" of "root" is "later "

  Scenario: A blank shortcut is none until a sequence is given
    When the property "shortcut" of "blank" is set to "Ctrl+B"
    And the user presses "Ctrl+B"
    Then the property "log" of "root" is "blank "

  Scenario: Actions that share a shortcut are triggered in turn
    When the property "shortcut" of "later" is set to "Ctrl+K"
    And the user presses "Ctrl+K"
    Then the property "log" of "root" is "self "
    When the user presses "Ctrl+K"
    Then the property "log" of "root" is "self later "
    When the property "shortcut" of "later" is set to ""
    And the user presses "Ctrl+K"
    Then the property "log" of "root" is "self later self "

  Scenario: An action given a Shortcut's sequence takes its turn after it
    When the property "shortcut" of "later" is set to "Ctrl+P"
    And the user presses "Ctrl+P"
    And the user presses "Ctrl+P"
    And the user presses "Ctrl+P"
    Then the property "log" of "root" is "told later told "
