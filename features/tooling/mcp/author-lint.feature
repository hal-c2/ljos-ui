# Sources:
#   docs/ARCHITECTURE.md sections 14 (accessibility), 15 (translation) and 19.3
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qtquick-internationalization.html
# The authoring tools and `ljos check` are Ljós's own: Qt has nothing to compare them with.
# A file given here is written without the spaces its doc string is indented by, so the
# first column of a line is 1. The rules need the table of Qt's types (`mise run types`).

@ljos-only @headless
Feature: Linting QML with the authoring tools
  `qml_lint` tells an agent what a file leaves an assistive technology or
  a translator without. The rules are the ones `ljos check` applies, under
  the same names and at the same places, so what an agent fixes is what
  the command line stops reporting.

  Background:
    Given the authoring tools are connected
    And the file "Main.qml":
      """qml
      import QtQuick
      import QtQuick.Controls

      Item {
          property string greeting: "hello"
          Rectangle {
              MouseArea {
                  anchors.fill: parent
                  onClicked: greeting = "pressed"
              }
          }
          Button {
              icon.source: "save.png"
          }
          Image {
              source: "logo.png"
          }
          Text {
              text: qsTr(greeting)
          }
          Label {
              text: "Hello"
          }
      }
      """

  Rule: What has no name, and what cannot be translated, is found

    Scenario Outline: Each finding has a code, a place and a severity
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/<nth>/code" is "<code>"
      And the answer's "/diagnostics/<nth>/line" is <line>
      And the answer's "/diagnostics/<nth>/column" is <column>
      And the answer's "/diagnostics/<nth>/severity" is "<severity>"

      Examples:
        | nth | code                        | line | column | severity |
        | 0   | a11y-unnamed-interactive    | 7    | 9      | error    |
        | 1   | focus-pointer-only          | 9    | 13     | warning  |
        | 2   | a11y-unnamed-icon-button    | 12   | 5      | error    |
        | 3   | a11y-icon-button-no-tooltip | 13   | 9      | warning  |
        | 4   | a11y-unnamed-image          | 15   | 5      | warning  |
        | 5   | i18n-qstr-non-literal       | 19   | 15     | warning  |
        | 6   | i18n-untranslated-string    | 22   | 9      | hint     |

    Scenario: A finding says what to write
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/4/message" matches "Accessible\.ignored: true"

    Scenario: A file with nothing to find has no findings
      Given the file "Named.qml":
        """qml
        import QtQuick

        Image {
            source: "logo.png"
            Accessible.name: qsTr("The logo")
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Named.qml"]}
        """
      Then the answer's "/diagnostics" is []

  Rule: The command line applies the same rules

    Scenario: The findings a reader pays for are reported in the same places
      When the author runs "ljos check ."
      Then the command's output mentions "Main.qml:7:9: error:"
      And the command's output mentions "[a11y-unnamed-interactive]"
      And the command's output mentions "Main.qml:12:5: error:"
      And the command's output mentions "[a11y-unnamed-icon-button]"
      And the command's output mentions "Main.qml:15:5: warning:"
      And the command's output mentions "[a11y-unnamed-image]"
      And the command's output mentions "Main.qml:19:15: warning:"
      And the command's output mentions "[i18n-qstr-non-literal]"

    Scenario: What has no name fails the check, and the rest is reported
      When the author runs "ljos check ."
      Then the command exits with status 1
      And the command's output mentions "1 files, 2 errors, 4 warnings"

    Scenario: A text that is not translated is a hint for the agent alone
      When the author runs "ljos check ."
      Then the command's output does not mention "i18n-untranslated-string"

  Rule: The rules can be read

    Scenario: Each rule is explained under its code
      When the agent reads the resource "ljos://lints"
      Then the resource contains "a11y-unnamed-interactive"
      And the resource contains "a11y-unnamed-icon-button"
      And the resource contains "a11y-unnamed-image"
      And the resource contains "i18n-qstr-non-literal"
      And the resource contains "i18n-untranslated-string"

    Scenario: The explanation says which rules the command line applies
      When the agent reads the resource "ljos://lints"
      Then the resource contains "ljos check"
