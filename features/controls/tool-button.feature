# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-toolbutton.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ToolButton
  A tool button is a button for a tool bar: square when it has no text,
  and quieter than a push button.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        ToolButton {
          objectName: "t"
          property int clicks: 0
          onClicked: clicks++
        }
      }
      """

  Scenario Outline: A tool button without text is square
    Then the property "<property>" of "t" is <value>

    Examples:
      | property                 | value |
      | implicitWidth            | 40    |
      | implicitHeight           | 40    |
      | implicitBackgroundWidth  | 40    |
      | implicitBackgroundHeight | 40    |
      | padding                  | 6     |
      | leftPadding              | 6     |
      | spacing                  | 6     |
      | flat                     | false |
      | highlighted              | false |
      | checkable                | false |
      | icon.width               | 24    |
      | icon.height              | 24    |
      | contentItem.x            | 6     |
      | contentItem.y            | 6     |
      | contentItem.width        | 28    |
      | contentItem.height       | 28    |

  Scenario: A click is counted
    When the user touches 20, 20
    Then the property "clicks" of "t" is 1

  Scenario: A long text makes the button wider
    When the property "text" of "t" is set to "A long text for a tool"
    Then the property "implicitWidth" of "t" is at least 41
    When the property "text" of "t" is bound to "'' + (implicitWidth - implicitContentWidth)"
    Then the property "text" of "t" is "12"

  Scenario Outline: The background tells the state
    When the property "<state>" of "t" is set to true
    Then the property "background.color" of "t" is "<colour>"
    And the property "background.opacity" of "t" is <opacity>

    Examples:
      | state       | colour  | opacity |
      | enabled     | #e0e0e0 | 0.5     |
      | down        | #bdbdbd | 1       |
      | checked     | #bdbdbd | 0.5     |
      | highlighted | #bdbdbd | 0.5     |
