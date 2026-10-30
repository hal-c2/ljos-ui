# Sources:
#   https://doc.qt.io/qt-6/qtquick-positioning-anchors.html
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#completed-signal
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Anchors hold when a component completes
  An item's anchors give its geometry as the component that made it
  completes, so a Component.onCompleted handler reads the anchored size and
  place, not the declared ones, before any frame is drawn.

  Scenario: A filled item reads its parent's size as it completes
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"; width: 100; height: 100
        property real w: -1
        Item {
          anchors.fill: parent
          Component.onCompleted: w = width
        }
      }
      """
    Then the property "w" of "root" is 100

  Scenario: A centred item reads its centred place as it completes
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"; width: 100; height: 100
        property real cx: -1
        Rectangle {
          width: 10; height: 10
          anchors.centerIn: parent
          Component.onCompleted: cx = x
        }
      }
      """
    Then the property "cx" of "root" is 45

  Scenario: An item anchored to a sibling reads its place as it completes
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"; width: 200; height: 100
        property real lx: -1
        Item { id: first; x: 20; width: 30; height: 10 }
        Item {
          width: 10; height: 10
          anchors.left: first.right
          Component.onCompleted: lx = x
        }
      }
      """
    Then the property "lx" of "root" is 50

  Scenario: A Loader's item and its child read their anchored size as they complete
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"; width: 100; height: 100
        property real w: -1
        property real cw: -1
        Loader {
          objectName: "loader"
          anchors.fill: parent
          active: false
          sourceComponent: Item {
            anchors.fill: parent
            Component.onCompleted: w = width
            Item {
              anchors.fill: parent
              Component.onCompleted: cw = width
            }
          }
        }
      }
      """
    When the property "active" of "loader" is set to true
    Then the property "w" of "root" is 100
    And the property "cw" of "root" is 100
