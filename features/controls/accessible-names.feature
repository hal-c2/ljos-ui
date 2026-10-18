# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qtquickcontrols-accessibility.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html (text)
# The names of mnemonics and of controls without text were read from qml6
# (Qt 6) through AT-SPI. The Qt oracle cannot read default accessible names
# through QML alone, so these scenarios run on Ljós only.

@ljos-only @a11y
Feature: What a control's text gives its accessible name
  A button's name is its text without the ampersand that marks a mnemonic;
  a doubled ampersand is one. Text that is not a button's keeps its
  ampersands. A control without text has no name, whatever its contents or
  icon show.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        width: 400; height: 400
        Column {
          Button { text: "&Print" }
          Button { text: "Copy && Paste" }
          Button { text: "a&&&b" }
          Button { text: "Trail&" }
          CheckBox { text: "&Keep" }
          ItemDelegate { text: "&Open" }
          GroupBox { title: "&Wired" }
          Label { text: "&Note" }
          Switch { objectName: "bare" }
          ItemDelegate {
            objectName: "row"; width: 200; height: 40
            Row { Image { source: "rooms.svg" } Label { text: "Rooms" } }
          }
        }
      }
      """

  Scenario Outline: The ampersand of a mnemonic is not part of a button's name
    Then the accessibility tree has a "<role>" named "<name>"

    Examples:
      | role     | name         |
      | button   | Print        |
      | button   | Copy & Paste |
      | button   | a&b          |
      | button   | Trail&       |
      | checkbox | Keep         |
      | listitem | Open         |
      | group    | &Wired       |
      | label    | &Note        |

  Scenario: A control without text has no name, whatever it shows
    Then the object "bare" is exposed as a "switch" named ""
    And the object "row" is exposed as a "listitem" named ""
    And the "label" named "Rooms" is inside the "listitem" named ""
