# Sources:
#   docs/ARCHITECTURE.md section 19.4 (`app_objects`' `mark` and
#   `since_mark`)
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent finds what a step leaves behind
  A step the user takes and undoes, opening and closing a dialog or
  pushing and popping a page, should leave nothing alive. `app_objects`
  with `mark` takes the objects alive now; with `since_mark` it lists what
  was made since and is still alive: the component instances and the
  objects by where each is declared, with the bytes they hold. The script
  collector runs first, so what only waits for it is not listed.

  Background:
    Given the file "Panel.qml":
      """qml
      import QtQuick
      Rectangle { width: 10; height: 10; Text { text: "panel" } }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool open: false
        property var kept: []
        Loader { active: parent.open; source: "Panel.qml" }
        Component { id: note; Item { objectName: "note" } }
        function keep() { kept.push(note.createObject(null)) }
      }
      """
    And the inspector is connected with the capabilities "inspect"

  Scenario: A step undone leaves nothing alive
    When the agent calls the tool "app_objects" with:
      """json
      {"mark": true}
      """
    Then the answer's "/marked" is at least 1
    When the property "open" of "root" is set to true
    And the property "open" of "root" is set to false
    And the agent calls the tool "app_objects" with:
      """json
      {"since_mark": true}
      """
    Then the answer's "/made" is 0
    And the answer's "/components" has 0 entries
    And the answer's text contains "nothing made since the mark is alive"

  Scenario: What is still alive is listed with where it is made
    When the agent calls the tool "app_objects" with:
      """json
      {"mark": true}
      """
    And the property "open" of "root" is set to true
    And the agent calls the tool "app_objects" with:
      """json
      {"since_mark": true}
      """
    Then the answer's "/made" is 2
    And the answer's "/components/0/name" is "Panel"
    And the answer's "/components/0/instances" is 1
    And the answer's "/bytes" is at least 1
    And the answer's text contains "component instances:"
    And the answer's text contains "Panel.qml:2:"

  Scenario: What a script keeps is listed by the place it is made from
    When the agent calls the tool "app_objects" with:
      """json
      {"mark": true}
      """
    And the method "keep" of "root" is called
    And the method "keep" of "root" is called
    And the agent calls the tool "app_objects" with:
      """json
      {"since_mark": true}
      """
    Then the answer's "/made" is 2
    And the answer's "/places/0/count" is 2
    And the answer's "/places/0/at" matches "Scenario\.qml:7:[0-9]+$"
    And the answer's text contains "(note, note)"

  Scenario: A client's own mark is not the one kept for the others
    When the agent calls the tool "app_objects" with:
      """json
      {"mark": true, "holder": "one"}
      """
    And the agent calls the tool "app_objects" with:
      """json
      {"since_mark": true}
      """
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "mark: true"

  Scenario: A mark and a component list are not asked for at once
    When the agent calls the tool "app_objects" with:
      """json
      {"mark": true, "components": true}
      """
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "one of"
