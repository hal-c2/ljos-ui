# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html#icon-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Assigning a control's icon as a whole
  icon is a grouped value: assigning another control's icon copies its members.

  Scenario: icon: other.icon copies the name
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        Button { id: src; icon.name: "go-next" }
        Button { objectName: "dst"; icon: src.icon }
      }
      """
    Then the property "icon.name" of "dst" is "go-next"
