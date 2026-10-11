# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html#doubleClicked-signal
# Verified on Qt 6.11.2 (qml6, offscreen; pointer input over its vnc platform).

@qt-parity
Feature: Quick clicks on a button nobody asked for double clicks of
  A second click that follows the first at once is a double click only to
  a button that has a `doubleClicked` handler. Every other button counts
  each click, however fast they come.

  Scenario Outline: Two clicks <apart> apart on a <control> are two clicks
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        <control> {
          objectName: "b"
          width: 100; height: 40
          property int clicks: 0
          onClicked: clicks += 1
        }
      }
      """
    When the user touches 30, 20
    And <apart> pass
    And the user touches 30, 20
    Then the property "clicks" of "b" is 2

    Examples:
      | control        | apart            |
      | Button         | 0 milliseconds   |
      | Button         | 100 milliseconds |
      | AbstractButton | 100 milliseconds |
      | ToolButton     | 100 milliseconds |
