# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html
#   https://doc.qt.io/qt-6/qtquick-input-handlers.html
# The Qt oracle cannot inject pointer input, so these run on Ljós only.

@ljos-only
Feature: TapHandler
  A TapHandler reports taps on its parent item without blocking items below.

  Scenario: A tap fires tapped and singleTapped
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 200
        property int taps: 0
        property int singles: 0
        Item {
          width: 100; height: 100
          TapHandler { onTapped: root.taps++; onSingleTapped: root.singles++ }
        }
        id: root
      }
      """
    When the user touches 10, 10
    Then the property "taps" of "root" is 1
    And the property "singles" of "root" is 1

  Scenario: A second tap soon after counts as a double tap
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 200
        property int doubles: 0
        Item {
          width: 100; height: 100
          TapHandler { onDoubleTapped: root.doubles++ }
        }
        id: root
      }
      """
    When the user touches 10, 10
    And the user touches 12, 10
    Then the property "doubles" of "root" is 1

  Scenario: The default gesture policy does not block a MouseArea below
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 200
        property int clicks: 0
        property int taps: 0
        MouseArea { width: 100; height: 100; onClicked: root.clicks++ }
        Item {
          width: 100; height: 100
          TapHandler { onTapped: root.taps++ }
        }
        id: root
      }
      """
    When the user touches 10, 10
    Then the property "taps" of "root" is 1
    And the property "clicks" of "root" is 1
