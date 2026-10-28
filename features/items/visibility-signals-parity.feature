# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#completed-signal
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Visibility signal parity

  Scenario: A child inside a statically hidden parent reads visible as false in Component.onCompleted with no signal
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property bool seenInCompleted: true
        property int childSignals: 0
        Item {
          id: hiddenParent; objectName: "hiddenParent"; visible: false
          Item {
            id: child; objectName: "child"
            onVisibleChanged: root.childSignals++
            Component.onCompleted: root.seenInCompleted = visible
          }
        }
      }
      """
    Then the property "seenInCompleted" of "root" is false
    And the property "childSignals" of "root" is 1
    When the property "visible" of "hiddenParent" is set to true
    Then the property "visible" of "child" is true
    And the property "childSignals" of "root" is 2
