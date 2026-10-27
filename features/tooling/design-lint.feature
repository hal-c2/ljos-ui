# Sources:
#   docs/design/README.md (severity, `ljos-allow`), docs/design/motion.md,
#   docs/design/accessibility.md
#   crates/ljos_design (the rules, their severities and limits)
# The design lints are Ljós's own: Qt has nothing to compare them with.
# A file given here is written without the spaces its doc string is indented by, so the
# first column of a line is 1.

@ljos-only @headless
Feature: The design rules a file's source can show
  `ljos check` and the authoring tool `qml_lint` report each design rule
  the source alone can tell, under the rule's code, with what to write
  instead and a link to its guideline. An error is certain and fails the
  check; a warning is kept with a reason in an `ljos-allow` comment; a
  hint is advice for the author and the agent alone.

  Background:
    Given the authoring tools are connected
    And the file "Main.qml":
      """qml
      import QtQuick
      import QtQuick.Controls

      Item {
          Image {
              source: "logo.png"
          }
          Popup {
              enter: Transition {
                  NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 150; easing.type: Easing.OutBounce }
              }
          }
      }
      """

  Rule: A finding names its rule, what to write and where it is explained

    Scenario: The agent is told the code, the severity, the fix and the guideline
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/1/code" is "motion-bounce"
      And the answer's "/diagnostics/1/line" is 10
      And the answer's "/diagnostics/1/severity" is "warning"
      And the answer's "/diagnostics/1/fix" matches "critically damped spring"
      And the answer's "/diagnostics/1/guide" is "docs/design/motion.md#motion-bounce"

    Scenario: The command line gives the guideline with each finding
      When the author runs "ljos check ."
      Then the command's output mentions "(docs/design/motion.md#motion-bounce) [motion-bounce]"
      And the command's output mentions "(docs/design/accessibility.md#a11y-unnamed-image) [a11y-unnamed-image]"

    Scenario: Each rule's guideline can be read from its code
      When the agent reads the resource "ljos://lints"
      Then the resource contains "docs/design/motion.md#motion-ease-in-enter"
      And the resource contains "docs/design/color.md#color-text-contrast"
      And the resource contains "docs/design/README.md#lint-allow-without-reason"

  Rule: The severity decides whether the check fails

    Scenario: Warnings alone do not fail the check
      When the author runs "ljos check ."
      Then the command exits with status 0
      And the command's output mentions "1 files, 0 errors, 2 warnings"

    Scenario: An error fails the check
      Given the file "Main.qml":
        """qml
        import QtQuick

        Rectangle {
            MouseArea {
                anchors.fill: parent
                Keys.onReturnPressed: console.log("pressed")
                onClicked: console.log("pressed")
            }
        }
        """
      When the author runs "ljos check ."
      Then the command exits with status 1
      And the command's output mentions "Main.qml:4:5: error:"
      And the command's output mentions "[a11y-unnamed-interactive]"

    Scenario: Code that is not one's own is reported with nothing failing
      Given the file "Main.qml":
        """qml
        import QtQuick

        Rectangle {
            MouseArea {
                anchors.fill: parent
                Keys.onReturnPressed: console.log("pressed")
                onClicked: console.log("pressed")
            }
        }
        """
      When the author runs "ljos check --lint=warn ."
      Then the command exits with status 0
      And the command's output mentions "Main.qml:4:5: warning:"

  Rule: A warning is kept with its reason written beside it

    Scenario: An allow with a reason keeps the warning of the object it opens
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Item {
            Image {
                source: "logo.png"
                Accessible.ignored: true
            }
            // ljos-allow(motion-bounce): the popup lands once, on the first run only
            Popup {
                enter: Transition {
                    NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 150; easing.type: Easing.OutBounce }
                }
            }
        }
        """
      When the author runs "ljos check ."
      Then the command exits with status 0
      And the command's output mentions "1 files, 0 errors, 0 warnings"

    Scenario: An allow without a reason is an error
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls

        Item {
            // ljos-allow(motion-bounce)
            Popup {
                enter: Transition {
                    NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 150; easing.type: Easing.OutBounce }
                }
            }
        }
        """
      When the author runs "ljos check ."
      Then the command exits with status 1
      And the command's output mentions "Main.qml:5:5: error:"
      And the command's output mentions "[lint-allow-without-reason]"

    Scenario: An allow that keeps nothing is a hint
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            // ljos-allow(motion-stagger): the list no longer staggers
            Rectangle {
                width: 10
                height: 10
            }
        }
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics/0/code" is "lint-allow-unused"
      And the answer's "/diagnostics/0/severity" is "hint"
      And the answer's "/diagnostics/0/line" is 4

  Rule: An object has what its component's own definition sets

    Scenario: A text component that wraps in its definition is not told to wrap where it is used
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            id: root
            width: 200
            property string name: "x"

            component Wrapped: Text {
                wrapMode: Text.Wrap
            }

            Wrapped {
                width: root.width
                text: root.name
            }
        }
        """
      When the author runs "ljos check ."
      Then the command exits with status 0
      And the command's output does not mention "text-unbounded-dynamic"

    Scenario: A text component of its own file that elides is not told to where it is used
      Given the file "Elided.qml":
        """qml
        import QtQuick

        Text {
            elide: Text.ElideRight
        }
        """
      And the file "Main.qml":
        """qml
        import QtQuick

        Item {
            id: root
            width: 200
            property string name: "x"

            Elided {
                width: root.width
                text: root.name
            }
        }
        """
      When the author runs "ljos check ."
      Then the command exits with status 0
      And the command's output does not mention "text-unbounded-dynamic"

    Scenario: A text component that neither wraps nor elides is told to where it is used
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            id: root
            width: 200
            property string name: "x"

            component Plain: Text {
                color: "black"
            }

            Plain {
                width: root.width
                text: root.name
            }
        }
        """
      When the author runs "ljos check ."
      Then the command's output mentions "Main.qml:14:9: warning:"
      And the command's output mentions "[text-unbounded-dynamic]"

  Rule: A text is named by what it shows

    Scenario: A tap that follows a link in a text needs no name besides the text
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            id: root
            width: 300
            height: 200
            property string page: "<p>See <a href=\"https://example.com\">the docs</a>.</p>"

            Text {
                id: body
                width: root.width
                text: root.page
                textFormat: Text.RichText
                wrapMode: Text.Wrap

                TapHandler {
                    acceptedDevices: PointerDevice.TouchScreen
                    onTapped: eventPoint => {
                        const link = body.linkAt(eventPoint.position.x, eventPoint.position.y);
                        if (link.length > 0)
                            Qt.openUrlExternally(link);
                    }
                }
            }
        }
        """
      When the author runs "ljos check ."
      Then the command exits with status 0
      And the command's output does not mention "a11y-unnamed-interactive"

    Scenario: A tap in a text that shows nothing still needs a name
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            Text {
                width: 100
                height: 20
                TapHandler {
                    onTapped: console.log("tapped")
                }
            }
        }
        """
      When the author runs "ljos check ."
      Then the command exits with status 1
      And the command's output mentions "Main.qml:7:9: error:"
      And the command's output mentions "[a11y-unnamed-interactive]"
