# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-drawer.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-drawer.html#position-prop
# The Qt oracle cannot run these: it holds the pointer across steps with a
# handler above the window, and a drawer that takes the pointer takes it
# from that handler too, so the run never goes on. The values were measured
# on Qt 6.11.2 with qmltestrunner (offscreen) instead, by mousePress,
# mouseMove and mouseRelease with the same timings.

@ljos-only
Feature: Where a dragged drawer goes when it is let go
  A drawer follows the pointer once the pointer has moved more than 20
  pixels along the drawer's axis. Let go, a drawer that was closed opens
  if it is at least 0.3 of the way in, or if the pointer was still moving
  in quickly; a drawer that was open stays open only if it is more than
  0.7 of the way in, however quickly it was moved.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300; property string log: ""
        Drawer {
          id: d; objectName: "d"; width: 120; height: root.height
          onAboutToShow: root.log += "aboutToShow "; onOpened: root.log += "opened "
          onAboutToHide: root.log += "aboutToHide "; onClosed: root.log += "closed "
        }
        function openNow() { d.open(); log = "" }
      }
      """

  @api-QtQuick.Controls.Drawer.position @api-QtQuick.Controls.Drawer.dragMargin
  Scenario: The drawer follows the pointer from where it took it
    When the user presses the pointer at 4, 150
    And 100 milliseconds pass
    And the user moves the pointer to 24, 150
    Then the property "visible" of "d" is false
    When 100 milliseconds pass
    And the user moves the pointer to 25, 150
    Then the property "visible" of "d" is true
    And the property "position" of "d" is 0
    And the property "log" of "root" is "aboutToShow "
    When 100 milliseconds pass
    And the user moves the pointer to 85, 150
    Then the property "position" of "d" is 0.5
    And the property "x" of "d" is -60

  @api-QtQuick.Controls.Drawer.position
  Scenario: A closed drawer let go short of 0.3 of the way goes back
    When the user presses the pointer at 4, 150
    And 100 milliseconds pass
    And the user moves the pointer to 30, 150
    And 100 milliseconds pass
    And the user moves the pointer to 58, 150
    And 100 milliseconds pass
    And the user moves the pointer to 60, 150
    And 300 milliseconds pass
    And the user releases the pointer at 60, 150
    And 1 second passes
    Then the property "visible" of "d" is false
    And the property "log" of "root" is "aboutToShow aboutToHide closed "

  @api-QtQuick.Controls.Drawer.position
  Scenario: A closed drawer let go 0.3 of the way in opens
    When the user presses the pointer at 4, 150
    And 100 milliseconds pass
    And the user moves the pointer to 30, 150
    And 100 milliseconds pass
    And the user moves the pointer to 64, 150
    And 100 milliseconds pass
    And the user moves the pointer to 66, 150
    And 300 milliseconds pass
    And the user releases the pointer at 66, 150
    And 1 second passes
    Then the property "position" of "d" is 1
    And the property "log" of "root" is "aboutToShow opened "

  Scenario: A drawer flicked in opens however little of it is in
    When the user presses the pointer at 4, 150
    And 10 milliseconds pass
    And the user moves the pointer to 30, 150
    And 10 milliseconds pass
    And the user moves the pointer to 38, 150
    And 10 milliseconds pass
    And the user moves the pointer to 46, 150
    And 10 milliseconds pass
    And the user moves the pointer to 54, 150
    And 100 milliseconds pass
    And the user releases the pointer at 54, 150
    And 1 second passes
    Then the property "position" of "d" is 1

  Scenario: A flick that stopped before the release does not open the drawer
    When the user presses the pointer at 4, 150
    And 10 milliseconds pass
    And the user moves the pointer to 30, 150
    And 10 milliseconds pass
    And the user moves the pointer to 38, 150
    And 10 milliseconds pass
    And the user moves the pointer to 46, 150
    And 10 milliseconds pass
    And the user moves the pointer to 54, 150
    And 200 milliseconds pass
    And the user releases the pointer at 54, 150
    And 1 second passes
    Then the property "visible" of "d" is false

  Scenario: A drawer taken back to its edge is closed at the release
    When the user presses the pointer at 4, 150
    And 100 milliseconds pass
    And the user moves the pointer to 30, 150
    And 100 milliseconds pass
    And the user moves the pointer to 60, 150
    And 100 milliseconds pass
    And the user moves the pointer to 10, 150
    And 300 milliseconds pass
    And the user releases the pointer at 10, 150
    Then the property "visible" of "d" is false
    And the property "log" of "root" is "aboutToShow aboutToHide closed "

  @api-QtQuick.Controls.Drawer.position
  Scenario: An open drawer let go more than 0.7 of the way in stays open
    When the method "openNow" of "root" is called
    And 1 second passes
    And the user presses the pointer at 110, 150
    And 100 milliseconds pass
    And the user moves the pointer to 80, 150
    And 100 milliseconds pass
    And the user moves the pointer to 52, 150
    And 100 milliseconds pass
    And the user moves the pointer to 50, 150
    And 300 milliseconds pass
    And the user releases the pointer at 50, 150
    And 1 second passes
    Then the property "position" of "d" is 1
    And the property "log" of "root" is "opened aboutToShow opened "

  @api-QtQuick.Controls.Drawer.position
  Scenario: An open drawer let go at less than 0.7 closes
    When the method "openNow" of "root" is called
    And 1 second passes
    And the user presses the pointer at 110, 150
    And 100 milliseconds pass
    And the user moves the pointer to 80, 150
    And 100 milliseconds pass
    And the user moves the pointer to 42, 150
    And 100 milliseconds pass
    And the user moves the pointer to 40, 150
    And 300 milliseconds pass
    And the user releases the pointer at 40, 150
    And 1 second passes
    Then the property "visible" of "d" is false
    And the property "log" of "root" is "opened aboutToHide closed "

  Scenario: An open drawer flicked out but still mostly in stays open
    When the method "openNow" of "root" is called
    And 1 second passes
    And the user presses the pointer at 110, 150
    And 10 milliseconds pass
    And the user moves the pointer to 80, 150
    And 10 milliseconds pass
    And the user moves the pointer to 70, 150
    And 10 milliseconds pass
    And the user moves the pointer to 60, 150
    And 10 milliseconds pass
    And the user moves the pointer to 56, 150
    And the user releases the pointer at 56, 150
    And 1 second passes
    Then the property "position" of "d" is 1
