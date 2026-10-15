# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#smooth-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#antialiasing-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Item.smooth
  smooth is true by default on every item and can be switched off.

  Scenario: Smooth defaults to true and can be set on a Rectangle
    Given the QML:
      """qml
      import QtQuick
      Item {
        Rectangle { objectName: "a" }
        Rectangle { objectName: "b"; smooth: false }
      }
      """
    Then the property "smooth" of "a" is true
    And the property "smooth" of "b" is false
