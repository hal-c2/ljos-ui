# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html (model, currentIndex)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.PathView.currentIndex @api-QtQuick.PathView.model
Feature: Where a PathView's current row starts, and who decides it
  A model given once the view is made starts from its first row, even when
  the view had no rows before. A current index that is bound keeps
  following what it is bound to, even after the view brought it round
  into its rows itself.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"; width: 100; height: 200
        property int wanted: -1
        component V: PathView {
          anchors.fill: parent
          delegate: Item {}
          path: Path { startX: 50; startY: 0; PathLine { x: 50; y: 200 } }
        }
        V { id: empty; objectName: "empty"; model: 0; currentIndex: 3 }
        V { id: none; objectName: "none" }
        V { id: bound; objectName: "bound"; model: 10; currentIndex: parent.wanted }
        function fill() { empty.model = 10; none.model = 10 }
      }
      """

  Scenario: A model given to a view without rows starts from its first row
    When the method "fill" of "root" is called
    Then the property "currentIndex" of "empty" is 0
    And the property "currentIndex" of "none" is 0

  Scenario: A bound current index brought round by the view still follows its binding
    Then the property "currentIndex" of "bound" is 9
    When the property "wanted" of "root" is set to 4
    Then the property "currentIndex" of "bound" is 4
