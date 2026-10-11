# Sources:
#   docs/ARCHITECTURE.md sections 5.4 (binding tiers), 19.3 and 19.6
#   docs/tier1-coverage.md
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
# The authoring tools are a Ljós extension: Qt has nothing to compare them with.
# A file given here is written without the spaces its doc string is indented by, so the
# first column of a line is 1.

@ljos-only @headless
Feature: Asking which tier a binding runs in
  A binding that only reads a property costs almost nothing, one that
  does arithmetic runs in the binding VM, and anything else is handed to
  JavaScript. `binding_tier_explain` tells an agent which of the three a
  binding is, why, and what to write instead when a cheaper form means
  the same.

  Rule: Every expression has a tier and a reason

    Background:
      Given the authoring tools are connected
      And the file "Main.qml":
        """qml
        import QtQuick

        Item {
            id: root
            property string name: "ada"
            property int count: 3
            width: 200
            height: width
            x: count * 2 + 1
            z: name.indexOf("a")
            Text {
                id: label
                width: parent.width
                text: root.name
                onTextChanged: console.log(text)
            }
        }
        """

    Scenario: A binding that only reads a property is tier 0
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "property": "height"}
        """
      Then the answer's "/expressions/0/tier" is "0"
      And the answer's "/expressions/0/kind" is "binding"
      And the answer's "/expressions/0/source" is "width"
      And the answer's "/expressions/0/why" matches "property path"

    Scenario: Arithmetic runs in the binding VM
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "property": "x"}
        """
      Then the answer's "/expressions/0/tier" is "1"
      And the answer's "/expressions/0/source" is "count * 2 + 1"

    Scenario: A method of a string is left to JavaScript, and the reason is the compiler's
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "property": "z"}
        """
      Then the answer's "/expressions/0/tier" is "js"
      And the answer's "/expressions/0/reason/kind" is "method call"
      And the answer's "/expressions/0/reason/detail" is ".indexOf()"

    Scenario: A value that is written out is not an expression at all
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "property": "root.width"}
        """
      Then the answer's "/expressions/0/kind" is "assigned"
      And the answer's "/expressions/0/tier" is null
      And the answer's "/expressions/0/source" is "200"

    Scenario: A handler has a tier too
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "property": "onTextChanged"}
        """
      Then the answer's "/expressions/0/kind" is "handler"
      And the answer's "/expressions/0/tier" is "js"
      And the answer's "/expressions/0/reason/kind" is "global function call"
      And the answer's "/expressions/0/reason/detail" is "console.log"

    Scenario: Without a place, every expression of the file is listed and counted
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml"}
        """
      Then the answer's "/summary/expressions" is 6
      And the answer's "/summary/tier0" is 3
      And the answer's "/summary/tier1" is 1
      And the answer's "/summary/js" is 2
      And the answer's "/summary/assigned" is 3
      And the answer's "/expressions/0/property" is "name"
      And the answer's "/expressions/0/line" is 5

  Rule: An expression is found by where it is written, or by its property

    Background:
      Given the authoring tools are connected
      And the file "Main.qml":
        """qml
        import QtQuick

        Item {
            id: root
            width: 200
            height: width

            Text {
                id: label
                width: parent.width - 8
            }
        }
        """

    Scenario: A line and a column find the expression there
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "line": 10, "column": 16}
        """
      Then the answer's "/expressions/0/property" is "width"
      And the answer's "/expressions/0/object" is "Text"
      And the answer's "/expressions/0/id" is "label"
      And the answer's "/expressions/0/line" is 10
      And the answer's "/expressions/0/column" is 16

    Scenario: A line alone finds what is written on it
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "line": 6}
        """
      Then the answer's "/expressions/0/property" is "height"
      And the answer's "/expressions/0/id" is "root"

    Scenario: A line with no expression on it is not found, and the nearest are named
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "line": 7}
        """
      Then the agent is refused with "not_found"
      And the refusal mentions "line 6"
      And the refusal mentions "line 10"

    Scenario: A property two objects have is ambiguous until the object is named
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "property": "width"}
        """
      Then the agent is refused with "ambiguous"
      And the refusal names 2 candidates
      And the refusal mentions "label.width"

    Scenario Outline: The object is named by its id or by its type
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "property": "<property>"}
        """
      Then the answer's "/expressions/0/line" is <line>

      Examples:
        | property    | line |
        | label.width | 10   |
        | Text.width  | 10   |
        | root.width  | 5    |
        | Item.width  | 5    |

    Scenario: A property nothing is bound to is not found
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "property": "opacity"}
        """
      Then the agent is refused with "not_found"
      And the refusal mentions "opacity"

  Rule: A rewrite is offered when a cheaper form means the same

    Background:
      Given the authoring tools are connected

    Scenario: A switch that only chooses a value becomes a conditional
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            property string kind: "a"
            opacity: {
                switch (kind) {
                case "a": return 0.5
                case "b": return 0.8
                default: return 1
                }
            }
        }
        """
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "property": "opacity"}
        """
      Then the answer's "/expressions/0/tier" is "js"
      And the answer's "/expressions/0/reason/detail" is "switch"
      And the answer's "/expressions/0/rewrite/source" is "kind === \"a\" ? 0.5 : kind === \"b\" ? 0.8 : 1"
      And the answer's "/expressions/0/rewrite/tier" is "1"

    Scenario: A function that is called where it is written becomes what it returns
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            property int count: 3
            scale: (function() { return count / 2 })()
        }
        """
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "property": "scale"}
        """
      Then the answer's "/expressions/0/tier" is "js"
      And the answer's "/expressions/0/rewrite/source" is "count / 2"
      And the answer's "/expressions/0/rewrite/tier" is "1"

    Scenario: Where no cheaper form means the same, none is made up
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            property string name: "ada"
            z: name.indexOf("a")
        }
        """
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "property": "z"}
        """
      Then the answer's "/expressions/0/rewrite" is null
      And the answer's "/expressions/0/advice" matches "JavaScript"

    Scenario: An expression that is already cheap needs no rewrite
      Given the file "Main.qml":
        """qml
        import QtQuick

        Item {
            width: parent.width
        }
        """
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Main.qml", "property": "width"}
        """
      Then the answer's "/expressions/0/tier" is "0"
      And the answer's "/expressions/0/rewrite" is null

  Rule: A file that cannot be compiled has no tiers to explain

    Background:
      Given the authoring tools are connected

    Scenario: A file that does not parse is refused, and the agent is sent to the checker
      Given the file "Broken.qml":
        """qml
        import QtQuick
        Item {
            width: 10 +
        }
        """
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Broken.qml"}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "qml_check"

    Scenario: A file that is not there is not found
      When the agent calls the tool "binding_tier_explain" with:
        """json
        {"file": "Missing.qml"}
        """
      Then the agent is refused with "not_found"
