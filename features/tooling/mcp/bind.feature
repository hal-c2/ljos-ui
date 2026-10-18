# Sources:
#   docs/ARCHITECTURE.md sections 12.1, 19.4, 19.6 and 19.7
#   docs/research/10-mcp.md
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#binding-method
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent that binds a property to an expression
  With `mutate` and `eval` together an agent can give a property a binding
  of its own: a JavaScript expression that names the object's properties
  and the ids of its file bare, as a binding written in the object names
  them, and that is worked
  out again whenever what it read changes, as `Qt.binding` is. Like a value,
  a binding can cover the one that was there and be taken back. The picture
  of the item says what the edit did as soon as the call is answered.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        width: 200; height: 100
        color: "#ffffff"
        property int count: 0
        property bool on: false
        property string title: "first"
        property int twice: count * 2
        Rectangle { objectName: "swatch"; width: 40; height: 20; color: "#ff0000" }
        Item { id: ruler; objectName: "ruler"; width: 7 }
        TextInput { objectName: "secret"; y: 40; width: 100; height: 20; echoMode: TextInput.Password }
      }
      """
    And the inspector is connected with the capabilities "inspect, mutate, eval"
    And the agent has requested a snapshot

  Rule: An expression is a binding

    Scenario: The property follows what the expression reads
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "title", "binding": "'n=' + count"}
        """
      And the property "count" of "root" is set to 4
      Then the property "title" of "root" is "n=4"

    Scenario: The expression names the ids of the file as a binding written there does
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#swatch", "property": "height", "binding": "ruler.width + 1"}
        """
      And the property "width" of "ruler" is set to 10
      Then the property "height" of "swatch" is 11

    Scenario: The answer says what the binding gave and that it is one
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "count", "binding": "on ? 7 : 3"}
        """
      Then the answer's "/value" is 3
      And the answer's "/previous" is 0
      And the answer's "/bound" is true

    Scenario: The expression replaces the binding that was there
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "twice", "binding": "count + 1"}
        """
      And the property "count" of "root" is set to 4
      Then the property "twice" of "root" is 5

    Scenario: The inspector gives the expression an agent bound
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "title", "binding": "'n=' + count"}
        """
      And the agent inspects "#root"
      Then the inspected property "title" is "bound"
      And the inspected property "title" is assigned by "devtools"
      And the inspected property "title" is bound to the expression "'n=' + count"

    Scenario: An expression that cannot be read is refused and nothing changes
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "count", "binding": "count +"}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "SyntaxError"
      And the property "count" of "root" is 0

    Scenario: A value and a binding together are refused
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "count", "value": 1, "binding": "2"}
        """
      Then the agent is refused with "invalid_argument"
      And the property "count" of "root" is 0

    Scenario: An expression run in a field that hides its text is refused
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#secret", "property": "color", "binding": "text"}
        """
      Then the agent is refused with "capability_denied"
      And the refusal mentions "hides"

  Rule: A binding that covers another can be taken back

    Scenario: A binding that keeps the one below it covers it
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "twice", "binding": "count * 10", "keep_binding": true}
        """
      And the property "count" of "root" is set to 3
      Then the property "twice" of "root" is 30
      And the answer's "/binding_kept" is true

    Scenario: Taking back a binding that kept the one below it brings that one back
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "twice", "binding": "count * 10", "keep_binding": true}
        """
      And the property "count" of "root" is set to 3
      And the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "twice", "revert": true}
        """
      Then the property "twice" of "root" is 6
      And the answer's "/restored" is "binding"

    Scenario: A second binding takes the place of the first
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "twice", "binding": "count * 10", "keep_binding": true}
        """
      And the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "twice", "binding": "count * 100", "keep_binding": true}
        """
      And the property "count" of "root" is set to 2
      Then the property "twice" of "root" is 200

    Scenario: A value given over a binding an agent bound takes its place
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "twice", "binding": "count * 10", "keep_binding": true}
        """
      And the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "twice", "value": 55, "keep_binding": true}
        """
      And the property "count" of "root" is set to 2
      Then the property "twice" of "root" is 55

    Scenario: Taking back a second edit goes back to before the first
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "twice", "binding": "count * 10", "keep_binding": true}
        """
      And the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "twice", "binding": "count * 100", "keep_binding": true}
        """
      And the property "count" of "root" is set to 2
      And the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "twice", "revert": true}
        """
      Then the property "twice" of "root" is 4

  Rule: The picture of an item shows an edit as soon as it is answered

    Scenario: A picture of the item taken after a binding shows what it gave
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#swatch", "property": "color", "binding": "parent.on ? '#0000ff' : '#00ff00'"}
        """
      And the agent calls the tool "app_screenshot" with:
        """json
        {"target": "#swatch"}
        """
      Then the answer has a PNG picture 40 by 20 pixels
      And the pixel at 20, 10 of the picture is "#00ff00"

    Scenario: The picture follows what the binding reads
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#swatch", "property": "color", "binding": "parent.on ? '#0000ff' : '#00ff00'"}
        """
      And the agent calls the tool "app_set_property" with:
        """json
        {"target": "#root", "property": "on", "value": true}
        """
      And the agent calls the tool "app_screenshot" with:
        """json
        {"target": "#swatch"}
        """
      Then the pixel at 20, 10 of the picture is "#0000ff"
