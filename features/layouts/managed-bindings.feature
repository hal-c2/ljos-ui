# Sources:
#   https://doc.qt.io/qt-6/qtquicklayouts-overview.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-rowlayout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-stacklayout.html
#   https://doc.qt.io/qt-6/qtquick-positioning-layouts.html
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Bindings on what a layout or a positioner places
  A layout sets the position and size of its children and a positioner
  their position. That is not an assignment: a binding on the property
  stays, and gives the property its value again when one of its inputs
  changes, until the layout or the positioner next places the child.

  Rule: A layout leaves the bindings of its children in place

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root
          objectName: "root"
          width: 400; height: 100
          property real w: 50
          property real px: 7
          RowLayout {
            objectName: "layout"
            width: 400; spacing: 0
            Rectangle { objectName: "a"; width: root.w; height: 20 }
            Rectangle { objectName: "b"; Layout.fillWidth: true; height: 20 }
          }
          RowLayout {
            objectName: "other"
            y: 50; width: 400; spacing: 0
            Rectangle {
              objectName: "g"; x: root.px; width: root.w; height: 20
              Layout.preferredWidth: 80
            }
          }
        }
        """
      When 100 milliseconds pass

    Scenario: A bound width follows its input after the layout placed the child
      Then the property "width" of "a" is 50
      When the property "w" of "root" is set to 60
      Then the property "width" of "a" is 60

    Scenario: The layout gives the child its size again when it next shares its width out
      When the property "w" of "root" is set to 60
      And the property "width" of "layout" is set to 300
      Then the property "width" of "a" is 50
      And the property "width" of "b" is 250
      When the property "w" of "root" is set to 70
      Then the property "width" of "a" is 70

    Scenario: A width the layout chose is replaced by the binding's next value
      Then the property "width" of "g" is 80
      And the property "x" of "g" is 0
      When the property "w" of "root" is set to 60
      Then the property "width" of "g" is 60
      When the property "px" of "root" is set to 9
      Then the property "x" of "g" is 9
      When the property "width" of "other" is set to 300
      Then the property "width" of "g" is 80
      And the property "x" of "g" is 0
      When the property "w" of "root" is set to 70
      Then the property "width" of "g" is 70

    Scenario: An assignment removes the binding, as it does anywhere
      When the property "width" of "a" is set to 33
      And the property "w" of "root" is set to 90
      Then the property "width" of "a" is 33

  Rule: A positioner leaves the bindings of its children in place

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 100
          property real w: 50
          property real px: 7
          Row {
            Rectangle { objectName: "c"; x: root.px; width: root.w; height: 10 }
            Rectangle { objectName: "d"; width: 10; height: 10 }
          }
          Column {
            y: 50
            Rectangle { objectName: "e"; x: root.px; y: root.px; width: 10; height: root.w }
            Rectangle { objectName: "f"; width: 10; height: 10 }
          }
        }
        """
      When 100 milliseconds pass

    Scenario: A bound x in a Row follows its input until the Row positions again
      Then the property "x" of "c" is 0
      When the property "px" of "root" is set to 9
      Then the property "x" of "c" is 9
      When the property "w" of "root" is set to 70
      When 100 milliseconds pass
      Then the property "x" of "c" is 0
      And the property "x" of "d" is 70
      When the property "px" of "root" is set to 11
      Then the property "x" of "c" is 11

    Scenario: A Column places along y and leaves x to its binding
      Then the property "y" of "e" is 0
      And the property "x" of "e" is 7
      When the property "px" of "root" is set to 9
      Then the property "y" of "e" is 9
      And the property "x" of "e" is 9
      When the property "w" of "root" is set to 70
      When 100 milliseconds pass
      Then the property "y" of "e" is 0
      And the property "y" of "f" is 70
      And the property "x" of "e" is 9

  Rule: A StackLayout leaves a binding on a page's visible in place

    Scenario: The binding shows a page the layout had hidden
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          id: root
          objectName: "root"
          property bool shown: true
          StackLayout {
            objectName: "stack"
            Rectangle { objectName: "first"; visible: root.shown }
            Rectangle { objectName: "second"; visible: root.shown }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "visible" of "second" is false
      When the property "shown" of "root" is set to false
      Then the property "visible" of "first" is false
      When the property "shown" of "root" is set to true
      Then the property "visible" of "second" is true
      When the property "currentIndex" of "stack" is set to 1
      Then the property "visible" of "first" is false
      And the property "visible" of "second" is true

  Rule: Placing a child is not animated by a Behavior

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          width: 400; height: 100
          Row {
            Rectangle { objectName: "lead"; width: 30; height: 10 }
            Rectangle {
              objectName: "moved"; width: 10; height: 10
              Behavior on x { NumberAnimation { duration: 2000 } }
            }
          }
          RowLayout {
            objectName: "layout"
            y: 50; width: 400; spacing: 0
            Rectangle {
              objectName: "sized"; implicitWidth: 10; height: 10; Layout.fillWidth: true
              Behavior on width { NumberAnimation { duration: 2000 } }
            }
            Rectangle { implicitWidth: 10; height: 10 }
          }
        }
        """
      When 100 milliseconds pass

    Scenario: A Row moves a child with a Behavior on x at once
      Then the property "x" of "moved" is 30
      When the property "width" of "lead" is set to 100
      When 100 milliseconds pass
      Then the property "x" of "moved" is 100

    Scenario: A layout sizes a child with a Behavior on width at once
      Then the property "width" of "sized" is 390
      When the property "width" of "layout" is set to 200
      Then the property "width" of "sized" is 190
