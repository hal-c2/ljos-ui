# Sources:
#   docs/ARCHITECTURE.md (section 19.5, `ljos observe`)
# The observer is a Ljós tool: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: The observer changes only the object whose values it shows
  On the Objects page the values shown are of one object, and a value
  pressed changes that object. Picking another object puts away the values
  of the one before at once, as its answer may take a while to come. An
  answer of an object no longer picked shows nothing, and nothing can be
  changed until the values of the one picked are shown.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        // Each has a value the other has not, so that whose values are
        // shown is told by their names.
        Rectangle { id: a; property string first: "a"; visible: true }
        Rectangle { id: b; property string second: "b"; visible: false }
        // Busy nearly all the time, as a loaded application is: what the
        // observer asks of it is answered up to two seconds after it is
        // asked, long after a click that follows at once. Each run ends
        // before the next is due, so that the application still answers
        // between them (hal-c2/ljos-ui#696).
        Timer {
          interval: 2000; repeat: true; running: true
          onTriggered: { var t = Date.now(); while (Date.now() - t < 1900) {} }
        }
      }
      """

  Scenario: A value pressed as soon as another object is picked is of that object
    When the application is observed
    And the observer's "text=Objects" is clicked
    And the observer's "role=listitem[name=\"Rectangle#a\"]" is clicked
    Then the observer has "role=button[name=\"first\"]"
    When the observer's "role=listitem[name=\"Rectangle#b\"]" is clicked
    And the observer's "role=checkbox[name=\"visible\"]" is clicked
    Then the observer has "role=button[name=\"second\"]"
    And the observer does not have "role=button[name=\"first\"]"
    And the observer has "role=checkbox[name=\"visible\"][checked]"
