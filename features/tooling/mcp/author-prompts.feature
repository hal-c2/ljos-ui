# Sources:
#   docs/ARCHITECTURE.md sections 19.3 (resources and prompts) and 19.6 (budgets)
#   https://modelcontextprotocol.io/specification/2025-11-25/server/prompts
#   https://modelcontextprotocol.io/specification/2025-11-25/server/resources
# The authoring tools are a Ljós extension: Qt has nothing to compare them with.
# A file given here is written without the spaces its doc string is indented by.

@ljos-only @headless
Feature: Prompts and reference pages of the authoring tools
  A prompt is a way of working that an agent can ask for by name: which
  tools to call, and in what order, to write a component, port one, name
  what a screen reader cannot name, make a text translatable, or describe
  a behaviour. The reference pages are what those ways of working read.

  Background:
    Given the authoring tools are connected with the capabilities "run"

  Rule: Each prompt names the tools to call, in the order to call them

    Scenario: The prompts are offered by name
      Then the agent is offered the prompts "write-component, port-from-qtquick, add-accessibility, localise, write-feature"

    Scenario: Writing a component goes from the ledger to the scenario to the QML
      When the agent asks for the prompt "write-component" with:
        """json
        {"name": "NoteCard", "purpose": "one note in a list"}
        """
      Then the prompt contains "NoteCard"
      And the prompt contains "one note in a list"
      And the prompt names the tools "api_status, docs_search, features_list, qml_format, qml_check, qml_lint, binding_tier_explain, features_run, run_status"

    Scenario: Porting from Qt Quick starts at what Ljós has
      When the agent asks for the prompt "port-from-qtquick" with:
        """json
        {"source": "views/Old.qml"}
        """
      Then the prompt contains "views/Old.qml"
      And the prompt names the tools "api_status, type_lookup, docs_search, qml_format, qml_check, qml_lint, binding_tier_explain"

    Scenario: Adding accessibility goes from the findings to a scenario that holds them fixed
      When the agent asks for the prompt "add-accessibility" with:
        """json
        {"paths": "views"}
        """
      Then the prompt contains "views"
      And the prompt contains "Accessible.name"
      And the prompt names the tools "qml_lint, type_lookup, qml_check, features_list, features_run, run_status"

    Scenario: Localising goes from the texts nobody can translate to the catalogue
      When the agent asks for the prompt "localise" with:
        """json
        {"paths": "views"}
        """
      Then the prompt contains "qsTr"
      And the prompt contains "ljos i18n extract"
      And the prompt names the tools "qml_lint, docs_search, qml_format, qml_check"

    Scenario: Writing a feature goes from what exists to a run
      When the agent asks for the prompt "write-feature" with:
        """json
        {"behaviour": "a box keeps its width when its parent grows"}
        """
      Then the prompt contains "a box keeps its width when its parent grows"
      And the prompt contains "ljos://features/steps"
      And the prompt names the tools "features_list, docs_search, api_status, features_run, run_status"

    Scenario: A prompt without what it needs is refused
      When the agent asks for the prompt "write-component"
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "name"

    Scenario: A prompt nobody wrote is not found
      When the agent asks for the prompt "write-poem"
      Then the agent is refused with "not_found"

  Rule: The reference pages are generated from what they describe

    Background:
      Given the file "features/items/box.feature":
        """gherkin
        # Sources:
        #   https://doc.qt.io/qt-6/qml-qtquick-item.html
        Feature: The size of a box

          Scenario: A box is as wide as it is told
            Given a box
        """
      And the file "Notes.md":
        """markdown
        Not a feature.
        """

    Scenario: The step catalogue lists the steps the runner has
      When the agent reads the resource "ljos://features/steps"
      Then the resource contains "Given the QML:"
      And the resource contains "is set to <value>"
      And the resource contains "Then the snapshot lists N node[s]"

    Scenario: A step that takes a doc string ends in a colon
      When the agent reads the resource "ljos://features/steps"
      Then the resource contains "Then the check reports:"

    Scenario: A feature is read by its path
      When the agent reads the resource "ljos://features/items/box.feature"
      Then the resource contains "Scenario: A box is as wide as it is told"

    Scenario: A directory of features is read as the list of what is in it
      When the agent reads the resource "ljos://features/items"
      Then the resource contains "ljos://features/items/box.feature"

    Scenario: A file outside the features directory is not a resource
      When the agent reads the resource "ljos://features/../Notes.md"
      Then the agent is refused with "path_denied"

  Rule: The server says how to begin

    Scenario: The instructions name the tools an agent starts with
      Then the authoring tools' instructions match "api_status"
      And the authoring tools' instructions match "qml_check"
      And the authoring tools' instructions match "docs_search"
