# Sources:
#   docs/ARCHITECTURE.md sections 19.3 (the authoring tools) and 19.7 (security)
# The authoring tools are a Ljós extension: Qt has nothing to compare them with.
# A link here is a symbolic link in the project to a directory beside it.

@ljos-only @headless
Feature: The authoring tools read nothing outside the project
  An agent reads the project's documentation, scenarios and ledger as
  resources, lists them, searches them and lists the scenarios. A
  directory or a file of the project that is a link out of it is not
  read, listed or searched, so the project an author names is all an
  agent sees.

  Background:
    Given the file "elsewhere/secret.md" outside the project:
      """markdown
      # Secret

      Nothing of the project.
      """
    And the file "elsewhere/secret.feature" outside the project:
      """gherkin
      Feature: Nothing of the project
      """
    And the file "elsewhere/qtquick.toml" outside the project:
      """toml
      # Nothing of the project.
      """
    And the file "elsewhere/README.md" outside the project:
      """markdown
      # Secret

      Nothing of the project.
      """

  Scenario Outline: A documentation or scenario directory that links out is not read
    Given the project's "<directory>" is a link to "elsewhere" outside the project
    And the authoring tools are connected
    When the agent reads the resource "<resource>"
    Then the agent is refused with "path_denied"

    Examples:
      | directory | resource                       |
      | docs      | ljos://docs/secret.md          |
      | docs      | ljos://docs                    |
      | features  | ljos://features/secret.feature |
      | features  | ljos://features                |

  Scenario Outline: A ledger that links out is not read
    Given the project's "<link>" is a link to "<target>" outside the project
    And the authoring tools are connected
    When the agent reads the resource "<resource>"
    Then the agent is refused with "path_denied"

    Examples:
      | link                | target                 | resource                   |
      | ledger/qtquick.toml | elsewhere/qtquick.toml | ljos://ledger              |
      | ledger/qtquick.toml | elsewhere/qtquick.toml | ljos://ledger/QtQuick/Item |
      | ledger              | elsewhere              | ljos://ledger              |
      | ledger              | elsewhere              | ljos://ledger/QtQuick/Item |

  Scenario Outline: Documentation that links out is not listed
    Given the project's "<link>" is a link to "<target>" outside the project
    And the authoring tools are connected
    When the agent lists the resources
    Then the answer's text contains "ljos://ledger"
    And the answer does not contain "<hidden>"

    Examples:
      | link               | target              | hidden                    |
      | docs               | elsewhere           | secret.md                 |
      | features           | elsewhere           | ljos://features/README.md |
      | features/README.md | elsewhere/README.md | ljos://features/README.md |

  Scenario Outline: Documentation that links out is not searched
    Given the project's "<link>" is a link to "<target>" outside the project
    And the authoring tools are connected
    When the agent calls the tool "docs_search" with:
      """json
      {"query": "secret nothing project"}
      """
    Then the answer's "/total" is 0
    And the answer does not contain "Nothing of the project"

    Examples:
      | link               | target              |
      | docs               | elsewhere           |
      | features           | elsewhere           |
      | features/README.md | elsewhere/README.md |

  Scenario: Scenarios that link out are not listed
    Given the project's "features" is a link to "elsewhere" outside the project
    And the authoring tools are connected
    When the agent calls the tool "features_list"
    Then the agent is refused with "path_denied"
    And the answer does not contain "Nothing of the project"
