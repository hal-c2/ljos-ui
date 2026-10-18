# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-container.html
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A repeater in a container
  A repeater declared in a container makes the container's items. It
  makes each of them once: moving the repeater into the content item as
  the container is completed does not make them again. A repeater that
  is moved to another parent later does make its items again.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property int made: 0; property int gone: 0
        SwipeView {
          id: view; objectName: "view"; anchors.fill: parent
          Repeater {
            id: rep; model: 3
            Item { Component.onCompleted: root.made++; Component.onDestruction: root.gone++ }
          }
        }
        Item { id: other }
        Item {
          id: plain
          Repeater {
            id: loose; model: 2
            Item { Component.onCompleted: root.made++; Component.onDestruction: root.gone++ }
          }
        }
        function move() { made = 0; gone = 0; loose.parent = other }
      }
      """

  Scenario: The repeater's items are made once
    Then the property "count" of "view" is 3
    And the property "made" of "root" is 5
    And the property "gone" of "root" is 0

  Scenario: A repeater moved later makes its items again
    When the method "move" of "root" is called
    Then the property "made" of "root" is 2
    And the property "gone" of "root" is 2
