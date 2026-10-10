# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-gridlayout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An attached value that is given later
  Several attached properties mean one thing until they are given a value
  and another once they are, also when the value given is the one the
  property already read as: a side margin follows Layout.margins, a layout
  inside a layout fills, a cell is chosen by the grid, and mirroring comes
  from the parent.

  Scenario: A side margin of 0 no longer follows Layout.margins
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      RowLayout {
        id: root; objectName: "root"
        width: 400; spacing: 0
        Rectangle {
          id: a; objectName: "a"; implicitWidth: 50; implicitHeight: 10
          Layout.margins: 10
        }
        Rectangle {
          objectName: "b"; implicitWidth: 50; implicitHeight: 10
          Layout.fillWidth: true
        }
        function sideMargin() { a.Layout.leftMargin = 0 }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "a" is 10
    When the method "sideMargin" of "root" is called
    And 100 milliseconds pass
    Then the property "x" of "a" is 0
    And the property "x" of "b" is 60

  Scenario: A layout inside a layout that is told not to fill does not
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      RowLayout {
        id: root; objectName: "root"
        width: 400; spacing: 0
        RowLayout {
          id: inner; objectName: "inner"
          Layout.alignment: Qt.AlignLeft
          Rectangle { implicitWidth: 50; implicitHeight: 10; Layout.fillWidth: true }
        }
        function noFill() { inner.Layout.fillWidth = false }
      }
      """
    When 100 milliseconds pass
    Then the property "width" of "inner" is 400
    When the method "noFill" of "root" is called
    And 100 milliseconds pass
    Then the property "width" of "inner" is 50

  Scenario: A minimum width of 0 takes the place of the one the content gave
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      RowLayout {
        id: root; objectName: "root"
        width: 50; spacing: 0
        RowLayout {
          id: tight; objectName: "tight"
          Layout.alignment: Qt.AlignLeft
          Rectangle {
            implicitWidth: 120; implicitHeight: 10
            Layout.minimumWidth: 100; Layout.fillWidth: true
          }
        }
        function noMinimum() { tight.Layout.minimumWidth = 0 }
      }
      """
    When 100 milliseconds pass
    Then the property "width" of "tight" is 100
    When the method "noMinimum" of "root" is called
    And 100 milliseconds pass
    Then the property "width" of "tight" is 50

  Scenario: Mirroring switched off for a child whose parent passes it on
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 100
        LayoutMirroring.enabled: true; LayoutMirroring.childrenInherit: true
        Rectangle {
          id: kid; objectName: "kid"; width: 10; height: 10
          anchors.left: parent.left
          LayoutMirroring.childrenInherit: false
        }
        function unmirrored() { kid.LayoutMirroring.enabled = false }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "kid" is 90
    When the method "unmirrored" of "root" is called
    And 100 milliseconds pass
    Then the property "x" of "kid" is 0

  Rule: A cell given to a child the grid had placed is where the child goes

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          id: root; objectName: "root"
          columns: 2; rowSpacing: 0; columnSpacing: 0
          Rectangle { implicitWidth: 10; implicitHeight: 10 }
          Rectangle {
            id: second; objectName: "second"; implicitWidth: 10; implicitHeight: 10
            Layout.alignment: Qt.AlignLeft
          }
          Rectangle { objectName: "third"; implicitWidth: 10; implicitHeight: 10 }
          Rectangle {
            id: fourth; objectName: "fourth"; implicitWidth: 10; implicitHeight: 10
            Layout.alignment: Qt.AlignLeft
          }
          function firstRow() { fourth.Layout.row = 0 }
          function firstColumn() { second.Layout.column = 0 }
        }
        """
      When 100 milliseconds pass

    Scenario: Row 0 given to a child the grid had put in the second row
      Then the property "y" of "fourth" is 10
      And the property "x" of "fourth" is 10
      When the method "firstRow" of "root" is called
      And 100 milliseconds pass
      Then the property "y" of "fourth" is 0
      And the property "x" of "fourth" is 0

    Scenario: Column 0 given to a child the grid had put in the second column
      Then the property "x" of "second" is 10
      When the method "firstColumn" of "root" is called
      And 100 milliseconds pass
      Then the property "x" of "second" is 0
      And the property "x" of "third" is 10
      And the property "y" of "third" is 0
