# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#font-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#palette-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#locale-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#palette-prop
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Handing the font, the colours and the locale of a control back
  A control takes its font, its palette and its locale from the control
  it is in until it is given its own. Assigning undefined takes that back:
  the control has what the one around it has again, and follows it.

  Rule: A colour taken back is the one from above again

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Pane {
          id: outer; objectName: "outer"
          width: 300; height: 300
          palette.button: "#aa0000"
          property bool given: true
          Button {
            objectName: "own"
            palette.button: "#00aa00"; palette.buttonText: "#0000aa"
            function colour() { palette.button = undefined }
            function words() { palette.buttonText = undefined }
          }
          Button {
            objectName: "bound"; y: 100
            palette.button: outer.given ? "#00aa00" : undefined
          }
        }
        """

    @api-QtQuick.Controls.Control.palette
    Scenario: The colour of the control around it
      When the method "colour" of "own" is called
      Then the property "palette.button" of "own" is "#aa0000"
      And the pixel at 30, 30 of the window is "#aa0000"

    Scenario: A colour that was taken back follows the control around it
      When the method "colour" of "own" is called
      And the property "palette.button" of "outer" is set to "#0000ff"
      Then the property "palette.button" of "own" is "#0000ff"

    Scenario: A colour nobody above gave is the style's
      When the method "words" of "own" is called
      Then the property "palette.buttonText" of "own" is "#26282a"

    Scenario: The other colours stay
      When the method "words" of "own" is called
      Then the property "palette.button" of "own" is "#00aa00"

    Scenario: A colour bound to undefined is the one from above for as long
      When the property "given" of "outer" is set to false
      Then the property "palette.button" of "bound" is "#aa0000"
      And the log has no warnings
      When the property "palette.button" of "outer" is set to "#0000ff"
      Then the property "palette.button" of "bound" is "#0000ff"
      When the property "given" of "outer" is set to true
      Then the property "palette.button" of "bound" is "#00aa00"

  Rule: Whatever has a palette hands a colour back

    Scenario Outline: The colour <role> of a <type> after undefined is <after>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Pane {
          width: 300; height: 300
          palette.<role>: "#aa0000"
          <type> {
            objectName: "c"
            palette.<role>: "#00aa00"
            function clear() { palette.<role> = undefined }
          }
        }
        """
      Then the property "palette.<role>" of "c" is "#00aa00"
      When the method "clear" of "c" is called
      Then the property "palette.<role>" of "c" is "<after>"

      Examples:
        | type      | role       | after   |
        | Label     | windowText | #aa0000 |
        | TextField | text       | #aa0000 |
        | TextArea  | base       | #aa0000 |
        | Pane      | window     | #aa0000 |
        | CheckBox  | mid        | #aa0000 |

    Scenario: A popup has the colour of its window again
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        ApplicationWindow {
          width: 300; height: 300; visible: true
          palette.button: "#aa0000"
          Popup {
            objectName: "over"
            palette.button: "#00aa00"
            function clear() { palette.button = undefined }
          }
        }
        """
      Then the property "palette.button" of "over" is "#00aa00"
      When the method "clear" of "over" is called
      Then the property "palette.button" of "over" is "#aa0000"

    Scenario: A window has the colour of the style again
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        ApplicationWindow {
          objectName: "root"
          width: 300; height: 300; visible: true
          palette.button: "#aa0000"
          function clear() { palette.button = undefined }
          Button { objectName: "inside" }
        }
        """
      Then the property "palette.button" of "inside" is "#aa0000"
      When the method "clear" of "root" is called
      Then the property "palette.button" of "root" is "#e0e0e0"
      And the property "palette.button" of "inside" is "#e0e0e0"

  Rule: A font taken back is the one from above again

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Pane {
          id: outer; objectName: "outer"
          width: 300; height: 300
          font.pixelSize: 20
          property bool given: true
          Button {
            objectName: "own"
            font.pixelSize: 30; font.italic: true
            function letters() { font = undefined }
          }
          Button {
            objectName: "bound"; y: 100
            font: outer.given ? Qt.font({pixelSize: 33}) : undefined
          }
          Button { objectName: "plain"; y: 200 }
        }
        """

    @api-QtQuick.Controls.Control.font
    Scenario: The font of the control around it
      When the method "letters" of "own" is called
      Then the property "font.pixelSize" of "own" is 20
      And the property "font.italic" of "own" is false

    Scenario: A font that was taken back follows the control around it
      When the method "letters" of "own" is called
      And the property "font.pixelSize" of "outer" is set to 11
      Then the property "font.pixelSize" of "own" is 11

    Scenario: A font that was taken back can be given again
      When the method "letters" of "own" is called
      And the property "font.italic" of "own" is set to true
      Then the property "font.italic" of "own" is true
      And the property "font.pixelSize" of "own" is 20

    Scenario: A font bound to undefined is the one from above for as long
      When the property "given" of "outer" is set to false
      Then the property "font.pixelSize" of "bound" is 20
      And the log has no warnings
      When the property "font.pixelSize" of "outer" is set to 11
      Then the property "font.pixelSize" of "bound" is 11
      When the property "given" of "outer" is set to true
      Then the property "font.pixelSize" of "bound" is 33

  Rule: A popup and a window hand their font back; a text does not

    Scenario: A popup has the font of its window again
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        ApplicationWindow {
          width: 300; height: 300; visible: true
          font.pixelSize: 20
          Popup {
            objectName: "over"
            font.pixelSize: 30
            function clear() { font = undefined }
          }
        }
        """
      Then the property "font.pixelSize" of "over" is 30
      When the method "clear" of "over" is called
      Then the property "font.pixelSize" of "over" is 20

    Scenario: A window has the font of the application again
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        ApplicationWindow {
          objectName: "root"
          width: 300; height: 300; visible: true
          font.italic: true
          function clear() { font = undefined }
          Button { objectName: "inside" }
        }
        """
      Then the property "font.italic" of "inside" is true
      When the method "clear" of "root" is called
      Then the property "font.italic" of "root" is false
      And the property "font.italic" of "inside" is false

    Scenario Outline: The font of a <type> refuses undefined
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        <type> {
          objectName: "c"
          property string said
          font.pixelSize: 30
          function clear() {
            try { font = undefined } catch (e) { said = String(e) }
          }
        }
        """
      When the method "clear" of "c" is called
      Then the property "said" of "c" is "Error: Cannot assign [undefined] to QFont"
      And the property "font.pixelSize" of "c" is 30

      Examples:
        | type      |
        | Label     |
        | TextField |
        | TextArea  |

  Rule: A locale taken back is the one from above again

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Pane {
          id: outer; objectName: "outer"
          width: 300; height: 300
          locale: Qt.locale("de_DE")
          property string speaks: own.locale.name
          property string inherits: plain.locale.name
          function other() { locale = Qt.locale("nb_NO") }
          Button {
            id: own; objectName: "own"
            locale: Qt.locale("fr_FR")
            function clear() { locale = undefined }
          }
          Button { id: plain }
        }
        """

    @api-QtQuick.Controls.Control.locale
    Scenario: A control has the locale of the control around it
      Then the property "inherits" of "outer" is "de_DE"
      And the property "speaks" of "outer" is "fr_FR"

    Scenario: The locale of the control around it
      When the method "clear" of "own" is called
      Then the property "speaks" of "outer" is "de_DE"

    Scenario: A locale that was taken back follows the control around it
      When the method "clear" of "own" is called
      And the method "other" of "outer" is called
      Then the property "speaks" of "outer" is "nb_NO"
      And the property "inherits" of "outer" is "nb_NO"
