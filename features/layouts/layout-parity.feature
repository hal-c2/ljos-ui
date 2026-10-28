# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-rowlayout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html#fillWidth-attached-prop
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html#margins-attached-prop
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html#alignment-attached-prop
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html#fillHeight-attached-prop
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-gridlayout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html#columnSpan-attached-prop
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html#rowSpan-attached-prop
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

  Scenario: Cross-axis margins raise RowLayout.implicitHeight and reduce only that child's filled height
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string log: ""
        RowLayout {
          id: rl; objectName: "rl"; spacing: 10
          Item {
            id: a; Layout.preferredWidth: 30; Layout.preferredHeight: 25
            Layout.topMargin: 12; Layout.bottomMargin: 18
            Layout.fillHeight: true
          }
          Item {
            id: b; Layout.preferredWidth: 30; Layout.preferredHeight: 40
            Layout.fillHeight: true
          }
          Item {
            id: c; Layout.preferredWidth: 30; Layout.preferredHeight: 20
            Layout.topMargin: 10; Layout.bottomMargin: 5
            Layout.alignment: Qt.AlignVCenter
          }
        }
        Component.onCompleted: log = rl.implicitHeight + ":" + a.y + "," + a.height + ";" + b.y + "," + b.height + ";" + c.y + "," + c.height
      }
      """
    Then the property "log" of "root" is "55:12,25;0,55;20,20"

  Scenario: Hiding the child with large vertical margins drops RowLayout.implicitHeight to the next tallest
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string log: ""
        RowLayout {
          id: rl; spacing: 10
          Item {
            id: a; Layout.preferredWidth: 30; Layout.preferredHeight: 25
            Layout.topMargin: 20; Layout.bottomMargin: 15
          }
          Item { id: b; Layout.preferredWidth: 30; Layout.preferredHeight: 40 }
        }
        function hideA() {
          log = "" + rl.implicitHeight
          a.visible = false
          log += "," + rl.implicitHeight
        }
      }
      """
    When the method "hideA" of "root" is called
    Then the property "log" of "root" is "60,60"

  Scenario: A columnSpan of 2 absorbs columnSpacing when widening the spanned columns
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string log: ""
        GridLayout {
          id: gl; objectName: "gl"; columns: 2; columnSpacing: 20; rowSpacing: 10
          Item { id: c0; Layout.preferredWidth: 30; Layout.preferredHeight: 15 }
          Item { id: c1; Layout.preferredWidth: 30; Layout.preferredHeight: 15 }
          Item { id: span; Layout.columnSpan: 2; Layout.preferredWidth: 100; Layout.preferredHeight: 15 }
        }
        Component.onCompleted: log = c0.width + "," + c1.x + "," + c1.width + "," + span.width + "," + gl.implicitWidth
      }
      """
    Then the property "log" of "root" is "30,60,30,100,100"

  Scenario: Auto-placed items in row 1 skip middle column 1 occupied by a rowSpan from row 0
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string log: ""
        GridLayout {
          id: gl; columns: 3; columnSpacing: 5; rowSpacing: 5
          Item { id: a; Layout.preferredWidth: 20; Layout.preferredHeight: 10 }
          Item { id: b; Layout.rowSpan: 2; Layout.preferredWidth: 30; Layout.preferredHeight: 35 }
          Item { id: c; Layout.preferredWidth: 40; Layout.preferredHeight: 10 }
          Item { id: d; Layout.preferredWidth: 20; Layout.preferredHeight: 10 }
          Item { id: e; Layout.preferredWidth: 40; Layout.preferredHeight: 10 }
        }
        Component.onCompleted: log = d.x + "," + d.y + ";" + e.x + "," + e.y + ";" + gl.implicitWidth + "," + gl.implicitHeight
      }
      """
    Then the property "log" of "root" is "0,23;60,23;100,35"
