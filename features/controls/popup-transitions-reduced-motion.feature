# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#enter-prop
#   https://doc.qt.io/qt-6/qaccessibilityhints.html#motionPreference-prop
#   docs/ARCHITECTURE.md (section 14, SystemPrefs)
# Qt tells an application that the user wants less motion and leaves the rest
# to it. Ours: a transition is then over in the step it starts in, so a popup
# is there or gone at once. Runs on Ljós only.

@ljos-only @a11y
Feature: Popup transitions with reduced motion
  When the system asks for reduced motion a popup neither fades nor grows:
  it is opened as soon as it is shown and closed as soon as it is hidden.

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
          enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 400 } }
          exit: Transition { NumberAnimation { property: "scale"; from: 1; to: 0.5; duration: 400 } }
          onAboutToShow: root.log += "aboutToShow "
          onOpened: root.log += "opened "
          onAboutToHide: root.log += "aboutToHide "
          onClosed: root.log += "closed "
          onVisibleChanged: root.log += visible ? "visible " : "hidden "
        }
      }
      """

  @api-QtQuick.Controls.Popup.enter
  Scenario: A popup is opened at once
    When the method "open" of "p" is called
    Then the property "log" of "root" is "aboutToShow visible opened "
    And the property "opened" of "p" is true
    And the property "opacity" of "p" is 1
    And the property "enter.running" of "p" is false

  @api-QtQuick.Controls.Popup.exit
  Scenario: A popup is closed at once
    When the method "open" of "p" is called
    And the property "log" of "root" is set to ""
    And the method "close" of "p" is called
    Then the property "log" of "root" is "aboutToHide hidden closed "
    And the property "visible" of "p" is false
    And the property "scale" of "p" is 1

  @api-QtQuick.Controls.Popup.visible
  Scenario: A popup that is made invisible is closed at once
    When the method "open" of "p" is called
    And the property "log" of "root" is set to ""
    And the property "visible" of "p" is set to false
    Then the property "log" of "root" is "aboutToHide hidden closed "
    And the property "visible" of "p" is false
