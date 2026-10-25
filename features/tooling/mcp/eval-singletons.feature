# Sources:
#   docs/ARCHITECTURE.md sections 6.4 and 19.4
#   https://doc.qt.io/qt-6/qml-singleton.html
#   features/language/singletons.feature
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: A script an agent runs names the application's singletons
  A script run with `app_eval` names a singleton as code written in the
  application's root file names it: a file that says `pragma Singleton`, and
  a singleton type the program registers, are read by the type's name, in an
  object or in the global scope. A singleton no code has read yet is made
  when the script reads it, as it would be for the application.

  The scenarios use the module `Fixtures` (features/README.md), whose
  `Session` is one for the engine.

  Background:
    Given the file "qmldir":
      """
      singleton Theme Theme.qml
      singleton Tints Tints.qml
      """
    And the file "Theme.qml":
      """qml
      pragma Singleton
      import QtQuick
      QtObject { property int base: 4 }
      """
    And the file "Tints.qml":
      """qml
      pragma Singleton
      import QtQuick
      QtObject { property string accent: "teal" }
      """
    And the QML:
      """qml
      import QtQuick
      import Fixtures
      Item {
        objectName: "root"
        property int base: Theme.base
        Component.onCompleted: Session.title = "open"
        Item { objectName: "inner" }
      }
      """
    And the inspector is connected with the capabilities "inspect, eval"

  Scenario: A file singleton is named in an object
    When the agent calls the tool "app_eval" with:
      """json
      {"target": "#inner", "source": "Theme.base * 10"}
      """
    Then the answer's "/value" is 40

  Scenario: A file singleton is named in the global scope
    When the agent calls the tool "app_eval" with:
      """json
      {"source": "Theme.base + 1"}
      """
    Then the answer's "/value" is 5

  Scenario: A file singleton no code has read is made when the script reads it
    When the agent calls the tool "app_eval" with:
      """json
      {"source": "Tints.accent"}
      """
    Then the answer's "/value" is "teal"

  Scenario: A singleton the program registers is named in either scope
    When the agent calls the tool "app_eval" with:
      """json
      {"source": "Session.title"}
      """
    Then the answer's "/value" is "open"
    When the agent calls the tool "app_eval" with:
      """json
      {"target": "#inner", "source": "Session.title + '!'"}
      """
    Then the answer's "/value" is "open!"

  Scenario: What a script writes to a singleton is what the application reads
    When the agent calls the tool "app_eval" with:
      """json
      {"source": "Theme.base = 6"}
      """
    Then the property "base" of "root" is 6

  Scenario: A name that is nothing is still an error
    When the agent calls the tool "app_eval" with:
      """json
      {"source": "Nowhere.base"}
      """
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "ReferenceError"
