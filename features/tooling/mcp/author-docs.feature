# Sources:
#   docs/ARCHITECTURE.md sections 19.3 (the authoring tools), 19.6 (budgets) and 1 (clean room)
#   CLEAN_ROOM.md
#   https://modelcontextprotocol.io/specification/2025-11-25/server/resources
# The authoring tools are a Ljós extension: Qt has nothing to compare them with.
# A file given here is written without the spaces its doc string is indented by, so the
# first line of a file is line 1.

@ljos-only @headless
Feature: Searching the documentation with the authoring tools
  An agent that does not know how Ljós does something asks `docs_search`.
  The answer is a short list of the sections that say the most about it,
  best first, each with a few lines of its text and a link the agent can
  read in full. A link out of the project goes to a page of Qt's
  documentation and never to Qt's source.

  Background:
    Given the authoring tools are connected
    And the file "docs/ARCHITECTURE.md":
      """markdown
      # Architecture

      Ljós runs QML.

      ## Binding tiers

      A binding runs in one of three tiers. Tier 0 reads a property path.
      See https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html for what
      a binding is.

      ## Rendering

      The scene is painted from a list. Nothing here is about the tiers of
      bindings.
      """
    And the file "docs/research/09-text.md":
      """markdown
      # Text layout

      Elision cuts a line that does not fit. Qt describes it at
      https://doc.qt.io/qt-6/qml-qtquick-text.html#elide-prop and its code
      is at https://code.qt.io/cgit/qt/qtdeclarative.git/tree/src/quick
      which nobody here opens.
      """
    And the file "features/README.md":
      """markdown
      # Behaviour specifications

      ## Tags

      A scenario that only Ljós can run is tagged `@ljos-only`.
      """
    And the file "ledger/qtquick.toml":
      """toml
      [[row]]
      module = "QtQuick"
      type = "Canvas"
      kind = "type"
      status = "dropped"
      reason = "Immediate painting is out of scope; use Shape or an Image."
      """

  Rule: The sections that say the most come first

    Scenario: A section is found by what its heading says
      When the agent calls the tool "docs_search" with:
        """json
        {"query": "binding tiers"}
        """
      Then the answer's "/results/0/uri" is "ljos://docs/ARCHITECTURE.md"
      And the answer's "/results/0/title" is "Architecture > Binding tiers"
      And the answer's "/results/0/line" is 5
      And the answer's "/results/0/snippet" matches "one of three tiers"

    Scenario: A heading that matches outranks a passing mention
      When the agent calls the tool "docs_search" with:
        """json
        {"query": "binding tiers"}
        """
      Then the answer's "/total" is 2
      And the answer's "/results/1/title" is "Architecture > Rendering"

    Scenario: Research notes below the documentation directory are searched
      When the agent calls the tool "docs_search" with:
        """json
        {"query": "elision"}
        """
      Then the answer's "/results/0/uri" is "ljos://docs/research/09-text.md"
      And the answer's "/results/0/title" is "Text layout"

    Scenario: The architecture outranks a research note that says more of the same
      Given the file "docs/research/02-painting.md":
        """markdown
        # Painting

        ## Rendering

        The scene is painted from a list, and the list is painted in order.
        """
      When the agent calls the tool "docs_search" with:
        """json
        {"query": "rendering painted"}
        """
      Then the answer's "/results/0/uri" is "ljos://docs/ARCHITECTURE.md"
      And the answer's "/results/1/uri" is "ljos://docs/research/02-painting.md"

    Scenario: The name of the project is found however it is typed
      When the agent calls the tool "docs_search" with:
        """json
        {"query": "what LJOS runs"}
        """
      Then the answer's "/results/0/title" is "Architecture"

    Scenario: The rules for writing scenarios are searched
      When the agent calls the tool "docs_search" with:
        """json
        {"query": "tags"}
        """
      Then the answer's "/results/0/uri" is "ljos://features/README.md"
      And the answer's "/results/0/title" is "Behaviour specifications > Tags"

    Scenario: The notes of the ledger are searched
      When the agent calls the tool "docs_search" with:
        """json
        {"query": "painting"}
        """
      Then the answer's "/results/0/uri" is "ljos://ledger/QtQuick/Canvas"
      And the answer's "/results/0/title" is "QtQuick.Canvas (dropped)"
      And the answer's "/results/0/snippet" matches "use Shape or an Image"

    Scenario: Words that are in nothing find nothing
      When the agent calls the tool "docs_search" with:
        """json
        {"query": "quaternion"}
        """
      Then the answer's "/total" is 0
      And the answer's "/results" is []

    Scenario: A search for nothing is refused
      When the agent calls the tool "docs_search" with:
        """json
        {"query": "  "}
        """
      Then the agent is refused with "invalid_argument"

  Rule: A result is small and the rest can be asked for

    Scenario: The agent says how many results it wants
      When the agent calls the tool "docs_search" with:
        """json
        {"query": "binding tiers", "limit": 1}
        """
      Then the answer's "/total" is 2
      And the answer's "/results/0/title" is "Architecture > Binding tiers"
      And the answer's "/next_cursor" is "1"

    Scenario: The next results follow the first
      When the agent calls the tool "docs_search" with:
        """json
        {"query": "binding tiers", "limit": 1, "cursor": "1"}
        """
      Then the answer's "/results/0/title" is "Architecture > Rendering"
      And the answer's "/next_cursor" is null

  Rule: A link leads to something the agent may read

    Scenario: The link of a result is a resource
      When the agent reads the resource "ljos://docs/research/09-text.md"
      Then the resource contains "Elision cuts a line that does not fit."

    Scenario: The link of a ledger note is the row it is a note of
      When the agent reads the resource "ljos://ledger/QtQuick/Canvas"
      Then the resource contains "Immediate painting is out of scope"

    Scenario: A page of Qt's documentation that a section cites is passed on
      When the agent calls the tool "docs_search" with:
        """json
        {"query": "elision"}
        """
      Then the answer's "/results/0/qt_docs/0" is "https://doc.qt.io/qt-6/qml-qtquick-text.html#elide-prop"

    Scenario: A link to Qt's source is never passed on
      When the agent calls the tool "docs_search" with:
        """json
        {"query": "elision"}
        """
      Then the answer does not contain "code.qt.io"

    Scenario: A document outside the documentation directory is not a resource
      When the agent reads the resource "ljos://docs/../ledger/qtquick.toml"
      Then the agent is refused with "path_denied"
