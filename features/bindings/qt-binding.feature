# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html (Qt.binding)
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Assignments and Qt.binding in JavaScript
  A plain assignment in a function replaces a binding; Qt.binding installs a new one.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int a: 1
        property int b: a * 2
        function freeze() { b = b }
        function follow() { b = Qt.binding(function() { return a * 3 }) }
      }
      """

  Scenario: Assigning in a function breaks the binding
    When the method "freeze" of "root" is called
    And the property "a" of "root" is set to 5
    Then the property "b" of "root" is 2

  Scenario: Qt.binding in a function installs a binding
    When the method "follow" of "root" is called
    And the property "a" of "root" is set to 5
    Then the property "b" of "root" is 15

  Scenario: Qt.binding replaces a broken binding
    When the method "freeze" of "root" is called
    And the method "follow" of "root" is called
    And the property "a" of "root" is set to 2
    Then the property "b" of "root" is 6
