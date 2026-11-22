# Sources:
#   features/README.md (Surface tags; A phone)
#   docs/ARCHITECTURE.md (§10.2: the application outlives its screen; §19.7: Android)
#   https://developer.android.com/tools/adb#shellcommands (run-as, am, input)
#   https://github.com/hal-c2/ljos-ui/issues/616
# The phone's runner installs one application on the phone or emulator
# (`mise run features:android`) and starts it for each scenario with the
# scenario's files laid over it, so a scenario takes no build. It drives
# the application through its inspector over adb, as the desktop runner
# drives `ljos run` through its own.
@ljos-only @android
Feature: A scenario of a phone runs on a phone
  The scenario's QML is what the phone shows, its objects are the ones
  the steps ask, and each scenario starts with an application of its own.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "w"; visible: true
        property int taps: 0
        function tap() { taps++ }
        Rectangle { anchors.fill: parent; color: "seagreen" }
      }
      """

  Scenario: The scenario's QML runs on the phone, in front
    Then the application is in front
    And the property "taps" of "w" is 0
    When the method "tap" of "w" is called 2 times
    Then the property "taps" of "w" is 2

  Scenario: Each scenario starts with an application of its own
    Then the property "taps" of "w" is 0
    When the property "taps" of "w" is set to 5
    Then the property "taps" of "w" is 5
    And the system has not found the application unresponsive
