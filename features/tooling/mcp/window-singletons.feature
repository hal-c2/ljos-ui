# Sources:
#   docs/ARCHITECTURE.md sections 6.4 and 19.4
#   features/language/singletons-per-window.feature
# The inspector is a Ljós extension, and so are singletons for each window.

@ljos-only @headless
Feature: The inspector and singletons for each window
  An object reads the singletons of its window, so that is what an agent is
  shown of them: inspecting an object names the ones its window has made,
  each with the properties that hold more than their default.

  The scenarios use the module `Fixtures` (features/README.md), whose
  `Document` is one for each window.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Fixtures
      Item {
        objectName: "root"
        property string title: Document.title
        Component.onCompleted: Document.title = "mine"
        Item { objectName: "inner" }
      }
      """
    And the inspector is connected

  Scenario: An object is shown the singletons its window has made
    When the agent inspects "#inner"
    Then the answer's "/window_singletons" has 1 entries
    And the answer's "/window_singletons/0/type" is "Document"

  Scenario: What a window's singleton holds is shown with it
    When the agent inspects "#inner"
    Then the answer's "/window_singletons/0/properties" is [{"name": "title", "value": "mine"}]
