# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-rowlayout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html#fillWidth-attached-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Layout parity

  Scenario: Two-stage maximumWidth cascade across three filling items in RowLayout
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"; width: 400; height: 100
        property string log: ""
        RowLayout {
          id: rl; width: 300; spacing: 0
          Item { id: a; implicitHeight: 20; Layout.fillWidth: true; Layout.preferredWidth: 50; Layout.maximumWidth: 60 }
          Item { id: b; implicitHeight: 20; Layout.fillWidth: true; Layout.preferredWidth: 50; Layout.maximumWidth: 110 }
          Item { id: c; implicitHeight: 20; Layout.fillWidth: true; Layout.preferredWidth: 50 }
        }
        Component.onCompleted: log = a.width + "," + b.width + "," + c.width + ":" + c.x
      }
      """
    Then the property "log" of "root" is "60,110,130:170"

  Scenario: Two-stage minimumWidth cascade when shrinking three filling items in RowLayout
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"; width: 400; height: 100
        property string log: ""
        RowLayout {
          id: rl; width: 150; spacing: 0
          Item { id: a; implicitHeight: 20; Layout.fillWidth: true; Layout.preferredWidth: 100; Layout.minimumWidth: 90 }
          Item { id: b; implicitHeight: 20; Layout.fillWidth: true; Layout.preferredWidth: 100; Layout.minimumWidth: 40 }
          Item { id: c; implicitHeight: 20; Layout.fillWidth: true; Layout.preferredWidth: 100; Layout.minimumWidth: 0 }
        }
        Component.onCompleted: log = a.width + "," + b.width + "," + c.width + ":" + c.x
      }
      """
    Then the property "log" of "root" is "93,48,9:141"
