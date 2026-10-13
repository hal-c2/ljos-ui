# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-busyindicator.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: BusyIndicator
  A busy indicator says that something is being done, for as long as it is
  `running`. When it stops it fades out; a stopped one shows nothing.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        BusyIndicator { objectName: "b" }
        BusyIndicator { objectName: "idle"; x: 100; running: false }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: A busy indicator is a square that does not take the keyboard
      Then the property "<property>" of "b" is <value>

      Examples:
        | property              | value |
        | running               | true  |
        | implicitWidth         | 60    |
        | implicitHeight        | 60    |
        | padding               | 6     |
        | focusPolicy           | 0     |
        | implicitContentWidth  | 48    |
        | implicitContentHeight | 48    |
        | contentItem.opacity   | 1     |
        | contentItem.visible   | true  |

  @qt-parity
  Rule: Running

    Scenario: One that is not running shows nothing
      Then the property "contentItem.opacity" of "idle" is 0
      And the property "contentItem.visible" of "idle" is false

    Scenario: One that stops fades out
      When the property "running" of "b" is set to false
      And 50 milliseconds pass
      Then the property "contentItem.visible" of "b" is true
      When 400 milliseconds pass
      Then the property "contentItem.opacity" of "b" is 0
      And the property "contentItem.visible" of "b" is false

    Scenario: One that starts shows again
      When the property "running" of "idle" is set to true
      And 400 milliseconds pass
      Then the property "contentItem.opacity" of "idle" is 1
      And the property "contentItem.visible" of "idle" is true
