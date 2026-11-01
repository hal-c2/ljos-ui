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

    Scenario: An object put first among the children stays below the blank line that ends the bindings
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            width: 200

            Rectangle { color: "red" }
            Text { text: "hello" }
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "insert_object", "parent": {"path": "root"}, "index": 0, "text": "Item {}"}}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick

        Item {
            width: 200

            Item {}
            Rectangle { color: "red" }
            Text { text: "hello" }
        }
        """

    Scenario: An object put among children with blank lines between them gets one too
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            width: 200

            Rectangle { color: "red" }

            Text { text: "hello" }
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "insert_object", "parent": {"path": "root"}, "index": 0, "text": "Item {}"}}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick

        Item {
            width: 200

            Item {}

            Rectangle { color: "red" }

            Text { text: "hello" }
        }
        """

  # qml6 refuses to load a file that gives one name two values, in any of
  # the forms these scenarios start from: `property int a: 1` with `a: 2`,
  # `f { }` or `f.bold: x` with `f: y`, `f: x` with `f.bold: y`, and
  # `o: Item { }` with `o.width: 5`. It loads an alias with a binding of its
  # name, and a required property with one.
  Rule: A name has one value, however it is written

    Background:
      Given the authoring tools are connected with the capabilities "fs"

    Scenario Outline: The value there is is replaced, whatever kind it is
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            <before>
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "set_binding", "object": {"path": "root"}, "name": "a", "value": "5"}}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick
        Item {
            <after>
        }
        """

      Examples:
        | before                         | after                       |
        | a: 1 // why                    | a: 5 // why                 |
        | a: { return 1 }                | a: 5                        |
        | a: Item { width: 3 }           | a: 5                        |
        | a: [ Item { }, Item { } ]      | a: 5                        |
        | a: [1, 2]                      | a: 5                        |
        | a: ({ x: 1 })                  | a: 5                        |
        | a: function () { return 1 }    | a: 5                        |
        | property int a: 1 // why       | property int a: 5 // why    |
        | property var a: { return 1 }   | property var a: 5           |
        | property Item a: Item { }      | property Item a: 5          |
        | property list<Item> a: [ Item { } ] | property list<Item> a: 5 |
        | readonly property int a: 1     | readonly property int a: 5  |
        | default property var a: 1      | default property var a: 5   |
        | readonly property int a        | readonly property int a: 5  |
        | property int a                 | property int a: 5           |
        | a { b: 1 }                     | a: 5                        |
        | a.b: 1                         | a: 5                        |

    Scenario: All the parts of a name go when the whole takes their place
      Given the file "Main.qml":
        """qml
        import QtQuick
        Text {
            font.bold: true
            width: 10 // stays
            font.pixelSize: 12
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "set_binding", "object": {"path": "root"}, "name": "font", "value": "Qt.font({})"}}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick
        Text {
            font: Qt.font({})
            width: 10 // stays
        }
        """

    Scenario: An object that is the value is where its members go
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            property Item a: Item { width: 3 }
            b: Item {
                height: 1
            }
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "set_binding", "object": {"path": "root"}, "name": "b.height", "value": "2"}}
        """
      And the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "set_binding", "object": {"path": "root"}, "name": "a.width", "value": "5"}}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick
        Item {
            property Item a: Item { width: 5 }
            b: Item {
                height: 2
            }
        }
        """

    Scenario: A part of an expression's value is refused
      Given the file "Main.qml":
        """qml
        import QtQuick
        Text {
            font: Qt.font({})
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "set_binding", "object": {"path": "root"}, "name": "font.bold", "value": "true"}}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "`font`"
      And the file "Main.qml" is:
        """qml
        import QtQuick
        Text {
            font: Qt.font({})
        }
        """

    Scenario Outline: What says no value of its own gets a binding beside it
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            <declaration>
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "set_binding", "object": {"path": "root"}, "name": "a", "value": "5"}}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick
        Item {
            <declaration>
            a: 5
        }
        """

      Examples:
        | declaration             |
        | required property int a |
        | property alias a: b.c   |

    Scenario: Taking out the value leaves the declaration
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            property int a: 1 // why
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "remove_binding", "object": {"path": "root"}, "name": "a"}}
        """
      Then the file "Main.qml" is:
        """qml
        import QtQuick
        Item {
            property int a // why
        }
        """

    Scenario: A declaration with no value has nothing to take out
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            property int a
        }
        """
      When the agent calls the tool "qml_edit_apply" with:
        """json
        {"path": "Main.qml", "write": true, "edit": {"op": "remove_binding", "object": {"path": "root"}, "name": "a"}}
        """
      Then the agent is refused with "not_found"

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
