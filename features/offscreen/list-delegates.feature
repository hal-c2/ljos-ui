# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (cacheBuffer, reuseItems)
#   docs/ARCHITECTURE.md section 8.4
# Verified on Qt 6.11.2 (qml6, offscreen).
# Delegate counts are bands, never exact numbers.

@qt-parity
Feature: Delegates exist only near the viewport
  A long list costs what is visible, not what it holds.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 300
        property int created: 0
        property int destroyed: 0
        readonly property int live: created - destroyed
        ListView {
          objectName: "list"
          anchors.fill: parent
          cacheBuffer: 0
          model: 10000
          delegate: Item {
            width: 200; height: 30
            Component.onCompleted: root.created++
            Component.onDestruction: root.destroyed++
          }
        }
      }
      """

  Scenario: Only delegates near the viewport are created
    Then the property "live" of "root" is at most 14

  Scenario: Scrolling far away recycles delegates instead of piling them up
    When the property "contentY" of "list" is set to 150000
    Then the property "live" of "root" is at most 14
