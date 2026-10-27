# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtquick-flow.html
# Verified on Qt 6.11.2 (qml6).

@qt-parity
Feature: A Flow child shown in the same change as its hidden ancestor
  A Flow child whose own visible binding turns true in the same change
  that shows a hidden ancestor reads visible, and the Flow places it and
  the children after it as if it had always been shown.

  Rule: A child in a Flow

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 300; height: 100
          property var model: null
          function run() { root.model = ({ child: 1 }) }
          Rectangle {
            visible: root.model !== null
            width: 300; height: 50
            Flow {
              anchors.fill: parent
              Rectangle { width: 10; height: 10 }
              Rectangle {
                objectName: "shown"
                visible: (root.model?.child ?? null) !== null
                width: 10; height: 10
              }
              Rectangle { objectName: "after"; width: 10; height: 10 }
            }
          }
        }
        """

    Scenario: The child is visible once its ancestor shows in the same change
      When the method "run" of "root" is called
      Then the property "visible" of "shown" is true

    Scenario: The Flow places the child and the one after it in their own places
      When the method "run" of "root" is called
      And 100 milliseconds pass
      Then the property "x" of "shown" is 10
      And the property "x" of "after" is 20

  Rule: A Flow of repeated children in a card shown with its host

    Scenario: The card and its repeated children show and take their room
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 300; height: 200
          property var approval: null
          function run() { root.approval = ({ choices: ["a", "b"] }) }
          Item {
            visible: root.approval !== null
            width: 300; height: 200
            Rectangle {
              objectName: "card"
              visible: root.approval !== null
              width: 300; height: flow.height
              Flow {
                id: flow; objectName: "flow"
                width: 300
                Repeater {
                  model: root.approval ? root.approval.choices : []
                  Rectangle { width: 10; height: 10 }
                }
              }
            }
          }
        }
        """
      When the method "run" of "root" is called
      And 100 milliseconds pass
      Then the property "visible" of "card" is true
      And the property "height" of "flow" is 10
