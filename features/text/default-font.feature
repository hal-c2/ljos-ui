# Sources:
#   https://doc.qt.io/qt-6/qguiapplication.html#font
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font-prop
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#application-prop
#   https://doc.qt.io/qt-6/qml-font.html
# Verified on Qt 6.11.2 (qml6, offscreen, as on a Unix desktop with no
# desktop theme): the application font is "Sans Serif" at 9 points, which
# fontconfig turns into the system's sans-serif face.

@qt-parity
Feature: The default font
  Text that sets no font is drawn in the application font: the platform's
  sans-serif face at its default size. A script reads that font back whole,
  sizes in both units included.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        objectName: "root"
        property font app: Qt.application.font
        Text { objectName: "text"; text: "x" }
        TextEdit { objectName: "edit" }
        TextInput { objectName: "input" }
        Label { objectName: "label"; text: "x" }
        Button { objectName: "button"; text: "x" }
        Text { objectName: "pixels"; font.pixelSize: 20 }
        Text { objectName: "points"; font.pointSize: 20 }
        Text { objectName: "bold"; font.bold: true }
        Text {
          objectName: "wrapped"; width: 100; wrapMode: Text.WordWrap
          text: "Hello world, this is a long line of text"
        }
      }
      """
    When 100 milliseconds pass

  Scenario Outline: An item that sets no font has the application font
    Then the property "font.family" of "<item>" is "Sans Serif"
    And the property "font.pointSize" of "<item>" is 9
    And the property "font.pixelSize" of "<item>" is 12
    And the property "font.weight" of "<item>" is 400

    Examples:
      | item   |
      | text   |
      | edit   |
      | input  |
      | label  |
      | button |

  Scenario: The application font is the platform's default
    Then the property "app.family" of "root" is "Sans Serif"
    And the property "app.pointSize" of "root" is 9
    And the property "app.pixelSize" of "root" is 12

  Scenario: A font that sets one size reads the other in its own unit
    Then the property "font.pixelSize" of "pixels" is 20
    And the property "font.pointSize" of "pixels" is 15
    And the property "font.pointSize" of "points" is 20
    And the property "font.pixelSize" of "points" is 26

  Scenario: A font that changes one thing keeps the default family and size
    Then the property "font.family" of "bold" is "Sans Serif"
    And the property "font.pixelSize" of "bold" is 12

  Scenario: A line of the default font is as tall as Qt's
    Then the property "height" of "text" is 14
    And the property "implicitWidth" of "text" is 6
    And the property "height" of "label" is 14

  Scenario: Text wraps into as many lines as in Qt
    Then the property "lineCount" of "wrapped" is 2

  Scenario: A script's font starts from the application font
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "script"
        Text { id: plain }
        Text { id: px; font.pixelSize: 12 }
        Text { id: other }
        Text { id: otherPx; font.pixelSize: 12 }
        property string written: plain.font.toString()
        property string pixelWritten: px.font.toString()
        property int copied: { const f = other.font; f.pointSize = 20; return f.pixelSize }
        property real kept: { const f = otherPx.font; f.pointSize = 20; return f.pointSize }
        property int made: Qt.font({ bold: true }).pixelSize
        property string madeFamily: Qt.font({ bold: true }).family
      }
      """
    Then the property "written" of "script" is "QFont(Sans Serif,9,-1,5,400,0,0,0,0,0,0,0,0,0,0,1,,0,0)"
    And the property "pixelWritten" of "script" is "QFont(Sans Serif,-1,12,5,400,0,0,0,0,0,0,0,0,0,0,1,,0,0)"
    And the property "copied" of "script" is 26
    And the property "kept" of "script" is 9
    And the property "made" of "script" is 12
    And the property "madeFamily" of "script" is "Sans Serif"
