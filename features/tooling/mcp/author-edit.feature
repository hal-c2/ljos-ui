# Sources:
#   docs/ARCHITECTURE.md section 19.3
#   docs/adr/0003-blocks-and-shells.md
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
# The authoring tools are a Ljós extension: Qt has nothing to compare them with.

@ljos-only @headless
Feature: Editing QML with the authoring tools
  An agent or a visual designer that changes one thing in a file asks
  `qml_edit_apply` for that change: a binding set or taken out, an object
  put in, moved or taken out. Every byte outside the member it changes stays
  as the author wrote it, comments included. An object is named by its `id`
  or by its path from the root object. The answer is the splices of the
  text; the file is written only when the agent asks and may.

  Rule: An edit changes only what it names

    Background:
      Given the authoring tools are connected with the capabilities "fs"

    Scenario: Setting a binding keeps the comments around it
      Given the file "Main.qml":
        """qml
        import QtQuick

        // The page header.
        Item {
            id: header
            width: 100 // fits the title
            height:   40
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "set_binding", "object": {"id": "header"}, "name": "height", "value": "48"}}
        """
      Then the answer's "/written" is true
      And the file "Main.qml" is:
        """qml
        import QtQuick

        // The page header.
        Item {
            id: header
            width: 100 // fits the title
            height:   48
        }
        """

    Scenario: A dotted name finds the binding in its group
      Given the file "Main.qml":
        """qml
        import QtQuick

        Text {
            id: title
            font {
                pixelSize: 12
            }
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "set_binding", "object": {"id": "title"}, "name": "font.bold", "value": "true"}}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick

        Text {
            id: title
            font {
                pixelSize: 12
                bold: true
            }
        }
        """

    Scenario: A new binding is written dotted where the file writes them dotted
      Given the file "Main.qml":
        """qml
        import QtQuick

        Text {
            font.pixelSize: 12
            text: "Hi"
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "set_binding", "object": {"path": "root"}, "name": "font.bold", "value": "true"}}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick

        Text {
            font.pixelSize: 12
            font.bold: true
            text: "Hi"
        }
        """

    Scenario: An object of several lines put into a one-line object opens it up
      Given the file "Main.qml":
        """qml
        import QtQuick

        Row { id: bar; spacing: 4; Text { text: "a" } }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "insert_object", "parent": {"id": "bar"}, "index": 0, "text": "Text {\n    text: \"b\"\n}"}}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick

        Row {
            id: bar
            spacing: 4
            Text {
                text: "b"
            }
            Text { text: "a" }
        }
        """

    Scenario: A moved object takes the comment above it along
      Given the file "Main.qml":
        """qml
        import QtQuick

        Column {
            // Says hello.
            Text { text: "Hello" }
            Rectangle {
                id: box
            }
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "move_object", "object": {"path": "root/0"}, "parent": {"id": "box"}, "index": 0}}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick

        Column {
            Rectangle {
                id: box
                // Says hello.
                Text { text: "Hello" }
            }
        }
        """

    Scenario: A removed object leaves its neighbours as they were
      Given the file "Main.qml":
        """qml
        import QtQuick

        Column {
            Text { text: "one" } // first
            // The second.
            Text { text: "two" }
            Text { text: "three" }
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "remove_object", "object": {"path": "root/1"}}}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick

        Column {
            Text { text: "one" } // first
            Text { text: "three" }
        }
        """

  Rule: Without `write`, the agent is shown what would change

    Scenario: A dry run answers the splices and the text, and writes nothing
      Given the authoring tools are connected
      And the file "Main.qml":
        """qml
        import QtQuick

        Item {
            width: 10
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "edit": {"op": "set_binding", "object": {"path": "root"}, "name": "width", "value": "20"}}
        """
      Then the answer's "/written" is false
      And the answer's "/changes/0/replacement" is "20"
      And the answer's "/text" is:
        """qml
        import QtQuick

        Item {
            width: 20
        }
        """
      And the file "Main.qml" is:
        """qml
        import QtQuick

        Item {
            width: 10
        }
        """

    Scenario: Asking to write without the right to is refused and names what is missing
      Given the authoring tools are connected
      And the file "Main.qml":
        """qml
        import QtQuick
        Item { width: 10 }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "remove_binding", "object": {"path": "root"}, "name": "width"}}
        """
      Then the agent is refused with "capability_denied"
      And the refusal mentions "fs"

  Rule: An edit that cannot be made cleanly is refused, and the file is left alone

    Background:
      Given the authoring tools are connected with the capabilities "fs"

    Scenario: An edit that would leave text that does not parse is refused
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item { width: 10 }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "set_binding", "object": {"path": "root"}, "name": "width", "value": "10 +"}}
        """
      Then the agent is refused with "would_break"
      And the file "Main.qml" is:
        """qml
        import QtQuick
        Item { width: 10 }
        """

    Scenario: A file that does not parse is refused
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item { width: }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "set_binding", "object": {"path": "root"}, "name": "height", "value": "5"}}
        """
      Then the agent is refused with "syntax"

    Scenario: An address that names nothing in the file is not found
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item { Text {} }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "remove_object", "object": {"path": "root/3"}}}
        """
      Then the agent is refused with "not_found"
      And the refusal mentions "root/3"
