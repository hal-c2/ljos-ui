# Sources:
#   https://developer.android.com/guide/components/activities/activity-lifecycle
#   https://developer.android.com/guide/topics/resources/runtime-changes
#   https://developer.android.com/guide/navigation/custom-back/predictive-back-gesture
#   https://doc.qt.io/qt-6/qguiapplication.html#applicationState
#   docs/ARCHITECTURE.md (§10.2: the application outlives its screen)
#   docs/spikes/01-android-ime-accesskit.md (§6, the lifecycle table)
#   https://github.com/hal-c2/ljos-ui/issues/613
#   https://developer.android.com/reference/android/content/ComponentCallbacks2#onTrimMemory(int)
#   docs/ARCHITECTURE.md (§8.5: what a hidden window gives back)
# A phone takes the application's screen away when its user leaves it,
# gives it a new one when they come back, and may end the screen of an
# application it hides to save memory. On Ljós the application, its
# objects and their properties live on through all of it; only what was
# drawn is drawn again. Qt on Android ends the application's QML with its
# screen unless the application keeps it alive itself, so these are
# Ljós's own.
@ljos-only @android
Feature: An application on a phone outlives its screen
  The user leaves an application for the home screen, turns the phone,
  presses Back with nothing left to go back to, and the system ends the
  screen of an application it hid. Each time the application comes back
  as it was: the same objects with the same values, at the new size, in
  front, and the system never finds it unresponsive.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        id: w; objectName: "w"; visible: true
        property int taps: 0
        property bool wide: width > height
        function tap() { taps++ }
        function quit() { Qt.quit() }
        Rectangle { anchors.fill: parent; color: "steelblue" }
      }
      """
    When the method "tap" of "w" is called 2 times

  Scenario: Back from the home screen, the application is as it was
    When the user goes to the home screen
    Then the application is in the background
    When the user opens the application again
    Then the application is in front
    And the property "taps" of "w" is 2
    And the system has not found the application unresponsive

  Scenario Outline: Turned to <to>, the window takes the new shape and keeps its values
    When the user turns the phone to landscape
    And the user turns the phone to <to>
    Then the property "wide" of "w" is <wide>
    And the property "taps" of "w" is 2
    And the system has not found the application unresponsive

    Examples:
      | to        | wide  |
      | landscape | true  |
      | portrait  | false |

  Scenario: Back with nothing to go back to puts the application in the background
    When the user presses Back
    Then the application is in the background
    When the user opens the application again
    Then the application is in front
    And the property "taps" of "w" is 2
    And the system has not found the application unresponsive

  Scenario: The application lives on when the system ends its screen
    When the user goes to the home screen
    And the system ends the application's screen
    And the user opens the application again
    Then the application is in front
    And the property "taps" of "w" is 2
    And the system has not found the application unresponsive

  Scenario: A hidden application short of memory gives back what it can draw again
    When the user goes to the home screen
    And the system is short of memory
    Then the application gave back memory it can rebuild
    When the user opens the application again
    Then the application is in front
    And the property "taps" of "w" is 2
    And the system has not found the application unresponsive

  Scenario: An application that quit starts anew when opened again
    When the method "quit" of "w" is called
    Then the application is no longer running
    And the system refused no JNI call of the application
    When the user opens the application again
    Then the application is in front
    And the property "taps" of "w" is 0
    And the system has not found the application unresponsive

  Scenario: A hidden application does nothing
    When the user goes to the home screen
    Then the application is left idle
