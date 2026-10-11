# Sources:
#   docs/ARCHITECTURE.md sections 19.3, 19.6 and 19.7
#   https://doc.qt.io/qt-6/qml-codingconventions.html
#   https://doc.qt.io/qt-6/qtqml-syntax-basics.html
# The authoring tools are a Ljós extension: Qt has nothing to compare them with.

@ljos-only @headless
Feature: Formatting QML with the authoring tools
  An agent that writes QML asks `qml_format` to lay it out. The layout is
  the same whoever asks and however often: one member a line, four spaces a
  level, the author's comments and groups of lines kept, and the JavaScript
  of a binding left as it was written. A file is changed only when the
  agent was started with the right to write.

  Rule: The layout

    Background:
      Given the authoring tools are connected with the capabilities "fs"

    Scenario: Each member gets a line of its own, four spaces in
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item { width: 10; height: 20
          Text{text:"hi"}
        }
        """
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Main.qml"], "write": true}
        """
      Then the answer's "/files/0/status" is "written"
      And the file "Main.qml" is:
        """qml
        import QtQuick

        Item {
            width: 10
            height: 20
            Text {
                text: "hi"
            }
        }
        """

    Scenario: An object with nothing in it stays on one line
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            Rectangle {
            }
        }
        """
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Main.qml"], "write": true}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick

        Item {
            Rectangle {}
        }
        """

    Scenario: Groups of lines stay groups, and no line is left empty at an edge
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {

            width: 10
            height: 20



            Button { text: "Go" }

        }
        """
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Main.qml"], "write": true}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick
        import QtQuick.Controls

        Item {
            width: 10
            height: 20

            Button {
                text: "Go"
            }
        }
        """

    Scenario: Comments stay where the author put them
      Given the file "Main.qml":
        """qml
        // A box.
        import QtQuick
        Item {
          // how wide
          width: 10   // in pixels
          /* and
             how tall */
          height: 20
        }
        """
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Main.qml"], "write": true}
        """
      Then the file "Main.qml" is:
        """qml
        // A box.
        import QtQuick

        Item {
            // how wide
            width: 10 // in pixels
            /* and
               how tall */
            height: 20
        }
        """

    Scenario: Pragmas and imports keep their order and their words
      Given the file "Theme.qml":
        """qml
        pragma Singleton
        pragma ComponentBehavior:Bound
        import QtQuick 2.15
        import QtQuick.Controls   as   C
        import "parts"
        QtObject { property color ink: "black" }
        """
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Theme.qml"], "write": true}
        """
      Then the file "Theme.qml" is:
        """qml
        pragma Singleton
        pragma ComponentBehavior: Bound
        import QtQuick 2.15
        import QtQuick.Controls as C
        import "parts"

        QtObject {
            property color ink: "black"
        }
        """

    Scenario: The JavaScript of a handler moves with it and is not rewritten
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          property int taps;
          signal tapped();
          onTapped: {
              if (taps>2)   taps = 0
              else
                  taps += 1
          }
          function reset(to) { taps=to }
        }
        """
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Main.qml"], "write": true}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick

        Item {
            property int taps
            signal tapped()
            onTapped: {
                if (taps>2)   taps = 0
                else
                    taps += 1
            }
            function reset(to) { taps=to }
        }
        """

    Scenario: Text that runs over lines inside a binding is not touched
      Given the file "Main.qml":
        """qml
        import QtQuick
        Text {
          text: `one
          two`
        }
        """
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Main.qml"], "write": true}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick

        Text {
            text: `one
          two`
        }
        """

    Scenario: A list of objects gets a line for each
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            states: [ State { name: "on" }, State { name: "off" } ]
            transitions: []
        }
        """
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Main.qml"], "write": true}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick

        Item {
            states: [
                State {
                    name: "on"
                },
                State {
                    name: "off"
                }
            ]
            transitions: []
        }
        """

    Scenario: Formatting what is formatted changes nothing
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item { width: 10; Text { text: "hi" } // greeting
        }
        """
      And the agent has called the tool "qml_format" with:
        """json
        {"paths": ["Main.qml"], "write": true}
        """
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Main.qml"], "write": true}
        """
      Then the answer's "/files/0/status" is "unchanged"
      And the answer's "/changed" is 0

    Scenario: Text the agent has not saved yet is formatted and handed back
      When the agent calls the tool "qml_format" with:
        """json
        {"source": "import QtQuick\nItem{width:10}\n"}
        """
      Then the answer's "/formatted" is:
        """qml
        import QtQuick

        Item {
            width: 10
        }
        """

  Rule: A file that does not parse is refused

    Background:
      Given the authoring tools are connected with the capabilities "fs"
      And the file "Broken.qml":
        """qml
        import QtQuick
        Item {
            width: 10 +
        }
        """

    Scenario: The refusal says where the mistake is
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Broken.qml"], "write": true}
        """
      Then the answer's "/files/0/status" is "refused"
      And the answer's "/files/0/diagnostics/0/line" is 4
      And the answer's "/files/0/diagnostics/0/severity" is "error"
      And the answer's "/refused" is 1

    Scenario: A refused file is left as it was
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Broken.qml"], "write": true}
        """
      Then the file "Broken.qml" is:
        """qml
        import QtQuick
        Item {
            width: 10 +
        }
        """

  Rule: Without the right to write, the agent is shown what would change

    Background:
      Given the authoring tools are connected
      And the file "Main.qml":
        """qml
        import QtQuick
        Item { width: 10 }
        """

    Scenario: The change comes back as a diff
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/files/0/status" is "changed"
      And the answer's "/files/0/diff" is:
        """text
        --- a/Main.qml
        +++ b/Main.qml
        @@ -1,2 +1,5 @@
         import QtQuick
        -Item { width: 10 }
        +
        +Item {
        +    width: 10
        +}
        """

    Scenario: The file is not changed
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick
        Item { width: 10 }
        """

    Scenario: Asking to write is refused and names what is missing
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["Main.qml"], "write": true}
        """
      Then the agent is refused with "capability_denied"
      And the refusal mentions "fs"
      And the file "Main.qml" is:
        """qml
        import QtQuick
        Item { width: 10 }
        """

    Scenario: A file outside the project is refused
      When the agent calls the tool "qml_format" with:
        """json
        {"paths": ["../Other.qml"]}
        """
      Then the agent is refused with "path_denied"
