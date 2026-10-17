# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html
#   https://doc.qt.io/qt-6/qml-qtqml-component.html (Component.onDestruction)
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: A view's delegate is still in the view while it is destroyed
  When a ListView or a PathView lets a delegate go, the delegate hears of
  its end while it is still in the view: `Component.onDestruction` finds
  its parent, and a binding that reads the parent does not run once more
  against none. A Tumbler's delegates read `Tumbler.tumbler` this way.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        ListView {
          objectName: "lv"; width: 100; height: 100; model: 2
          delegate: Item {
            width: 10; height: 10
            property real across: parent.width
            Component.onDestruction: root.log += parent === null ? " list:none" : " list:in"
          }
        }
        PathView {
          objectName: "pv"; width: 100; height: 100; model: 2
          path: Path { startX: 0; startY: 0; PathLine { x: 100; y: 100 } }
          delegate: Item {
            width: 10; height: 10
            property real across: parent.width
            Component.onDestruction: root.log += parent === null ? " path:none" : " path:in"
          }
        }
      }
      """

  Scenario: A ListView's dropped delegates still have their parent
    When the property "model" of "lv" is set to 0
    Then the property "log" of "root" is " list:in list:in"
    And the log has no warnings

  Scenario: A PathView's dropped delegates still have their parent
    When the property "model" of "pv" is set to 0
    Then the property "log" of "root" is " path:in path:in"
    And the log has no warnings
