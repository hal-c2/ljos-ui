# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-material.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-itemdelegate.html
# Verified on Qt 6.11.2 (qml6, offscreen): sizes read from the delegates,
# colours from a picture of the window.

@qt-parity
Feature: The Material ItemDelegate
  A Material item delegate is a row 48 pixels tall, its text 16 pixels
  from the edge. The current row (highlighted) is tinted with the theme's
  text colour, and a pressed one more faintly; any other row shows what is
  behind it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Rectangle {
        width: 300; height: 300; color: "white"
        Column {
          ItemDelegate { objectName: "plain"; text: "Ag"; width: 200 }
          ItemDelegate { objectName: "hi"; text: "Ag"; width: 200; highlighted: true }
          ItemDelegate { objectName: "down"; text: "Ag"; width: 200; down: true }
          ItemDelegate { objectName: "dis"; text: "Ag"; width: 200; enabled: false }
        }
      }
      """

  Scenario Outline: A Material item delegate's <property> is <value>
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property          | value       |
      | plain  | height            | 48          |
      | plain  | leftPadding       | 16          |
      | plain  | rightPadding      | 16          |
      | plain  | topPadding        | 8           |
      | plain  | bottomPadding     | 8           |
      | plain  | spacing           | 16          |
      | plain  | contentItem.x     | 16          |
      | plain  | contentItem.color | "#dd000000" |
      | dis    | contentItem.color | "#60000000" |
      | hi     | background.color  | "#1e000000" |
      | plain  | background.color  | "#00000000" |

  Scenario Outline: The pixel at <x>, <y> is the <part>
    Then the pixel at <x>, <y> of the window is "<colour>"

    Examples:
      | x   | y   | part                 | colour  |
      | 150 | 24  | plain row            | #ffffff |
      | 150 | 72  | current row          | #e1e1e1 |
      | 150 | 120 | pressed row          | #efefef |
      | 150 | 168 | disabled row         | #ffffff |
