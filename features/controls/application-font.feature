# Sources:
#   https://doc.qt.io/qt-6/qguiapplication.html#setFont
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#font-prop
#   https://doc.qt.io/qt-6/qml-qtquick-application.html
# Qt 6.11.2 (qml6, offscreen): assigning `Qt.application.font` from QML throws
# "TypeError: Cannot assign to read-only property", so only the host changes it; a
# test cannot, which is why the change itself is Ljós's alone.
# A host program built on Qt's public API that calls `QGuiApplication::setFont` while the
# window shows sees `Qt.application.font`, `Qt.font({...})` and every Text, Label and control
# made afterwards take the new font, and a Text made before keep its own. Qt 6.11 leaves the
# controls and windows made before on the old font too; Ljós makes them follow, as a desktop's
# font change asks.

@ljos-only @api-QtQuick.Application.font
Feature: The application font changes while the application runs
  The host can make another font the application font at any time, as a desktop does when its
  user picks another one. Every control, label and window that inherits its font then takes the
  new one. An attribute that an object, or one of the objects above it, set itself stays.
  `Qt.application.font` reads the new font, and QML cannot assign it. A `Qt.font({...})` and a
  plain text made after the change start from the new font; a plain text made before keeps its own.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        objectName: "window"
        width: 300; height: 300
        property font app: Qt.application.font
        Column {
          Label { objectName: "label"; text: "x" }
          Button { objectName: "bold"; text: "x"; font.bold: true }
          Button { objectName: "sized"; text: "x"; font.pixelSize: 30 }
          Pane {
            objectName: "pane"
            font.italic: true
            Label { objectName: "inner"; text: "x" }
          }
          Text { objectName: "plain"; text: "x" }
        }
      }
      """

  Scenario: Every font that inherits takes the new application font
    When the application font is set to:
      | family    | Serif |
      | pixelSize | 20    |
    Then the property "font.family" of "window" is "Serif"
    And the property "font.pixelSize" of "window" is 20
    And the property "font.family" of "label" is "Serif"
    And the property "font.pixelSize" of "label" is 20
    And the property "font.pixelSize" of "inner" is 20

  Scenario: What an object set itself stays
    When the application font is set to:
      | family    | Serif |
      | pixelSize | 20    |
    Then the property "font.bold" of "bold" is true
    And the property "font.family" of "bold" is "Serif"
    And the property "font.pixelSize" of "sized" is 30
    And the property "font.family" of "sized" is "Serif"
    And the property "font.italic" of "pane" is true
    And the property "font.italic" of "inner" is true
    And the property "font.bold" of "label" is false

  Scenario: The parts of a control are drawn with the new font
    When the application font is set to:
      | pixelSize | 20 |
    Then the property "contentItem.font.pixelSize" of "window/bold" is 20

  Scenario: Qt.application.font is the new font
    When the application font is set to:
      | family    | Serif |
      | pixelSize | 20    |
    Then the property "app.family" of "window" is "Serif"
    And the property "app.pixelSize" of "window" is 20

  Scenario: Another change follows the first
    When the application font is set to:
      | pixelSize | 20 |
    And the application font is set to:
      | pixelSize | 24 |
      | italic    | true |
    Then the property "font.pixelSize" of "label" is 24
    And the property "font.italic" of "label" is true
    And the property "font.pixelSize" of "sized" is 30

  Scenario: Taking the application font back gives the default again
    When the application font is set to:
      | pixelSize | 20 |
    And the application font is set back to the default
    Then the property "font.family" of "label" is "Sans Serif"
    And the property "font.pointSize" of "label" is 9
    And the property "app.pointSize" of "window" is 9

  Scenario: A plain text keeps the font it was made with
    When the application font is set to:
      | pixelSize | 20 |
    Then the property "font.pointSize" of "plain" is 9

  Scenario: Qt.font starts from the new application font
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool again: false
        property font made: again ? Qt.font({ bold: true }) : Qt.font({ italic: true })
      }
      """
    When the application font is set to:
      | family    | Serif |
      | pixelSize | 20    |
    And the property "again" of "root" is set to true
    Then the property "made.family" of "root" is "Serif"
    And the property "made.pixelSize" of "root" is 20
    And the property "made.bold" of "root" is true

  Scenario: A plain text made after the change has the new font
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: host
        objectName: "host"
        property bool shown: false
        Loader { active: host.shown; sourceComponent: Text { objectName: "late"; text: "x" } }
      }
      """
    When the application font is set to:
      | family    | Serif |
      | pixelSize | 20    |
    And the property "shown" of "host" is set to true
    Then the property "font.family" of "late" is "Serif"
    And the property "font.pixelSize" of "late" is 20

  Scenario: A text made after the font is taken back has the default again
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: host
        objectName: "host"
        property bool shown: false
        Loader { active: host.shown; sourceComponent: Text { objectName: "late"; text: "x" } }
      }
      """
    When the application font is set to:
      | pixelSize | 20 |
    And the application font is set back to the default
    And the property "shown" of "host" is set to true
    Then the property "font.pointSize" of "late" is 9
    And the property "font.family" of "late" is "Sans Serif"

  Scenario: QML cannot assign the application font
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string error
        Component.onCompleted: {
          try { Qt.application.font = Qt.font({ pixelSize: 20 }) } catch (e) { error = e.name }
        }
      }
      """
    Then the property "error" of "root" is "TypeError"

  Scenario: A control made after the change has the new font
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        id: host
        objectName: "host"
        property bool shown: false
        Loader { active: host.shown; sourceComponent: Label { objectName: "late"; text: "x" } }
      }
      """
    When the application font is set to:
      | pixelSize | 20 |
    And the property "shown" of "host" is set to true
    Then the property "font.pixelSize" of "late" is 20

  Scenario: A style with a font of its own lays the application font's size over it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      ApplicationWindow {
        Button { objectName: "button"; text: "x" }
      }
      """
    When the application font is set to:
      | pixelSize | 20 |
    Then the property "font.pixelSize" of "button" is 20
