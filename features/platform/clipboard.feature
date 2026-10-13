# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#canPaste-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#canPaste-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: canPaste follows the system's clipboard
  What another application copies can be pasted. Qt hears of every change
  to the clipboard; Ljós asks the clipboard again when the window becomes
  active and when an editor gets the keyboard, the moments a user can
  paste after copying elsewhere, so that nothing polls.

  Scenario: Text copied elsewhere can be pasted when the window is back
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 200
        TextInput { objectName: "input"; focus: true; width: 200 }
      }
      """
    When the window loses focus
    And "copied" is copied in another application
    And the window gains focus
    Then the property "canPaste" of "input" is true

  Scenario: Text copied elsewhere can be pasted into the editor that gets the keyboard
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 200
        TextInput { objectName: "first"; focus: true; width: 200 }
        TextEdit { objectName: "second"; y: 50; width: 200; height: 50 }
      }
      """
    When "copied" is copied in another application
    And the method "forceActiveFocus" of "second" is called
    Then the property "canPaste" of "second" is true
    And the property "activeFocus" of "second" is true

  Scenario: What is pasted is what was copied elsewhere
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 200
        TextInput { objectName: "input"; focus: true; width: 200 }
      }
      """
    When the window loses focus
    And "copied" is copied in another application
    And the window gains focus
    And the user presses "Ctrl+V"
    Then the property "text" of "input" is "copied"
