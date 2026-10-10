# Sources:
#   https://doc.qt.io/qt-6/qtquicklayouts-overview.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Layouts, positioners and Loaders inside a layout
  A layout inside a layout fills by default, between the minimum and maximum
  sizes its own children give it. A positioner or a Loader inside a layout is
  an item with an implicit size.

  Scenario: A layout whose children cannot grow keeps its preferred width
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      RowLayout {
        width: 300; height: 40
        RowLayout {
          objectName: "inner"
          Item { objectName: "a"; width: 50; height: 20 }
          Item { objectName: "b"; width: 50; height: 20 }
        }
        Item { objectName: "c"; width: 40; height: 20 }
      }
      """
    When 100 milliseconds pass
    Then the property "width" of "inner" is 105
    And the property "x" of "b" is 55
    And the property "x" of "c" is 219

  Rule: A layout with a filling child fills its cell

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "outer"
          width: 300; height: 40
          RowLayout {
            objectName: "inner"
            Item { objectName: "a"; width: 50; height: 20 }
            Item { objectName: "b"; height: 20; Layout.fillWidth: true; Layout.minimumWidth: 20 }
          }
          Item { objectName: "c"; width: 40; height: 20 }
        }
        """
      When 100 milliseconds pass

    Scenario: The inner layout takes the room the outer one has left
      Then the property "width" of "inner" is 255
      And the property "width" of "b" is 200
      And the property "x" of "c" is 260

    Scenario: The inner layout's minimum width is the sum of its children's
      When the property "width" of "outer" is set to 100
      And 100 milliseconds pass
      Then the property "width" of "inner" is 75
      And the property "width" of "b" is 20
      And the property "x" of "c" is 80

    Scenario: The outer layout prefers the sum of everything inside it
      Then the property "implicitWidth" of "outer" is 120
      And the property "implicitWidth" of "inner" is 75

  Rule: A child that does not fill is sized by what it holds

    Scenario: A layout told not to fill keeps its preferred size and is aligned like any child
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          width: 300; height: 40
          ColumnLayout {
            objectName: "inner"
            Layout.fillWidth: false; Layout.fillHeight: false
            Item { objectName: "a"; width: 50; height: 10 }
            Item { objectName: "b"; height: 10; Layout.fillWidth: true }
          }
          Item { objectName: "c"; width: 40; height: 20 }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "inner" is 50
      And the property "height" of "inner" is 25
      And the property "y" of "inner" is 8
      And the property "width" of "b" is 50
      And the property "x" of "c" is 169

    Scenario: A positioner in a layout is laid out by its implicit size
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "outer"
          Row {
            objectName: "row"
            spacing: 2
            Item { width: 10; height: 12 }
            Item { width: 20; height: 8 }
          }
          Item { objectName: "after"; width: 40; height: 20 }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "row" is 32
      And the property "x" of "after" is 37
      And the property "implicitWidth" of "outer" is 77
      And the property "y" of "row" is 4

  Rule: A Loader in a layout stands for its item

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "outer"
          width: 300; height: 40
          Component { id: box; Item { width: 30; height: 10 } }
          Component { id: sized; Item { implicitWidth: 30; implicitHeight: 10 } }
          Loader { objectName: "plain"; sourceComponent: box }
          Loader {
            objectName: "filling"; sourceComponent: sized; Layout.fillWidth: true
            property real itemWidth: item ? item.width : -1
          }
          Item { objectName: "last"; width: 40; height: 20 }
        }
        """
      When 100 milliseconds pass

    Scenario: A Loader takes its item's size as its implicit size
      Then the property "implicitWidth" of "plain" is 30
      And the property "implicitHeight" of "plain" is 10
      And the property "width" of "plain" is 30
      And the property "x" of "filling" is 35

    Scenario: A filling Loader gives the width it gets to its item
      Then the property "width" of "filling" is 220
      And the property "itemWidth" of "filling" is 220
      And the property "implicitWidth" of "filling" is 30
      And the property "x" of "last" is 260
