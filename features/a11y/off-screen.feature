# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (cacheBuffer)
#   docs/ARCHITECTURE.md (section 14)
# Probed on qml6 (Qt 6.11.2, Wayland) through AT-SPI: an item wholly outside
# the window is listed without the "showing" and "visible" states, and one
# outside its view's clip but inside the window keeps both. The Qt oracle
# cannot read those states through QML, so these scenarios run on Ljós only.

@ljos-only @a11y
Feature: What lies outside the window is off screen
  An item that is exposed but lies wholly outside the window, such as a
  row a view made ahead of scrolling or kept for later, is off screen: an
  assistive technology does not read it as shown, nor anything inside it.
  What is partly inside the window, or inside it but outside its view's
  clip, is on screen.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 400; height: 400
        Rectangle { x: -100; width: 80; height: 30; Accessible.role: Accessible.Button; Accessible.name: "Left" }
        Rectangle { x: 390; width: 80; height: 30; Accessible.role: Accessible.Button; Accessible.name: "Edge" }
        Rectangle {
          x: -100; y: 250; width: 80; height: 30
          Accessible.role: Accessible.Pane; Accessible.name: "Drawer"
          Rectangle { x: 120; width: 60; height: 30; Accessible.role: Accessible.Button; Accessible.name: "Reach" }
        }
        ListView {
          objectName: "list"
          y: 50; width: 200; height: 120
          clip: true
          model: 40
          delegate: Rectangle {
            required property int index
            width: 200; height: 30
            Accessible.role: Accessible.ListItem; Accessible.name: "row " + index
          }
          Component.onCompleted: positionViewAtIndex(15, ListView.Beginning)
        }
      }
      """
    And an assistive technology is listening

  Scenario: An item wholly outside the window is off screen
    Then the "button" named "Left" is off screen

  Scenario: An item partly inside the window is on screen
    Then the "button" named "Edge" is not off screen

  # A screen reader is told nothing under a node that is not shown.
  Scenario: What reaches into the window from a parent outside it is off screen
    Then the "button" named "Reach" is inside the "pane" named "Drawer"
    And the "button" named "Reach" is off screen

  Scenario: The rows a view shows are on screen
    Then the "listitem" named "row 15" is not off screen
    And the "listitem" named "row 18" is not off screen

  Scenario: Rows a view keeps outside the window are off screen
    Then the "listitem" named "row 0" is off screen
    And the "listitem" named "row 28" is off screen

  Scenario: A row inside the window but outside its view's clip is on screen
    Then the "listitem" named "row 20" is not off screen

  Scenario: A row scrolled back into the window is on screen again
    When the property "contentY" of "list" is set to 0
    Then the "listitem" named "row 0" is not off screen
