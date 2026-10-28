# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#active-prop
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#status-prop
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#loaded-signal
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Loader lifecycle parity

  Scenario: Deactivating and reactivating a hidden Loader runs destruction and completion with zero visibleChanged signals
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int created: 0
        property int destroyed: 0
        property int visSignals: 0
        Component {
          id: comp
          Item {
            objectName: "inner"
            Component.onCompleted: root.created++
            Component.onDestruction: root.destroyed++
            onVisibleChanged: root.visSignals++
          }
        }
        Loader {
          id: loader; objectName: "loader"
          visible: false; sourceComponent: comp
        }
      }
      """
    When the property "active" of "loader" is set to false
    Then the property "status" of "loader" is 0
    And the property "destroyed" of "root" is 1
    When the property "active" of "loader" is set to true
    Then the property "status" of "loader" is 1
    And the property "created" of "root" is 2
    And the property "visSignals" of "root" is 2
