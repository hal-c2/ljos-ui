# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.9
#   https://doc.qt.io/qt-6/qml-qtquick-controls-applicationwindow.html
#   https://developer.mozilla.org/en-US/docs/Web/CSS/ID_selectors
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An object is named by its id when nothing has that object name
  A locator `#name` names the object whose `objectName` is `name`. A window
  is written with an `id` and seldom with an `objectName` as well, and it
  is not a line of the outline below the window: it is the window. So when
  no object has the object name, `#name` names the object whose QML `id` it
  is, as `#` names an element by its id on the web, and an agent is not
  made to look up the window's handle first. What has the object name goes
  on being the one named.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      ApplicationWindow {
        id: app
        width: 300; height: 200
        title: "Demo"
        property int count: 0
        Button { objectName: "more"; text: "More"; onClicked: app.count += 1 }
        Item { id: more; property int n: 1 }
      }
      """
    And the inspector is connected with the capabilities "inspect, input, mutate"

  Scenario: The window is found by its id
    When the agent inspects "#app"
    Then the answer's "/types/1" is "ApplicationWindow"
    And the answer's "/id" is "app"
    And the inspected property "title" has the value "Demo"

  Scenario: A property of the window is set through its id
    When the agent calls the tool "app_set_property" with:
      """json
      {"target": "#app", "property": "count", "value": 5}
      """
    And the agent inspects "#app"
    Then the inspected property "count" has the value "5"

  Scenario: The window is traced through its id
    When the agent calls the tool "app_trace" with:
      """json
      {"target": "#app"}
      """
    And the agent calls the tool "app_click" with:
      """json
      {"target": "#more"}
      """
    And the agent calls the tool "app_trace"
    Then the answer's "/events/0/name" is "count"
    And the answer's "/events/0/value" is 1

  Scenario: What has the object name is named before what has the id
    When the agent inspects "#more"
    Then the answer's "/type" is "Button"

  Scenario: A name that is neither an object name nor an id is refused
    When the agent inspects "#nobody"
    Then the agent is refused with "not_found"
