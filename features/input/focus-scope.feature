# Sources:
#   https://doc.qt.io/qt-6/qtquick-input-focus.html#acquiring-focus-and-focus-scopes
#   https://doc.qt.io/qt-6/qml-qtquick-focusscope.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#forceActiveFocus-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: FocusScope
  A focus scope remembers which of its items wants focus. That item has
  active focus only while the scope itself has it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string active:
          Window.activeFocusItem ? Window.activeFocusItem.objectName : ""
        property string order: ""
        function adopt() { loose.parent = scope }
        function release() { field.parent = root }
        Item { objectName: "outside"; focus: true }
        FocusScope {
          id: scope
          objectName: "scope"
          onActiveFocusChanged: root.order += "scope "
          Item {
            id: field
            objectName: "field"; focus: true
            onActiveFocusChanged: root.order += "field "
          }
          Item { objectName: "other" }
        }
        Item { id: loose; objectName: "loose" }
      }
      """

  Scenario: An item in a scope without focus has focus but not active focus
    Then the property "focus" of "field" is true
    And the property "activeFocus" of "field" is false
    And the property "focus" of "scope" is false
    And the property "active" of "root" is "outside"

  Scenario: Giving the scope focus gives its item active focus
    When the property "focus" of "scope" is set to true
    Then the property "activeFocus" of "scope" is true
    And the property "activeFocus" of "field" is true
    And the property "active" of "root" is "field"
    And the property "focus" of "outside" is false
    And the property "order" of "root" is "scope field "

  Scenario: When focus leaves the scope its item keeps focus for next time
    When the property "focus" of "scope" is set to true
    And the property "focus" of "outside" is set to true
    Then the property "focus" of "field" is true
    And the property "activeFocus" of "field" is false
    And the property "focus" of "scope" is false
    And the property "activeFocus" of "scope" is false
    And the property "order" of "root" is "scope field field scope "

  Scenario: Focus moves inside a scope without changing who has active focus
    When the property "focus" of "other" is set to true
    Then the property "focus" of "field" is false
    And the property "activeFocus" of "other" is false
    And the property "active" of "root" is "outside"

  Scenario: forceActiveFocus on an item also gives its scope focus
    When the method "forceActiveFocus" of "other" is called
    Then the property "activeFocus" of "other" is true
    And the property "focus" of "scope" is true
    And the property "activeFocus" of "scope" is true
    And the property "focus" of "field" is false
    And the property "focus" of "outside" is false

  Scenario: A scope whose item gives up focus has active focus itself
    When the property "focus" of "scope" is set to true
    And the property "focus" of "field" is set to false
    Then the property "active" of "root" is "scope"
    And the property "activeFocus" of "scope" is true

  Scenario: A disabled scope takes active focus from its item
    When the property "focus" of "scope" is set to true
    And the property "enabled" of "scope" is set to false
    Then the property "activeFocus" of "field" is false
    And the property "focus" of "field" is true
    And the property "focus" of "scope" is true
    And the property "active" of "root" is ""

  Scenario: An item moved into a scope without a focus item keeps focus
    When the property "focus" of "field" is set to false
    And the property "focus" of "loose" is set to true
    And the method "adopt" of "root" is called
    Then the property "focus" of "loose" is true
    And the property "activeFocus" of "loose" is false
    And the property "active" of "root" is ""

  Scenario: An item moved into a scope that has a focus item loses focus
    When the property "focus" of "loose" is set to true
    And the method "adopt" of "root" is called
    Then the property "focus" of "loose" is false
    And the property "focus" of "field" is true
    And the property "active" of "root" is ""

  Scenario: An item moved out of its scope takes its focus along
    When the property "focus" of "scope" is set to true
    And the method "release" of "root" is called
    Then the property "focus" of "field" is false
    And the property "active" of "root" is "scope"
