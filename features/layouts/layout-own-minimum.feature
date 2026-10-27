# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html#minimumWidth-attached-prop
# Verified on Qt 6.11.2 (qml6).

@qt-parity
Feature: A layout with no minimum of its own reads the minimum its children need
  `Layout.minimumWidth` and `Layout.minimumHeight` of a layout nested in
  another read the least its children take while they are unset. A
  sibling sized from them fits.

  Rule: A row layout

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root
          objectName: "root"
          width: 300; height: 300
          property real innerMinimum: inner.Layout.minimumWidth
          property real sideEnd: side.x + side.width
          property real leafMinimum: leaf.Layout.minimumWidth
          RowLayout {
            id: outer; width: 300; height: 100
            spacing: 0
            RowLayout {
              id: inner
              Layout.fillWidth: true
              spacing: 0
              Rectangle { id: leaf; implicitWidth: 120; implicitHeight: 20 }
            }
            Rectangle {
              id: side
              Layout.preferredWidth: outer.width - inner.Layout.minimumWidth
              implicitHeight: 20
            }
          }
        }
        """

    Scenario: A row layout's minimum width is what its children need
      Then the property "innerMinimum" of "root" is 120
      And the property "sideEnd" of "root" is 300

    Scenario: An item that is no layout has no minimum of its own
      Then the property "leafMinimum" of "root" is 0

  Rule: A column layout

    Scenario: A column layout's minimum height is what its children need, spacing in
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root
          objectName: "root"
          width: 300; height: 200
          property real stackMinimum: stack.Layout.minimumHeight
          ColumnLayout {
            width: 300; height: 200
            spacing: 0
            ColumnLayout {
              id: stack
              Layout.fillHeight: true
              spacing: 5
              Rectangle { implicitWidth: 20; implicitHeight: 30 }
              Rectangle { implicitWidth: 20; implicitHeight: 40 }
            }
          }
        }
        """
      Then the property "stackMinimum" of "root" is 75
