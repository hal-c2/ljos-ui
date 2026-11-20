# Sources:
#   https://developer.android.com/guide/navigation/custom-back/predictive-back-gesture
#   https://developer.android.com/reference/androidx/activity/OnBackPressedCallback
#   https://developer.android.com/reference/androidx/activity/BackEventCompat
#   docs/ARCHITECTURE.md (§10.3: the `Ljos.Back` router, the topmost consumer wins)
#   https://github.com/hal-c2/ljos-ui/issues/614
# A phone's Back is the system's: a key, a button or a swipe from the edge
# of the screen that the system previews as it goes (predictive back).
# An application says where it can go back to with `Back` handlers; while
# none is enabled, Back is the system's to do, which on Android sends the
# application to the background with the system's own animation. Qt has
# no such type, so these are Ljós's own.
@ljos-only
Feature: Back goes to the application's topmost Back handler
  A `Back` handler that is enabled, and shown where it is in an item,
  takes the system's Back. Of several, the last one made takes it: a page
  pushed on another, or a popup over a page. A swipe tells the handler as
  it starts, how far it has come, and whether it was cancelled or
  completed; only a completed one goes back.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      import Ljos
      Item {
        id: root; objectName: "root"
        property int depth: 3
        property bool popupOpen: false; property bool sheetOpen: false
        property string told: ""
        Back {
          id: pages; objectName: "pages"
          enabled: root.depth > 1
          onStarted: root.told += "started "
          onProgressed: root.told += "at " + progress + " "
          onCancelled: root.told += "cancelled "
          onInvoked: { root.told += "invoked "; root.depth-- }
        }
        Item {
          id: popup; visible: root.popupOpen
          Back { objectName: "popupBack"; onInvoked: root.popupOpen = false }
        }
        Loader {
          active: root.sheetOpen
          sourceComponent: Item { Back { onInvoked: root.sheetOpen = false } }
        }
        property bool dialogOpen: false
        Popup {
          visible: root.dialogOpen
          Back { objectName: "dialogBack"; onInvoked: root.dialogOpen = false }
        }
      }
      """

  Scenario: An enabled handler takes Back
    When the user presses the system's Back
    Then the application took Back
    And the property "depth" of "root" is 2

  Scenario: With no handler enabled, Back is the system's
    When the property "depth" of "root" is set to 1
    And the user presses the system's Back
    Then the application left Back to the system
    And the property "depth" of "root" is 1

  Scenario: Of two handlers the last one made takes Back
    When the property "popupOpen" of "root" is set to true
    And the user presses the system's Back
    Then the application took Back
    And the property "popupOpen" of "root" is false
    And the property "depth" of "root" is 3

  Scenario: A handler in a hidden item does not take Back
    When the user presses the system's Back
    Then the property "popupOpen" of "root" is false
    And the property "depth" of "root" is 2

  Scenario: A handler declared in a closed popup does not take Back
    When the user presses the system's Back
    Then the property "depth" of "root" is 2

  Scenario: A handler declared in an open popup takes Back
    When the property "dialogOpen" of "root" is set to true
    And the user presses the system's Back
    Then the application took Back
    And the property "dialogOpen" of "root" is false
    And the property "depth" of "root" is 3

  Scenario: A cancelled back swipe goes nowhere
    When the user starts the back swipe
    Then the property "active" of "pages" is true
    When the user's back swipe has come 0.5 of the way
    Then the property "progress" of "pages" is 0.5
    When the user cancels the back swipe
    Then the property "told" of "root" is "started at 0.5 cancelled "
    And the property "active" of "pages" is false
    And the property "progress" of "pages" is 0
    And the property "depth" of "root" is 3

  Scenario: A completed back swipe goes back once
    When the user starts the back swipe
    And the user's back swipe has come 0.25 of the way
    And the user completes the back swipe
    Then the application took Back
    And the property "told" of "root" is "started at 0.25 invoked "
    And the property "active" of "pages" is false
    And the property "depth" of "root" is 2

  Scenario: A swipe ends at the handler it started at
    When the user starts the back swipe
    And the property "popupOpen" of "root" is set to true
    And the user completes the back swipe
    Then the property "depth" of "root" is 2
    And the property "popupOpen" of "root" is true

  Scenario: A swipe whose handler is gone goes back where Back goes now
    When the property "sheetOpen" of "root" is set to true
    And the user starts the back swipe
    And the property "sheetOpen" of "root" is set to false
    And the user completes the back swipe
    Then the application took Back
    And the property "depth" of "root" is 2
