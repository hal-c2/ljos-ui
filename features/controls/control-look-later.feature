# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#palette-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#font-prop
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#palette-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.Control.palette @api-QtQuick.Controls.Control.font
Feature: The look of a control made after others set theirs
  A control made later, once some colours or fonts are set elsewhere,
  hands its own to what it holds, and takes the rest from where it is put.

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

  Scenario: A loaded control hands its font to what it holds
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Pane { font.italic: true }
        Pane {
          y: 100
          font.pixelSize: 20
          Loader {
            objectName: "loader"
            active: false
            sourceComponent: Pane {
              font.bold: true
              Label { objectName: "made" }
            }
          }
        }
      }
      """
    When the property "active" of "loader" is set to true
    Then the property "font.pixelSize" of "made" is 20
    And the property "font.bold" of "made" is true
    And the property "font.italic" of "made" is false

  Scenario: A colour bound on a control reaches what it holds each time it changes
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        width: 300; height: 200
        property bool dark: false
        Pane {
          palette.button: parent.dark ? "#000000" : "#ffffff"
          Column {
            Button { objectName: "inside" }
            Item { objectName: "plain" }
            Pane { palette.button: "#00aa00"; Button { objectName: "nested" } }
          }
        }
        Button { objectName: "outside"; y: 150 }
      }
      """
    When the property "dark" of "root" is set to true
    Then the property "palette.button" of "inside" is "#000000"
    And the property "palette.button" of "plain" is "#000000"
    And the property "palette.button" of "nested" is "#00aa00"
    And the property "palette.button" of "outside" is "#e0e0e0"
    When the property "dark" of "root" is set to false
    Then the property "palette.button" of "inside" is "#ffffff"
    And the property "palette.button" of "plain" is "#ffffff"

  Scenario: Colours bound on a popup reach what it holds each time they change
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        width: 300; height: 200
        property bool dark: false
        Popup {
          objectName: "popup"
          palette.button: parent.dark ? "#000000" : "#e0e0e0"
          palette.window: parent.dark ? "#202020" : "#fafafa"
          Column {
            Button { objectName: "inside" }
            Item { objectName: "plain" }
          }
        }
        Button { objectName: "outside"; y: 150 }
      }
      """
    When the property "dark" of "root" is set to true
    Then the property "palette.button" of "inside" is "#000000"
    And the property "palette.button" of "plain" is "#000000"
    And the property "palette.window" of "inside" is "#202020"
    And the property "palette.inactive.button" of "popup" is "#000000"
    And the property "palette.disabled.window" of "popup" is "#202020"
    And the property "palette.button" of "outside" is "#e0e0e0"
    When the property "dark" of "root" is set to false
    Then the property "palette.button" of "inside" is "#e0e0e0"
    And the property "palette.window" of "plain" is "#fafafa"
    And the property "palette.inactive.button" of "popup" is "#e0e0e0"
