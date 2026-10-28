# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html#itemRemoved-signal
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html#clear-method
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html#model-prop
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html#itemAdded-signal
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Repeater behaviour under a hidden parent

  Scenario: Clearing a model under a hidden parent removes all delegates and fires zero visibility signals
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int removed: 0
        property int visSignals: 0
        ListModel {
          id: lm; objectName: "lm"
          ListElement { tag: "a" }
          ListElement { tag: "b" }
        }
        Item {
          id: box; objectName: "box"; visible: false
          Repeater {
            id: rep; objectName: "rep"; model: lm
            onItemRemoved: root.removed++
            Item {
              objectName: "d-" + model.tag
              onVisibleChanged: root.visSignals++
            }
          }
        }
      }
      """
    When the method "clear" of "lm" is called
    Then the property "count" of "rep" is 0
    And the property "removed" of "root" is 2
    And the property "visSignals" of "root" is 2

  Scenario: Increasing an integer model under a hidden parent creates delegates immediately with effective visible false
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int added: 0
        property int visOnAdd: 0
        Item {
          id: box; objectName: "box"; visible: false
          Repeater {
            id: rep; objectName: "rep"; model: 1
            onItemAdded: (i, item) => {
              root.added++
              if (item.visible) root.visOnAdd++
            }
            Item { objectName: "d" + index }
          }
        }
      }
      """
    When the property "model" of "rep" is set to 3
    Then the property "count" of "rep" is 3
    And the property "added" of "root" is 4
    And the property "visOnAdd" of "root" is 0
    And the property "visible" of "d2" is false
    When the property "visible" of "box" is set to true
    Then the property "visible" of "d2" is true
