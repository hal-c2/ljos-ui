# Sources:
#   docs/ARCHITECTURE.md sections 18.3 (the runner), 19.1 (long operations), 19.3 and 19.7
#   features/README.md (tags and how a run is chosen)
#   https://cucumber.io/docs/cucumber/api/#tag-expressions
# The authoring tools are a Ljós extension: Qt has nothing to compare them with.
# A file given here is written without the spaces its doc string is indented by, and a file
# whose name ends in ".sh" may be run. The features runner of these scenarios is a script
# that says what it was asked: what a real run prints is the runner's own business.

@ljos-only @headless
Feature: Listing and running scenarios with the authoring tools
  An agent finds the scenarios that describe a behaviour with
  `features_list` and, when it was started with the right to run things,
  runs them with `features_run`. A run is a job: the agent gets its name
  at once and asks `run_status` how it went, or stops it with
  `run_cancel`.

  Background:
    Given the file "features/items/box.feature":
      """gherkin
      # Sources:
      #   https://doc.qt.io/qt-6/qml-qtquick-item.html
      @qt-diverges @api-QtQuick.Item.width
      Feature: The size of a box

        Scenario: A box is as wide as it is told
          Given a box
          Then the box is 10 wide

        @backlog
        Scenario: A box can be turned
          Given a box
      """
    And the file "features/tooling/hints.feature":
      """gherkin
      # Sources:
      #   docs/ARCHITECTURE.md
      @ljos-only
      Feature: Hints

        Rule: A hint is short

          Scenario: A hint fits a line
            Given a hint
      """

  Rule: Scenarios are listed by where they are, how they are tagged and what they are called

    Background:
      Given the authoring tools are connected

    Scenario: Every scenario is listed with where it is and what it carries
      When the agent calls the tool "features_list"
      Then the answer's "/total" is 3
      And the answer's "/files" is 2
      And the answer's "/scenarios/0/file" is "features/items/box.feature"
      And the answer's "/scenarios/0/line" is 6
      And the answer's "/scenarios/0/feature" is "The size of a box"
      And the answer's "/scenarios/0/name" is "A box is as wide as it is told"
      And the answer's "/scenarios/0/tags" is ["@qt-diverges", "@api-QtQuick.Item.width"]
      And the answer's "/scenarios/0/status" is "active"

    Scenario: A scenario that does not pass yet says so
      When the agent calls the tool "features_list"
      Then the answer's "/scenarios/1/name" is "A box can be turned"
      And the answer's "/scenarios/1/status" is "backlog"

    Scenario: A scenario says which rule it is an example of
      When the agent calls the tool "features_list"
      Then the answer's "/scenarios/2/rule" is "A hint is short"
      And the answer's "/scenarios/0/rule" is null

    Scenario Outline: A directory or a file narrows the list
      When the agent calls the tool "features_list" with:
        """json
        {"paths": ["<path>"]}
        """
      Then the answer's "/total" is <scenarios>

      Examples:
        | path                           | scenarios |
        | features/items                 | 2         |
        | items                          | 2         |
        | features/tooling/hints.feature | 1         |
        | tooling/hints.feature          | 1         |

    Scenario Outline: A tag expression narrows the list
      When the agent calls the tool "features_list" with:
        """json
        {"tags": "<tags>"}
        """
      Then the answer's "/total" is <scenarios>

      Examples:
        | tags                          | scenarios |
        | @qt-diverges                  | 2         |
        | @qt-diverges and not @backlog | 1         |
        | @backlog or @ljos-only        | 2         |
        | @api-QtQuick.Item.width       | 2         |

    Scenario: A name narrows the list
      When the agent calls the tool "features_list" with:
        """json
        {"name": "turned|hint"}
        """
      Then the answer's "/total" is 2
      And the answer's "/scenarios/1/name" is "A hint fits a line"

    Scenario: A tag expression that cannot be read is refused
      When the agent calls the tool "features_list" with:
        """json
        {"tags": "@qt-diverges not"}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "@a and not @b"

    Scenario: A directory that is not there is not found
      When the agent calls the tool "features_list" with:
        """json
        {"paths": ["features/nowhere"]}
        """
      Then the agent is refused with "not_found"

    Scenario: A feature can be read in full
      When the agent reads the resource "ljos://features/items/box.feature"
      Then the resource contains "Scenario: A box can be turned"

  Rule: Running needs the right to run

    Scenario: An agent without the right is not offered the tool
      Given the authoring tools are connected
      Then the agent is offered the tools "qml_check, qml_lint, type_lookup, api_status, project_info, qml_format, binding_tier_explain, docs_search, features_list, run_status, run_cancel, design_rules, design_motion, design_palette, design_contrast"

    Scenario: An agent without the right is refused and told what is missing
      Given the authoring tools are connected
      When the agent calls the tool "features_run"
      Then the agent is refused with "capability_denied"
      And the refusal mentions "run"

    Scenario: An agent with the right is offered the tool
      Given the authoring tools are connected with the capabilities "run"
      Then the agent is offered the tools "qml_check, qml_lint, type_lookup, api_status, project_info, qml_format, binding_tier_explain, docs_search, features_list, features_run, run_status, run_cancel, design_rules, design_motion, design_palette, design_contrast"

    Scenario: A project that has no runner cannot run, and the refusal says how to name one
      Given the authoring tools are connected with the capabilities "run"
      When the agent calls the tool "features_run"
      Then the agent is refused with "unavailable"
      And the refusal mentions "--features-runner"

  Rule: A run is a job the agent asks about

    Background:
      Given the file "runner.sh":
        """sh
        #!/bin/sh
        echo "backend $LJOS_BDD_BACKEND"
        echo "asked $*"
        echo "[Summary]"
        echo "1 scenario (1 passed)"
        """
      And the authoring tools are connected with the capabilities "run" and the features runner "runner.sh"

    Scenario: The run is named at once
      When the agent calls the tool "features_run"
      Then the answer's "/run_id" is "run-1"
      And the answer's "/state" is "running"

    Scenario: The agent waits for the end and reads how it went
      Given the agent has called the tool "features_run"
      When the agent calls the tool "run_status" with:
        """json
        {"run_id": "run-1", "wait_ms": 20000}
        """
      Then the answer's "/state" is "passed"
      And the answer's "/exit_code" is 0
      And the answer's "/summary" matches "1 scenario \(1 passed\)"

    Scenario: A run is on Ljós unless the agent asks for Qt
      Given the agent has called the tool "features_run"
      When the agent calls the tool "run_status" with:
        """json
        {"run_id": "run-1", "wait_ms": 20000}
        """
      Then the answer's "/output" matches "backend ljos"

    Scenario: A run on Qt is asked for by name
      Given the agent has called the tool "features_run" with:
        """json
        {"backend": "qt"}
        """
      When the agent calls the tool "run_status" with:
        """json
        {"run_id": "run-1", "wait_ms": 20000}
        """
      Then the answer's "/output" matches "backend qt"

    Scenario: What to run is chosen as it is for a list
      Given the agent has called the tool "features_run" with:
        """json
        {"paths": ["items"], "tags": "@qt-diverges and not @backlog", "name": "wide"}
        """
      When the agent calls the tool "run_status" with:
        """json
        {"run_id": "run-1", "wait_ms": 20000}
        """
      Then the answer's "/output" matches "--input \S*/features/items/\*\*/\*\.feature"
      And the answer's "/output" matches "-t @qt-diverges and not @backlog -n wide"

    Scenario: A run of nothing is refused before it starts
      When the agent calls the tool "features_run" with:
        """json
        {"paths": ["features/nowhere"]}
        """
      Then the agent is refused with "not_found"

    Scenario: A run nobody started is not found
      When the agent calls the tool "run_status" with:
        """json
        {"run_id": "run-9"}
        """
      Then the agent is refused with "not_found"
      And the refusal mentions "run-9"

  Rule: A run that fails says what failed

    Background:
      Given the file "runner.sh":
        """sh
        #!/bin/sh
        echo "Feature: The size of a box"
        echo "  Scenario: A box is as wide as it is told"
        echo "   ✔  Given a box"
        echo "   ✘  Then the box is 10 wide"
        echo "      Step failed: the box is 12 wide"
        echo "[Summary]"
        echo "1 scenario (1 failed)"
        exit 1
        """
      And the authoring tools are connected with the capabilities "run" and the features runner "runner.sh"
      And the agent has called the tool "features_run"

    Scenario: The state and the exit status say that it failed
      When the agent calls the tool "run_status" with:
        """json
        {"run_id": "run-1", "wait_ms": 20000}
        """
      Then the answer's "/state" is "failed"
      And the answer's "/exit_code" is 1
      And the answer's "/summary" matches "1 scenario \(1 failed\)"

    Scenario: The failure is given with its scenario, its step and its reason
      When the agent calls the tool "run_status" with:
        """json
        {"run_id": "run-1", "wait_ms": 20000}
        """
      Then the answer's "/failures/0/scenario" is "A box is as wide as it is told"
      And the answer's "/failures/0/step" is "Then the box is 10 wide"
      And the answer's "/failures/0/detail" matches "the box is 12 wide"

  Rule: One run at a time, and a run can be stopped

    Background:
      Given the file "runner.sh":
        """sh
        #!/bin/sh
        echo "started"
        exec sleep 60
        """
      And the authoring tools are connected with the capabilities "run" and the features runner "runner.sh"
      And the agent has called the tool "features_run"

    Scenario: A run that has not ended says so
      When the agent calls the tool "run_status" with:
        """json
        {"run_id": "run-1"}
        """
      Then the answer's "/state" is "running"
      And the answer's "/exit_code" is null

    Scenario: A second run is refused while the first is running
      When the agent calls the tool "features_run"
      Then the agent is refused with "busy"
      And the refusal mentions "run-1"

    Scenario: A run is stopped
      When the agent calls the tool "run_cancel" with:
        """json
        {"run_id": "run-1"}
        """
      Then the answer's "/state" is "cancelled"

    Scenario: A stopped run stays stopped and makes room for the next
      Given the agent has called the tool "run_cancel" with:
        """json
        {"run_id": "run-1"}
        """
      When the agent calls the tool "features_run"
      Then the answer's "/run_id" is "run-2"
