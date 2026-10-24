# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#lineCount-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#wrapMode-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A label and an editor of the same text break it each their own way
  Each text item breaks its text at its own width. A label without a width
  stays on one line, even after an editor wrapped the same text in the same
  font.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 200
        TextEdit {
          objectName: "edit"; width: 40; wrapMode: TextEdit.WordWrap
          text: "hello brave world"
        }
        Text { objectName: "label"; y: 100 }
      }
      """
    When 100 milliseconds pass
    And the property "text" of "label" is set to "hello brave world"
    And 100 milliseconds pass

  Scenario: The editor wraps and the label does not
    Then the property "lineCount" of "edit" is 3
    And the property "lineCount" of "label" is 1
