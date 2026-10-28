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
  what a screen reader cannot name, make a text translatable, describe
  a behaviour, test an application or show QML from Rust. The reference
  pages are what those ways of working read. The same texts are the skills
  in `skills/` at the root of the repository.

  Background:
    Given the authoring tools are connected with the capabilities "run"

  Rule: Each prompt names the tools to call, in the order to call them

    Scenario: The prompts are offered by name
      Then the agent is offered the prompts "write-component, port-from-qtquick, add-accessibility, design-review, localise, write-feature, test-app, embed-in-rust"

    Scenario: Writing a component goes from the ledger to the scenario to the QML to the audit
      When the agent asks for the prompt "write-component" with:
        """json
        {"name": "NoteCard", "purpose": "one note in a list"}
        """
      Then the prompt contains "NoteCard"
      And the prompt contains "one note in a list"
      And the prompt names the tools "api_status, features_list, design_rules, qml_format, qml_check, qml_lint, binding_tier_explain, features_run, run_status"
      And the prompt contains "ljos audit"
      And the prompt contains "ljos-allow(<code>): <reason>"

    Scenario: Porting from Qt Quick starts at what the checks reject
      When the agent asks for the prompt "port-from-qtquick" with:
        """json
        {"source": "views/Old.qml"}
        """
      Then the prompt contains "views/Old.qml"
      And the prompt names the tools "qml_check, qml_lint, api_status, type_lookup, qml_format, binding_tier_explain"

    Scenario: Adding accessibility goes from the findings to a scenario that holds them fixed
      When the agent asks for the prompt "add-accessibility" with:
        """json
        {"paths": "views"}
        """
      Then the prompt contains "views"
      And the prompt contains "Accessible.name"
      And the prompt names the tools "qml_lint, design_rules, type_lookup, qml_check, features_list, features_run, run_status"
      And the prompt contains "docs/design/accessibility.md"
      And the prompt contains "ljos audit"

    Scenario: A design review goes from the lint to the audit at the full matrix and the guides
      When the agent asks for the prompt "design-review" with:
        """json
        {"source": "views/NoteCard.qml"}
        """
      Then the prompt contains "views/NoteCard.qml"
      And the prompt names the tools "design_rules, design_palette, design_contrast, design_motion, qml_check, qml_lint, qml_format, features_list, features_run, run_status"
      And the prompt contains "ljos audit"
      And the prompt contains "app_audit"
      And the prompt contains "ljos-allow(<code>): <reason>"
      And the prompt contains "[a11y-unnamed-interactive]"

    Scenario: Localising goes from the texts nobody can translate to the catalogue
      When the agent asks for the prompt "localise" with:
        """json
        {"paths": "views"}
        """
      Then the prompt contains "qsTr"
      And the prompt contains "ljos i18n extract"
      And the prompt names the tools "qml_lint, qml_format, qml_check"

    Scenario: Writing a feature goes from what exists to a run
      When the agent asks for the prompt "write-feature" with:
        """json
        {"behaviour": "a box keeps its width when its parent grows"}
        """
      Then the prompt contains "a box keeps its width when its parent grows"
      And the prompt contains "ljos://features/steps"
      And the prompt names the tools "features_list, api_status, features_run, run_status"

    Scenario: Testing an application specifies what it does and runs that on Ljós and on Qt
      When the agent asks for the prompt "test-app" with:
        """json
        {"paths": "Main.qml"}
        """
      Then the prompt contains "Main.qml"
      And the prompt contains "ljos://features/steps"
      And the prompt contains "objectName"
      And the prompt contains "# Verified on Qt 6.x.y (qml6, offscreen)."
      And the prompt contains "`both`"
      And the prompt names the tools "features_list, features_run, run_status"

    Scenario: Embedding in Rust says how a program is built while it is written and when it ships
      When the agent asks for the prompt "embed-in-rust" with:
        """json
        {"program": "a list of downloads that a worker thread fills"}
        """
      Then the prompt contains "a list of downloads that a worker thread fills"
      And the prompt contains "`dev-qml`"
      And the prompt contains "LJOS_TYPES=write cargo test"
      And the prompt contains "setup.ui_handle()"
      And the prompt contains "docs/site/rust/threads-and-uihandle.md"
      And the prompt names the tools "qml_format, qml_check, qml_lint"

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
