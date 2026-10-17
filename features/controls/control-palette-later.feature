# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#palette-prop
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.Control.palette
Feature: The palette of a control made after colours were set
  A control made later, once some colours are set elsewhere, hands its own
  colours to what it holds, and takes the rest from where it is put.

  Scenario: A loaded control hands its colours to what it holds
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Pane { palette.button: "#aa0000" }
        Pane {
          y: 100
          palette.window: "#0000aa"
          Loader {
            objectName: "loader"
            active: false
            sourceComponent: Pane {
              palette.button: "#00aa00"
              Button { objectName: "made" }
            }
          }
        }
      }
      """
    When the property "active" of "loader" is set to true
    Then the property "palette.button" of "made" is "#00aa00"
    And the property "palette.window" of "made" is "#0000aa"
