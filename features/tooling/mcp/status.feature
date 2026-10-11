# Sources:
#   docs/ARCHITECTURE.md sections 8.5, 9.1, 19.4, 19.6 and 19.9
#   docs/research/10-mcp.md
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#console-log
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Status, log, measurements and pictures from the inspector
  Besides the outline, an agent can ask what state the application is in,
  what it logged, where its time went and what it looks like.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        width: 200; height: 100
        color: "#ff0000"
        function shout() { console.log("hello from shout") }
        function slip() { console.warn("careful") }
        TextInput {
          objectName: "field"
          width: 100; height: 20
          focus: true
          Accessible.name: "Field"
        }
      }
      """
    And the inspector is connected

  Rule: The status says what there is to look at

    Scenario: The window, the clock and what the agent may do are given
      When the agent calls the tool "app_status"
      Then the answer's "/windows/0/width" is 400
      And the answer's "/windows/0/height" is 400
      And the answer's "/windows/0/scale" is 1
      And the answer's "/windows/0/visible" is true
      And the answer's "/clock" is "virtual"
      And the answer's "/capabilities" is ["inspect"]

    Scenario: The item with the keyboard is named
      When the agent calls the tool "app_status"
      Then the answer's "/windows/0/focus/role" is "textinput"
      And the answer's "/windows/0/focus/name" is "Field"
      And the answer's "/windows/0/focus/ref" matches "^e\d+$"

  Rule: The log is given with the file and the line of each message

    Scenario: A message says where it was written
      When the method "shout" of "root" is called
      And the agent calls the tool "app_logs"
      Then the answer's "/entries/0/message" is "hello from shout"
      And the answer's "/entries/0/level" is "debug"
      And the answer's "/entries/0/file" matches "Scenario\.qml$"
      And the answer's "/entries/0/line" is 6

    Scenario: The agent can ask for warnings alone
      When the method "shout" of "root" is called
      And the method "slip" of "root" is called
      And the agent calls the tool "app_logs" with:
        """json
        {"level": "warning"}
        """
      Then the answer's "/entries/0/message" is "careful"
      And the answer's "/count" is 1

    Scenario: The agent can ask for what was logged since it last asked
      When the method "shout" of "root" is called
      And the agent calls the tool "app_logs"
      And the method "slip" of "root" is called
      And the agent asks for the log since its last request
      Then the answer's "/count" is 1
      And the answer's "/entries/0/message" is "careful"

    Scenario: A long log is cut at 100 lines, the newest last
      When the method "shout" of "root" is called 150 times
      And the agent calls the tool "app_logs"
      Then the answer's "/count" is 100
      And the answer's "/truncated" is true

  Rule: The measurements say where the time went

    Scenario: The inspector's own work is in the measurements
      Given the agent has requested a snapshot
      When the agent calls the tool "app_metrics"
      Then the answer's "/frames/count" is at least 1
      And the answer's "/jobs/done" is at least 1
      And the answer's "/owners/0/name" is "inspector"
      And the answer's "/objects" is at least 2

  Rule: A picture is drawn without a screen

    Scenario: The window as a picture
      When the agent calls the tool "app_screenshot"
      Then the answer has a PNG picture 400 by 400 pixels
      And the pixel at 150, 50 of the picture is "#ff0000"
      And the answer's "/width" is 400

    Scenario: One item as a picture
      When the agent calls the tool "app_screenshot" with:
        """json
        {"target": "#field"}
        """
      Then the answer has a PNG picture 100 by 20 pixels

    Scenario: A picture is never larger than the agent asked for
      When the agent calls the tool "app_screenshot" with:
        """json
        {"max_size": 100}
        """
      Then the answer has a PNG picture 100 by 100 pixels
      And the answer's "/scale" is 0.25

  Rule: The same can be read as resources

    Scenario Outline: A resource holds what its tool answers
      When the method "shout" of "root" is called
      And the agent reads the resource "<resource>"
      Then the resource contains "<text>"

      Examples:
        | resource           | text             |
        | ljos://app/tree    | textinput        |
        | ljos://app/logs    | hello from shout |
        | ljos://app/metrics | lanes            |

    Scenario: A resource nobody has is refused
      When the agent reads the resource "ljos://app/secrets"
      Then the agent is refused with "not_found"
