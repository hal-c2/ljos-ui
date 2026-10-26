# Sources:
#   docs/design/motion.md, docs/design/accessibility.md, docs/design/content.md,
#   docs/design/layout.md, docs/design/interaction.md
#   crates/ljos_design (the rules, their severities and limits)
# The design lints are Ljós's own: Qt has nothing to compare them with.
# A file given here is written without the spaces its doc string is indented by, so the
# first column of a line is 1.

@ljos-only @headless
Feature: More design rules a file's source can show
  Motion that flashes or never ends, a field that only its placeholder
  names, a button whose text says nothing, and the like are told by the
  source alone. The check reports each under the rule's code, an error
  fails it, and a warning is kept with a reason in an `ljos-allow` comment.

  Background:
    Given the authoring tools are connected

  Rule: Motion that flashes is an error

    Scenario: A colour that strobes fails the check
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Item {
            width: 200; height: 80
            Rectangle {
                width: 40; height: 40
                color: palette.window
                SequentialAnimation on color { loops: Animation.Infinite
                    ColorAnimation { to: palette.highlight; duration: 120 }
                    ColorAnimation { to: palette.window; duration: 120 }
                }
            }
        }
        """
      When the author runs "ljos check ."
      Then the command exits with status 1
      And the command's output mentions "Main.qml:9:9: error:"
      And the command's output mentions "[motion-flash]"

    Scenario: The same colour changing under three times a second passes
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Item {
            width: 200; height: 80
            Rectangle {
                width: 40; height: 40
                color: palette.window
                SequentialAnimation on color { loops: 3
                    ColorAnimation { to: palette.highlight; duration: 400 }
                    ColorAnimation { to: palette.window; duration: 400 }
                }
            }
        }
        """
      When the author runs "ljos check ."
      Then the command exits with status 0
      And the command's output does not mention "[motion-flash]"

  Rule: Motion that never stops is a warning

    Background:
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Item {
            width: 120; height: 40
            Rectangle {
                width: 100; height: 12
                color: palette.mid
                SequentialAnimation on opacity { loops: Animation.Infinite
                    NumberAnimation { to: 0.5; duration: 1000 }
                    NumberAnimation { to: 1; duration: 1000 }
                }
            }
        }
        """

    Scenario: A fade that loops for ever is a warning
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/0/code" is "motion-endless"
      And the answer's "/diagnostics/0/severity" is "warning"
      And the answer's "/diagnostics/0/line" is 9

    Scenario: An allow with a reason keeps the warning
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Item {
            width: 120; height: 40
            // ljos-allow(motion-endless): the placeholder shows only while the first load runs
            Rectangle {
                width: 100; height: 12
                color: palette.mid
                SequentialAnimation on opacity { loops: Animation.Infinite
                    NumberAnimation { to: 0.5; duration: 1000 }
                    NumberAnimation { to: 1; duration: 1000 }
                }
            }
        }
        """
      When the author runs "ljos check ."
      Then the command exits with status 0
      And the command's output mentions "1 files, 0 errors, 0 warnings"

    Scenario: A busy indicator is the message and is left alone
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        BusyIndicator {
            running: true
            contentItem: Item {
                Rectangle {
                    width: 8; height: 8
                    color: palette.highlight
                    RotationAnimator on rotation { from: 0; to: 360; duration: 1000; loops: Animation.Infinite }
                }
            }
        }
        """
      When the author runs "ljos check ."
      Then the command's output does not mention "[motion-endless]"
      And the command's output does not mention "[motion-reduced-ignored]"

  Rule: Movement that ignores the reduced motion setting is a hint

    Scenario: A spinning decoration with nothing reading the preference
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            width: 80; height: 80
            Rectangle {
                width: 20; height: 20
                RotationAnimator on rotation { from: 0; to: 360; duration: 2000; loops: Animation.Infinite }
            }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/0/code" is "motion-reduced-ignored"
      And the answer's "/diagnostics/0/severity" is "hint"

  Rule: A field that only its placeholder names is a warning

    Scenario: A search field with a placeholder and nothing else
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        import QtQuick.Layouts

        ColumnLayout {
            width: 240
            TextField { placeholderText: qsTr("Search") }
            Button { text: qsTr("Find") }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/0/code" is "a11y-placeholder-label"
      And the answer's "/diagnostics/0/severity" is "warning"
      And the answer's "/diagnostics/0/line" is 7

    Scenario Outline: A name, or a label beside the field, answers the rule
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        import QtQuick.Layouts

        ColumnLayout {
            width: 240
            <before>
            TextField { placeholderText: qsTr("Search"); <naming> }
            Button { text: qsTr("Find") }
        }
        """
      When the author runs "ljos check ."
      Then the command's output does not mention "[a11y-placeholder-label]"

      Examples:
        | before                      | naming                          |
        | Label { text: qsTr("Find") } | Layout.fillWidth: true          |
        | Item {}                     | Accessible.name: qsTr("Search") |

  Rule: A button's text says what it does

    Scenario: A button that says only OK
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Item {
            width: 200; height: 80
            Button { text: qsTr("&OK"); onClicked: console.log("saved") }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/0/code" is "content-vague-action"
      And the answer's "/diagnostics/0/severity" is "hint"
      And the answer's "/diagnostics/0/line" is 6

    Scenario: The standard buttons of a dialog are exempt
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Item {
            width: 200; height: 80
            Dialog { standardButtons: Dialog.Ok | Dialog.Cancel }
        }
        """
      When the author runs "ljos check ."
      Then the command's output does not mention "[content-vague-action]"

  Rule: A menu entry that opens a dialog ends in an ellipsis

    Scenario: Rename opens a dialog and says so
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Item {
            width: 200; height: 200
            Dialog { id: renameDialog; title: qsTr("Rename"); standardButtons: Dialog.Ok | Dialog.Cancel }
            Menu {
                MenuItem { text: qsTr("Rename"); onTriggered: renameDialog.open() }
            }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/0/code" is "content-menu-ellipsis"
      And the answer's "/diagnostics/0/severity" is "hint"
      And the answer's "/diagnostics/0/line" is 8

  Rule: A destructive action asks first or can be undone

    Scenario: Delete at once
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Item {
            width: 200; height: 80
            Button { text: qsTr("Delete project"); onClicked: console.log("deleted") }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/0/code" is "focus-destructive-undo"
      And the answer's "/diagnostics/0/line" is 6

  Rule: A popup grows from where it opened, and corners nest

    Scenario: A menu that scales about its centre
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Item {
            width: 200; height: 200
            Menu {
                enter: Transition {
                    NumberAnimation { property: "scale"; from: 0.9; to: 1; duration: 150; easing.type: Easing.OutCubic }
                }
                MenuItem { text: qsTr("Cut") }
            }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/0/code" is "motion-origin"
      And the answer's "/diagnostics/0/line" is 7

    Scenario: A card whose corners are rounder than its frame
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Rectangle {
            width: 200; height: 100
            radius: 6
            color: palette.window
            Rectangle {
                anchors.fill: parent
                radius: 12
                color: palette.base
            }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/0/code" is "layout-nested-radius"
      And the answer's "/diagnostics/0/line" is 10
