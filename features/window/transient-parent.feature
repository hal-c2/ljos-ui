# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A window that belongs to another
  A window declared inside another is transient for it: it is a window of
  its own, and while it has the keyboard the window it belongs to still
  counts as active.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        id: outer
        objectName: "outer"
        width: 300; height: 200
        visible: true
        readonly property bool ownsInner: inner.transientParent === outer
        TextInput { objectName: "outerInput"; focus: true }
        Window {
          id: inner
          objectName: "inner"
          width: 100; height: 100
          TextInput { objectName: "innerInput"; focus: true }
        }
      }
      """

  @api-QtQuick.Window.transientParent
  Scenario: The window it is declared in is its transient parent
    Then the property "ownsInner" of "outer" is true

  @api-QtQuick.Window.visible
  Scenario: It is not shown with the window it belongs to
    Then the property "visible" of "inner" is false
    And the property "active" of "inner" is false

  @api-QtQuick.Window.active
  Scenario: Showing it gives it the keyboard
    When the method "show" of "inner" is called
    And 50 milliseconds pass
    Then the property "active" of "inner" is true
    And the property "activeFocus" of "innerInput" is true
    And the property "activeFocus" of "outerInput" is false

  @api-QtQuick.Window.active
  Scenario: The window it belongs to stays active
    When the method "show" of "inner" is called
    And 50 milliseconds pass
    Then the property "active" of "outer" is true
