# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-shortcut.html#activatedAmbiguously-signal
#   https://doc.qt.io/qt-6/qml-qtquick-shortcut.html#context-prop
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Shortcuts with the same sequence
  When several enabled shortcuts have the sequence that was typed, none is
  activated. One of them is told it was ambiguous, a different one each time,
  so that an application can step through them. They take their turns in
  the order they registered in: declared ones as they complete, the last
  first, and one whose sequence changes after all the others.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        Shortcut {
          objectName: "first"; sequence: "Ctrl+D"
          onActivated: root.log += "first! "
          onActivatedAmbiguously: root.log += "first "
        }
        Shortcut {
          objectName: "second"; sequence: "Ctrl+D"; context: Qt.ApplicationShortcut
          onActivated: root.log += "second! "
          onActivatedAmbiguously: root.log += "second "
        }
        Shortcut {
          objectName: "third"; sequences: ["Ctrl+E", "Ctrl+D"]
          onActivated: root.log += "third! "
          onActivatedAmbiguously: root.log += "third "
        }
      }
      """

  Scenario: Each press tells one of the shortcuts, the last one first
    When the user presses "Ctrl+D"
    Then the property "log" of "root" is "third "
    When the user presses "Ctrl+D"
    And the user presses "Ctrl+D"
    And the user presses "Ctrl+D"
    Then the property "log" of "root" is "third second first third "

  Scenario: A shortcut given its sequence again takes its turn last
    When the property "sequence" of "second" is set to "Ctrl+X"
    And the property "sequence" of "second" is set to "Ctrl+D"
    And the user presses "Ctrl+D"
    And the user presses "Ctrl+D"
    And the user presses "Ctrl+D"
    Then the property "log" of "root" is "third first second "

  Scenario: A sequence only one shortcut has activates it
    When the user presses "Ctrl+E"
    Then the property "log" of "root" is "third! "

  Scenario: Disabling the others ends the ambiguity
    When the property "enabled" of "second" is set to false
    And the property "enabled" of "third" is set to false
    And the user presses "Ctrl+D"
    Then the property "log" of "root" is "first! "
