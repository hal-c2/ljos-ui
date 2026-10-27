# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html#text-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubar.html
# Verified on Qt 6.11.2 (qml6, offscreen, Basic style).

@qt-parity
Feature: Mnemonics in the text of a control
  An "&" in the text of a button, menu title or menu item marks the letter
  after it as the mnemonic: the "&" is not drawn, and "&&" is one "&".
  The text property keeps what was written. The letter is drawn
  underlined, before Alt is pressed too. Alt with the letter activates a
  button, and opens the menu of a menu bar.

  Scenario Outline: The marker is not drawn
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        width: 300; height: 200; visible: true
        Button { id: written; text: "<written>" }
        Button { id: drawn; text: "<drawn>" }
        Item {
          objectName: "o"
          property bool same: written.contentItem.implicitWidth === drawn.contentItem.implicitWidth
          property string kept: written.text
        }
      }
      """
    Then the property "same" of "o" is true
    And the property "kept" of "o" is "<written>"

    Examples:
      | written  | drawn   |
      | &File    | File    |
      | Save &as | Save as |
      | a&&b c&d | a&&b cd |

  # Qt draws one "&" less than there are in a run: three draw as two.
  Scenario: Three ampersands draw as two
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        width: 300; height: 200; visible: true
        Button { id: three; text: "a&&&b" }
        Button { id: two; text: "a&&b" }
        Item {
          objectName: "o"
          property bool wider: three.contentItem.implicitWidth > two.contentItem.implicitWidth
        }
      }
      """
    Then the property "wider" of "o" is true

  Scenario: A doubled ampersand is drawn
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        width: 300; height: 200; visible: true
        Button { id: written; text: "A&&B" }
        Button { id: plain; text: "AB" }
        Item {
          objectName: "o"
          property bool wider: written.contentItem.implicitWidth > plain.contentItem.implicitWidth
        }
      }
      """
    Then the property "wider" of "o" is true

  Scenario: A menu title in a menu bar is drawn without the marker
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        width: 300; height: 200; visible: true
        MenuBar {
          id: bar
          Menu { title: "&File" }
        }
        MenuBar {
          id: plain
          y: 50
          Menu { title: "File" }
        }
        Item {
          objectName: "o"
          property bool same: bar.count > 0 && plain.count > 0 && bar.itemAt(0).contentItem.implicitWidth === plain.itemAt(0).contentItem.implicitWidth
          property string kept: bar.count > 0 ? bar.itemAt(0).text : ""
        }
      }
      """
    Then the property "same" of "o" is true
    And the property "kept" of "o" is "&File"

  # qml6 underlines the letter in a button, a menu bar's title and a menu
  # item alike; a check box draws its "&".
  Scenario: The mnemonic letter is drawn underlined
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 120; height: 60
        color: "#ffffff"
        Button { padding: 0; background: null; font.pixelSize: 40; text: "&H" }
        Button { x: 60; padding: 0; background: null; font.pixelSize: 40; text: "H" }
      }
      """
    Then the pixel at 16, 39 of the window is within 120 of "#000000"
    And the pixel at 76, 39 of the window is within 10 of "#ffffff"

  # Only the first "&" that is dropped marks a character, a space as well
  # as a letter; when an "&" follows it nothing is underlined.
  Scenario Outline: Which character is drawn underlined
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 200; height: 60
        color: "#ffffff"
        Button {
          anchors.<side>: parent.<side>
          padding: 0; background: null; font.pixelSize: 40; text: "<text>"
        }
      }
      """
    Then the pixel at <x>, 39 of the window is within 120 of "<colour>"

    Examples:
      | text     | side  | x   | colour  |
      | & H      | left  | 4   | #000000 |
      | H        | left  | 4   | #ffffff |
      | & &H     | right | 186 | #ffffff |
      | a&&&H    | right | 186 | #ffffff |
      | a&&&b &H | right | 186 | #ffffff |

  Scenario: Alt and the letter clicks a button
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        width: 300; height: 200; visible: true
        Button { objectName: "b"; text: "&File"; property int hits: 0; onClicked: hits++ }
      }
      """
    When the user presses "Alt+F"
    Then the property "hits" of "b" is 1

  Scenario: Alt and the letter without the mnemonic does nothing
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        width: 300; height: 200; visible: true
        Button { objectName: "b"; text: "&File"; property int hits: 0; onClicked: hits++ }
      }
      """
    When the user presses "Alt+G"
    Then the property "hits" of "b" is 0

  # The keys read the ampersands in pairs, so "a&&&b" has "b" although
  # nothing is drawn underlined, and a space after the first "&" is the
  # mnemonic even when a letter follows a later "&".
  Scenario Outline: Which key clicks a button
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        width: 300; height: 200; visible: true
        Button { objectName: "b"; text: "<text>"; property int hits: 0; onClicked: hits++ }
      }
      """
    When the user presses "<keys>"
    Then the property "hits" of "b" is <hits>

    Examples:
      | text    | keys      | hits |
      | a&&&b   | Alt+B     | 1    |
      | a&&&b   | Alt+A     | 0    |
      | a& b    | Alt+Space | 1    |
      | a& b    | Alt+B     | 0    |
      | a& b &c | Alt+C     | 0    |
      | ab      | Alt+Space | 0    |

  Scenario: A mnemonic given to a button later clicks it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        objectName: "w"; width: 300; height: 200; visible: true
        property string name: "Go"
        Button { objectName: "b"; text: "File"; property int hits: 0; onClicked: hits++ }
        Button { objectName: "c"; text: name; property int hits: 0; onClicked: hits++ }
      }
      """
    When the property "text" of "b" is set to "&File"
    And the property "name" of "w" is set to "&Go"
    And the user presses "Alt+F"
    And the user presses "Alt+G"
    Then the property "hits" of "b" is 1
    And the property "hits" of "c" is 1

  Scenario Outline: Alt and the letter opens the menu of the bar
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        width: 300; height: 200; visible: true
        MenuBar {
          Menu { id: file; title: "&File" }
          Menu { id: edit; title: "&Edit" }
        }
        Item {
          objectName: "o"
          property bool fileOpen: file.visible
          property bool editOpen: edit.visible
        }
      }
      """
    When the user presses "<keys>"
    Then the property "fileOpen" of "o" is <file>
    And the property "editOpen" of "o" is <edit>

    Examples:
      | keys  | file  | edit  |
      | Alt+F | true  | false |
      | Alt+E | false | true  |
      | F     | false | false |
