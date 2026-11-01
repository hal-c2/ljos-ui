# Sources:
#   docs/ARCHITECTURE.md sections 17 and 19.3
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html
# The authoring tools are a Ljós extension: Qt has nothing to compare them with.

@ljos-only @headless
Feature: What the authoring tools say Ljós has
  `api_status` says whether Ljós has a type or a member. `Qt` is a type no
  module registers: it is one object every script has, so what it has is
  asked of a running engine, as the rest of the table is asked of the
  engine's registry. Every function and property a script finds on `Qt` is
  told as supported; what the engine does not put there is not.

  Background:
    Given the authoring tools are connected

  Rule: What a script finds on Qt is supported

    Scenario Outline: The function Qt.<name> is supported
      When the agent calls the tool "api_status" with:
        """json
        {"query": "QtQml.Qt.<name>"}
        """
      Then the answer's "/rows/0/status" is "supported"
      And the answer's "/rows/0/kind" is "method"

      Examples:
        | name              |
        | alpha             |
        | binding           |
        | callLater         |
        | color             |
        | colorEqual        |
        | createComponent   |
        | createQmlObject   |
        | darker            |
        | font              |
        | fontFamilies      |
        | formatDate        |
        | formatDateTime    |
        | formatTime        |
        | hsla              |
        | hsva              |
        | isQtObject        |
        | lighter           |
        | locale            |
        | matrix4x4         |
        | openUrlExternally |
        | point             |
        | quaternion        |
        | quit              |
        | rect              |
        | resolvedUrl       |
        | rgba              |
        | size              |
        | tint              |
        | vector2d          |
        | vector3d          |
        | vector4d          |

    Scenario Outline: The property Qt.<name> is supported
      When the agent calls the tool "api_status" with:
        """json
        {"query": "QtQml.Qt.<name>"}
        """
      Then the answer's "/rows/0/status" is "supported"
      And the answer's "/rows/0/kind" is "property"

      Examples:
        | name        |
        | application |
        | platform    |
        | styleHints  |
        | uiLanguage  |

    Scenario: A file that imports QtQuick has the same Qt
      When the agent calls the tool "api_status" with:
        """json
        {"query": "QtQuick.Qt.colorEqual"}
        """
      Then the answer's "/rows/0/status" is "supported"

    Scenario: The enums of Qt stay what they are
      When the agent calls the tool "api_status" with:
        """json
        {"query": "QtQml.Qt.AlignLeft"}
        """
      Then the answer's "/rows/0/status" is "supported"
      And the answer's "/rows/0/kind" is "enum"

  Rule: What the engine does not put on Qt is not told as supported

    Scenario: A name no script finds on Qt has no such answer
      When the agent calls the tool "api_status" with:
        """json
        {"query": "QtQml.Qt.noSuchFunction"}
        """
      Then the agent is refused with "not_found"
