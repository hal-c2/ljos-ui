# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-stacklayout.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: StackLayout
  A StackLayout shows one of its children at a time, at the layout's size.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        StackLayout {
          objectName: "stack"
          width: 200; height: 100
          Item { objectName: "a"; implicitWidth: 40; implicitHeight: 10 }
          Item { objectName: "b"; implicitWidth: 60; implicitHeight: 30 }
          Item { objectName: "c"; implicitWidth: 20; implicitHeight: 50; Layout.fillWidth: false }
        }
        StackLayout { objectName: "empty" }
      }
      """
    When 100 milliseconds pass

  Scenario: The first child is shown and the others are hidden
    Then the property "currentIndex" of "stack" is 0
    And the property "count" of "stack" is 3
    And the property "visible" of "a" is true
    And the property "visible" of "b" is false
    And the property "visible" of "c" is false

  Scenario: The shown child gets the layout's size
    Then the property "width" of "a" is 200
    And the property "height" of "a" is 100

  Scenario: Changing the current index shows another child
    When the property "currentIndex" of "stack" is set to 1
    And 100 milliseconds pass
    Then the property "visible" of "a" is false
    And the property "visible" of "b" is true
    And the property "width" of "b" is 200
    And the property "height" of "b" is 100

  Scenario: A child told not to fill keeps its preferred width
    When the property "currentIndex" of "stack" is set to 2
    And 100 milliseconds pass
    Then the property "visible" of "c" is true
    And the property "width" of "c" is 20
    And the property "height" of "c" is 100

  Scenario: The layout prefers the size of its largest child
    Then the property "implicitWidth" of "stack" is 60
    And the property "implicitHeight" of "stack" is 50

  Scenario: An index with no child hides them all
    When the property "currentIndex" of "stack" is set to 7
    And 100 milliseconds pass
    Then the property "currentIndex" of "stack" is 7
    And the property "visible" of "a" is false
    And the property "visible" of "b" is false
    And the property "visible" of "c" is false

  Scenario: An empty StackLayout has no current child
    Then the property "currentIndex" of "empty" is -1
    And the property "count" of "empty" is 0
