# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#enter-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-drawer.html
#   https://doc.qt.io/qt-6/qaccessibilityhints.html#motionPreference-prop
#   https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html
#   https://developer.apple.com/design/human-interface-guidelines/motion
#   docs/ARCHITECTURE.md (section 14, SystemPrefs)
#   docs/design/motion.md (reduced motion)
# Qt tells an application that the user wants less motion and leaves the rest
# to it. Ours: a popup's way in and out does not move it. What would have
# grown, shrunk or slid it is at its end at once, and the popup fades in or
# out instead, in at most 150 milliseconds. A fade of its own stays. The
# signals come in the same order as with motion, opened and closed when the
# fade ends. Runs on Ljós only.

@ljos-only @a11y
Feature: Popup transitions with reduced motion
  When the system asks for reduced motion a popup fades in and out in place:
  it neither grows nor slides, and it says that it opened or closed when
  the fade is over.

  Background:
    Given the system setting "reduced motion" is true
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property string log: ""
        Popup {
          id: p; objectName: "p"; width: 100; height: 100
          enter: Transition {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 400 }
            NumberAnimation { property: "scale"; from: 0.5; to: 1; duration: 400 }
          }
          exit: Transition { NumberAnimation { property: "scale"; from: 1; to: 0.5; duration: 400 } }
          onAboutToShow: root.log += "aboutToShow "
          onOpened: root.log += "opened "
          onAboutToHide: root.log += "aboutToHide "
          onClosed: root.log += "closed "
          onVisibleChanged: root.log += visible ? "visible " : "hidden "
        }
        Drawer { id: d; objectName: "d"; width: 120; height: root.height; onOpened: root.log += "opened "; onClosed: root.log += "closed " }
      }
      """

  @api-QtQuick.Controls.Popup.enter
  Scenario: A popup fades in and does not grow
    When the method "open" of "p" is called
    And 75 milliseconds pass
    Then the property "scale" of "p" is 1
    And the property "opacity" of "p" is within 0.15 of 0.5
    And the property "opened" of "p" is false
    And the property "log" of "root" is "aboutToShow visible "

  @api-QtQuick.Controls.Popup.opened
  Scenario: A popup says it opened when the fade is over
    When the method "open" of "p" is called
    And 200 milliseconds pass
    Then the property "log" of "root" is "aboutToShow visible opened "
    And the property "opened" of "p" is true
    And the property "opacity" of "p" is 1
    And the property "enter.running" of "p" is false

  @api-QtQuick.Controls.Popup.exit
  Scenario: A popup whose way out only shrinks it fades out instead
    When the method "open" of "p" is called
    And 200 milliseconds pass
    And the property "log" of "root" is set to ""
    And the method "close" of "p" is called
    And 75 milliseconds pass
    Then the property "scale" of "p" is 1
    And the property "opacity" of "p" is within 0.15 of 0.5
    And the property "visible" of "p" is true
    And the property "log" of "root" is "aboutToHide "

  @api-QtQuick.Controls.Popup.closed
  Scenario: A popup says it closed when the fade is over, and is whole again
    When the method "open" of "p" is called
    And 200 milliseconds pass
    And the property "log" of "root" is set to ""
    And the method "close" of "p" is called
    And 200 milliseconds pass
    Then the property "log" of "root" is "aboutToHide hidden closed "
    And the property "visible" of "p" is false
    And the property "opacity" of "p" is 1
    And the property "scale" of "p" is 1

  @api-QtQuick.Controls.Popup.visible
  Scenario: A popup that is made invisible fades out as it closes
    When the method "open" of "p" is called
    And 200 milliseconds pass
    And the property "log" of "root" is set to ""
    And the property "visible" of "p" is set to false
    And 200 milliseconds pass
    Then the property "log" of "root" is "aboutToHide hidden closed "
    And the property "visible" of "p" is false

  @api-QtQuick.Controls.Drawer.position
  Scenario: A drawer is all the way in at once and fades in
    When the method "open" of "d" is called
    And 75 milliseconds pass
    Then the property "position" of "d" is 1
    And the property "opacity" of "d" is within 0.15 of 0.5
    And the property "opened" of "d" is false
    When 150 milliseconds pass
    Then the property "opacity" of "d" is 1
    And the property "log" of "root" is "opened "

  Scenario: A drawer fades out where it is and then goes back past the edge
    When the method "open" of "d" is called
    And 200 milliseconds pass
    And the method "close" of "d" is called
    And 75 milliseconds pass
    Then the property "position" of "d" is 1
    And the property "opacity" of "d" is within 0.15 of 0.5
    When 150 milliseconds pass
    Then the property "position" of "d" is 0
    And the property "visible" of "d" is false
    And the property "log" of "root" is "opened closed "
