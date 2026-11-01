# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#contentData-prop
#   https://doc.qt.io/qt-6/qtquick-positioning-anchors.html#restrictions
# Verified on Qt 6.11.2 (qml6, offscreen).
@qt-parity
Feature: Anchors between what is declared in a control

  What is declared in a popup or a pane are siblings under its content item,
  so they can anchor to each other.

  Scenario Outline: Siblings in a <host> anchor to each other without a warning
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        width: 200; height: 200
        <host> {
          width: 100; height: 100
          Item { id: first; x: 7; width: 10; height: 10 }
          Item { objectName: "second"; anchors.left: first.right }
        }
      }
      """
    Then the log has 0 warnings matching "anchor"

    Examples:
      | host  |
      | Popup |
      | Pane  |
