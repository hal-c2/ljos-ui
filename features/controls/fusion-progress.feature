# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-progressbar.html#indeterminate-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-busyindicator.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.ProgressBar @api-QtQuick.Controls.BusyIndicator
Feature: How the Fusion style shows progress
  A progress bar is a sunken field with a faint shade under its top edge,
  filled in the highlight as far as it has come. One that does not know
  how far is filled all the way, with stripes that move along it. A busy
  indicator is a ring in the text's colour that fades towards its tail.

  Background:
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """

  Scenario: A bar is shaded under its top edge and filled as far as it has come
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Rectangle {
        width: 140; height: 44
        ProgressBar { x: 10; y: 10; value: 0.25 }
      }
      """
    Then the pixel at 70, 11 of the window is within 3 of "#ededed"
    And the pixel at 70, 22 of the window is "#ffffff"
    And the pixel at 20, 22 of the window is within 6 of "#359cdc"

  Scenario: A bar that does not know how far it has come is filled all the way
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Rectangle {
        width: 140; height: 44
        ProgressBar { x: 10; y: 10; indeterminate: true }
      }
      """
    Then the pixel at 10, 22 of the window is within 3 of "#22648d"
    And the pixel at 100, 22 of the window is within 8 of "#36a0e1"
    And the pixel at 125, 22 of the window is within 8 of "#36a0e1"

  Scenario: A busy indicator is a ring in the text's colour
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Rectangle {
        width: 60; height: 60
        BusyIndicator { objectName: "busy"; x: 10; y: 10 }
      }
      """
    Then the property "implicitWidth" of "busy" is 40
    And the property "contentItem.width" of "busy" is 28
    And the property "contentItem.color" of "busy" is "#000000"
    And the pixel at 30, 30 of the window is "#ffffff"
