# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html (Qt.binding)
#   docs/research/09-bdd-feature-files.md
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Property bindings
  A bound property follows the properties it reads until something assigns to it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int a: 1
        property int b: a + 1
      }
      """

  Scenario: A binding follows its dependencies
    When the property "a" of "root" is set to 5
    Then the property "b" of "root" is 6

  Scenario: A binding is evaluated once at creation
    Then the property "b" of "root" is 2

  Scenario: Assigning a value breaks the binding
    When the property "b" of "root" is set to 100
    And the property "a" of "root" is set to 8
    Then the property "b" of "root" is 100

  Scenario: Qt.binding puts a binding back
    When the property "b" of "root" is set to 100
    And the property "b" of "root" is bound to "a + 1"
    And the property "a" of "root" is set to 9
    Then the property "b" of "root" is 10

  Scenario: Rebinding replaces the old dependency
    When the property "b" of "root" is bound to "a * 10"
    And the property "a" of "root" is set to 3
    Then the property "b" of "root" is 30
