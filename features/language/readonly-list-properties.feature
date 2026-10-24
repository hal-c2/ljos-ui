# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#read-only-properties
#   https://doc.qt.io/qt-6/qml-list.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings), buckets error-kind:TypeError/Error:step-push(literal)
# and value:list<string>:step-push(literal).

@qt-parity
Feature: A read-only list property
  A `readonly` list property cannot be given another list, and its entries cannot be changed
  either: the list a script reads from it is read-only too.

  @backlog
  Scenario: push on a read-only list is a TypeError and adds nothing
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property list<string> names: []
        property string seen: ""
        function add() {
          try { names.push("a") } catch (e) { seen = e.name }
        }
      }
      """
    When the method "add" of "root" is called
    Then the property "seen" of "root" is "TypeError"
    And the property "names" of "root" is []
