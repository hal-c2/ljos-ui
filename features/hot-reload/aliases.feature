# Sources:
#   docs/ARCHITECTURE.md (sections 12.1 and 19.4)
#   features/hot-reload/roots.feature (the directories an application is watched in)
# Module aliases are a Ljós extension: Qt has nothing to compare them with.

@ljos-only
Feature: A module imported from a directory of another's choosing
  An application that imports a module can be run against another
  directory that holds a module of the same name: a stand-in that answers
  as the real one would, say. The alias is given when the application
  starts, or set and dropped by an agent while it runs; either way the
  files of the directory are watched as the project's are, and a change of
  alias reloads, keeping the application's state as a reload does. The
  objects made from the module are of other types then, so they are made
  anew, as an object whose type changes in an edit is. A directory is
  taken only if it holds the module, and an agent may only name one inside
  the project.

  Background:
    Given the file "app/Main.qml":
      """qml
      import QtQuick
      import App.Media
      Item {
        objectName: "main"
        property int taps: 0
        function tap() { taps += 1 }
        Player { objectName: "player" }
      }
      """
    And the file "app/App/Media/qmldir":
      """
      module App.Media
      Player 1.0 Player.qml
      """
    And the file "app/App/Media/Player.qml":
      """qml
      import QtQuick
      Item { property string source: "real" }
      """
    And the file "app/mocks/Media/qmldir":
      """
      module App.Media
      Player 1.0 Player.qml
      """
    And the file "app/mocks/Media/Player.qml":
      """qml
      import QtQuick
      Item { property string source: "mock" }
      """
    And the file "app/App/Other/qmldir":
      """
      module App.Other
      """
    And the file "elsewhere/Media/qmldir":
      """
      module App.Media
      Player 1.0 Player.qml
      """
    And the file "elsewhere/Media/Player.qml":
      """qml
      import QtQuick
      Item { property string source: "elsewhere" }
      """
    And the QML:
      """qml
      import QtQuick
      Item { objectName: "root" }
      """

  Rule: An alias given at the start

    Scenario: The module is imported from the directory it is aliased to
      When the application watches the roots "app" for the entries "Main" with the module "App.Media" aliased to "app/mocks/Media"
      Then the property "source" of "player" is "mock"

    Scenario: A directory outside the project is watched as the project is
      When the application watches the roots "app" for the entries "Main" with the module "App.Media" aliased to "elsewhere/Media"
      And the method "tap" of "main" is called
      And the file "elsewhere/Media/Player.qml" is saved on disk as:
        """qml
        import QtQuick
        Item { property string source: "changed" }
        """
      Then the property "source" of "player" is "changed"
      And the property "taps" of "main" is 1
      And there is no reload error

    Scenario: A directory that does not hold the module is refused
      When the application is asked to watch the roots "app" for the entries "Main" with the module "App.Media" aliased to "app/App/Other"
      Then the application is refused with a message that mentions "App.Media"
      And the message mentions "App/Other"

  Rule: An alias set by an agent

    Background:
      Given the inspector is connected with the capabilities "inspect, mutate"
      And the application has watched the roots "app" for the entries "Main"

    Scenario: An alias reloads and keeps the state
      When the method "tap" of "main" is called
      And the agent calls the tool "app_alias" with:
        """json
        {"module": "App.Media", "dir": "mocks/Media"}
        """
      Then the answer's "/outcome" is "applied"
      And the answer's "/generation" is 1
      And the property "source" of "player" is "mock"
      And the property "taps" of "main" is 1

    Scenario: A dropped alias gives the module back
      When the agent calls the tool "app_alias" with:
        """json
        {"module": "App.Media", "dir": "mocks/Media"}
        """
      And the method "tap" of "main" is called
      And the agent calls the tool "app_alias" with:
        """json
        {"module": "App.Media", "dir": null}
        """
      Then the answer's "/outcome" is "applied"
      And the answer's "/generation" is 2
      And the property "source" of "player" is "real"
      And the property "taps" of "main" is 1

    Scenario: The files of the aliased directory reload when saved
      When the agent calls the tool "app_alias" with:
        """json
        {"module": "App.Media", "dir": "mocks/Media"}
        """
      And the file "app/mocks/Media/Player.qml" is saved on disk as:
        """qml
        import QtQuick
        Item { property string source: "mock, changed" }
        """
      Then the property "source" of "player" is "mock, changed"

    Scenario: The status lists the aliases
      When the agent calls the tool "app_alias" with:
        """json
        {"module": "App.Media", "dir": "mocks/Media"}
        """
      And the agent calls the tool "app_status"
      Then the answer's "/reload/aliases" has 1 entries
      And the answer's "/reload/aliases/0/module" is "App.Media"
      And the answer's "/reload/aliases/0/dir" matches "mocks/Media$"

    Scenario: With no alias the status lists none
      When the agent calls the tool "app_status"
      Then the answer's "/reload/aliases" has 0 entries

    Scenario: A directory outside the project is refused
      When the agent calls the tool "app_alias" with:
        """json
        {"module": "App.Media", "dir": "../elsewhere/Media"}
        """
      Then the agent is refused with "path_denied"
      And the refusal mentions "elsewhere"
      And the property "source" of "player" is "real"

    Scenario: A directory that does not hold the module is refused
      When the agent calls the tool "app_alias" with:
        """json
        {"module": "App.Media", "dir": "App/Other"}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "App.Media"
      And the refusal mentions "App/Other"

    Scenario: An alias whose files do not compile fails as a reload does
      When the file "app/mocks/Media/Player.qml" is saved on disk as:
        """qml
        import QtQuick
        Item { property string source: }
        """
      And the agent calls the tool "app_alias" with:
        """json
        {"module": "App.Media", "dir": "mocks/Media"}
        """
      Then the answer's "/outcome" is "failed"
      And the answer's "/generation" is 0
      And the property "source" of "player" is "real"
