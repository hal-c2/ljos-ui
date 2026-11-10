# Sources:
#   docs/ARCHITECTURE.md (section 19.5, `ljos observe`)
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html (Focus and key events:
#   a Loader is a focus scope)
# The observer is a Ljós tool: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: The observer types in one value at a time
  On the Objects page a value that can be changed is pressed to type in
  it: a field takes its place, made by a Loader of its own, until the value
  is entered, Escape is pressed or the field loses focus. Pressing another
  value gives the field of that one focus, so the first field lets go and
  only one is open at a time. Each Loader is a focus scope, so a field
  keeps its focus inside its own Loader when another field takes focus.
  The Loader is what loses focus.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
      }
      """

  Scenario: Pressing another value lets the field of the first go
    When the application is observed
    And the observer's "text=Objects" is clicked
    And the observer's "role=listitem[name=\"Scenario\"]" is clicked
    And the observer's "role=button[name=\"width\"]" is clicked
    Then the observer has "role=textinput[name=\"width\"]"
    When the observer's "role=button[name=\"height\"]" is clicked
    Then the observer has "role=textinput[name=\"height\"]"
    And the observer does not have "role=textinput[name=\"width\"]"
