# Sources:
#   docs/ARCHITECTURE.md section 19.4 (`app_objects`' `components` and
#   `component`)
#   https://doc.qt.io/qt-6/qtqml-documents-definetypes.html
#   https://doc.qt.io/qt-6/qml-qtqml-component.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent reads what the application has loaded
  A QML application is made of components: each file is one, a file
  may declare inline components, and every delegate and every content
  of a `Component` is one too. To find what is loaded and what holds the
  memory, `app_objects` with `components` lists each component of every
  file loaded, how it is written, how many instances of it live and the
  objects and bytes they hold, the components nothing was made from
  included. With `component` it lists the instances of one by handle, a
  page at a time, to be looked at in the object tree.

  Background:
    Given the file "Badge.qml":
      """qml
      import QtQuick
      Rectangle { width: 10; height: 10; property string label }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: top
        objectName: "root"
        width: 200; height: 200
        property int count: 3
        component Pill: Rectangle { width: 5; height: 5 }
        component Unused: Item {}
        Column {
          Badge { objectName: "first" }
          Badge { objectName: "second" }
          Pill { objectName: "pill" }
          Repeater { model: top.count; Text { text: index } }
        }
      }
      """
    And the inspector is connected with the capabilities "inspect"

  Scenario: A file component is listed with its instances and what they hold
    When the agent calls the tool "app_objects" with:
      """json
      {"components": true}
      """
    Then the answer's text contains "file Badge (Badge.qml, Badge): 2 instances, 2 objects"

  Scenario: An inline component and a delegate are listed with theirs
    When the agent calls the tool "app_objects" with:
      """json
      {"components": true}
      """
    Then the answer's text contains "inline Pill"
    And the answer's text contains "Pill): 1 instance, 1 objects"
    And the answer's text contains "delegate Text"
    And the answer's text contains "3 instances, 3 objects"

  Scenario: A component nothing was made from is listed with none
    When the agent calls the tool "app_objects" with:
      """json
      {"components": true}
      """
    Then the answer's text contains "Unused): 0 instances, 0 objects, 0 bytes"

  Scenario: The instances of one component are listed by handle
    When the agent calls the tool "app_objects" with:
      """json
      {"component": "Badge"}
      """
    Then the answer's "/kind" is "file"
    And the answer's "/count" is 2
    And the answer's "/instances" has 2 entries
    And the answer's "/instances/0/object_name" is "first"
    And the answer's "/instances/1/type" is "Badge"
    And the answer's "/next_offset" is null

  Scenario: Instances made later are counted, and those gone are not
    When the property "count" of "root" is set to 5
    And the agent calls the tool "app_objects" with:
      """json
      {"components": true}
      """
    Then the answer's text contains "5 instances, 5 objects"
    When the property "count" of "root" is set to 1
    And the agent calls the tool "app_objects" with:
      """json
      {"components": true}
      """
    Then the answer's text contains "1 instance, 1 objects"

  Scenario: The instances are read a page at a time
    When the agent calls the tool "app_objects" with:
      """json
      {"component": "Badge", "limit": 1}
      """
    Then the answer's "/instances" has 1 entry
    And the answer's "/next_offset" is 1

  Scenario: A component that is not loaded is refused
    When the agent calls the tool "app_objects" with:
      """json
      {"component": "Nowhere"}
      """
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "components: true"

  Scenario: Components and models are not asked for at once
    When the agent calls the tool "app_objects" with:
      """json
      {"components": true, "models": true}
      """
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "one of"
