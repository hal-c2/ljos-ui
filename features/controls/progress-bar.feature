# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-progressbar.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: ProgressBar
  A progress bar shows how far an operation has gone, as a value between
  `from` and `to`. One whose progress is not known is `indeterminate`. It
  shows and takes nothing from the keyboard.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        ProgressBar { objectName: "p"; value: 0.25 }
        ProgressBar { objectName: "rtl"; y: 20; value: 0.25; LayoutMirroring.enabled: true }
        ProgressBar { objectName: "back"; y: 40; from: 10; to: 0; value: 2 }
        ProgressBar { objectName: "unknown"; y: 60; indeterminate: true; value: 0.5 }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: A progress bar is a thin bar that does not take the keyboard
      Then the property "<property>" of "p" is <value>

      Examples:
        | property                 | value     |
        | from                     | 0         |
        | to                       | 1         |
        | indeterminate            | false     |
        | implicitWidth            | 200       |
        | implicitHeight           | 6         |
        | padding                  | 0         |
        | focusPolicy              | 0         |
        | activeFocusOnTab         | false     |
        | implicitBackgroundWidth  | 200       |
        | implicitBackgroundHeight | 6         |
        | background.color         | "#e4e4e4" |
        | contentItem.x            | 0         |
        | contentItem.width        | 200       |
        | contentItem.height       | 6         |

  @qt-parity
  Rule: The value

    Scenario: The position is the value's share of the way
      Then the property "value" of "p" is 0.25
      And the property "position" of "p" is 0.25
      And the property "visualPosition" of "p" is 0.25

    Scenario Outline: The value stays between the ends
      When the property "value" of "p" is set to <given>
      Then the property "value" of "p" is <kept>

      Examples:
        | given | kept |
        | 0.5   | 0.5  |
        | 2     | 1    |
        | -1    | 0    |

    Scenario: Moving an end takes the value with it
      When the property "to" of "p" is set to 0.2
      Then the property "value" of "p" is 0.2
      And the property "position" of "p" is 1

    Scenario: The ends may be given the other way round
      Then the property "value" of "back" is 2
      And the property "position" of "back" is 0.8

    Scenario: A mirrored bar fills from the right
      Then the property "position" of "rtl" is 0.25
      And the property "visualPosition" of "rtl" is 0.75
      And the property "contentItem.x" of "rtl" is 0
      And the property "contentItem.width" of "rtl" is 200

    Scenario: An indeterminate bar keeps its value
      Then the property "indeterminate" of "unknown" is true
      And the property "position" of "unknown" is 0.5
      When the property "indeterminate" of "unknown" is set to false
      Then the property "position" of "unknown" is 0.5
