# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html (Focus and key events)
#   https://doc.qt.io/qt-6/qtquick-input-focus.html#acquiring-focus-and-focus-scopes
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#forceActiveFocus-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A Loader is a focus scope
  Each loaded item keeps the focus it asks for inside its own Loader, so
  two loaded items that both want focus do not take it from each other.
  A loaded item has active focus only while its Loader has focus.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 100
        property string active:
          Window.activeFocusItem ? Window.activeFocusItem.objectName : ""
        Loader {
          objectName: "left"; focus: true
          sourceComponent: Rectangle {
            objectName: "a"; focus: true; width: 100; height: 100
          }
        }
        Loader {
          objectName: "right"; x: 100
          sourceComponent: Rectangle {
            objectName: "b"; focus: true; width: 100; height: 100
          }
        }
      }
      """

  Scenario: Both loaded items keep focus in their own Loader
    Then the property "focus" of "a" is true
    And the property "focus" of "b" is true
    And the property "focus" of "left" is true
    And the property "focus" of "right" is false

  Scenario: Only the item of the Loader with focus has active focus
    Then the property "activeFocus" of "a" is true
    And the property "activeFocus" of "b" is false
    And the property "active" of "root" is "a"

  Scenario: forceActiveFocus on a loaded item gives its Loader focus
    When the method "forceActiveFocus" of "b" is called
    Then the property "activeFocus" of "b" is true
    And the property "focus" of "right" is true
    And the property "activeFocus" of "right" is true
    And the property "focus" of "left" is false
    And the property "active" of "root" is "b"

  Scenario: The other loaded item keeps focus for when its Loader has it again
    When the method "forceActiveFocus" of "b" is called
    Then the property "focus" of "a" is true
    And the property "activeFocus" of "a" is false
    When the property "focus" of "left" is set to true
    Then the property "activeFocus" of "a" is true
    And the property "focus" of "b" is true
    And the property "activeFocus" of "b" is false
    And the property "active" of "root" is "a"

  Scenario: A Loader whose item gives up focus has active focus itself
    When the property "focus" of "a" is set to false
    Then the property "activeFocus" of "left" is true
    And the property "active" of "root" is "left"
