# Sources:
#   docs/ARCHITECTURE.md sections 19.4, 19.6 and 19.7
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#transform-prop
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent finds an item where its transform puts it
  An item above may be moved by its `transform` list, as a whole screen scaled
  to fit its window is. What the tree says of the items below is where they are
  painted, so a click on a reference reaches them.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: bg; objectName: "root"; width: 100; height: 100
        property int clicks: 0
        Item {
          width: 200; height: 200; x: -50; y: -50
          transform: Scale { origin.x: 100; origin.y: 100; xScale: 0.5; yScale: 0.5 }
          Rectangle {
            x: 20; y: 160; width: 60; height: 30
            Accessible.role: Accessible.Button
            Accessible.name: "Corner"
            MouseArea { anchors.fill: parent; onClicked: bg.clicks++ }
          }
        }
      }
      """
    And the inspector is connected with the capabilities "inspect, input"
    And the agent has requested a snapshot

  Scenario: A click on a reference reaches an item a transform above has moved
    When the agent calls the tool "app_click" on the reference of the "button" named "Corner"
    Then the property "clicks" of "root" is 1
