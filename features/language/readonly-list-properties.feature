# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#read-only-properties
#   https://doc.qt.io/qt-6/qml-list.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings), buckets error-kind:TypeError/Error:step-push(literal),
# value:list<string>:step-push(literal), value:list<real>:step-set(string-pair),
# value:var(array):step-set(literal) and value:var(string):step-set(string-pair).

@qt-parity
Feature: A read-only list property
  The entries of a `readonly` list property cannot be changed: the list a script reads from it
  is read-only. A script can still give the property another list, which is converted to the
  list's type as any list is.

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

  @backlog
  Scenario Outline: A script gives a read-only <list> the list <given>
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property <list> values: [1, 2, 3]
        property string seen: ""
        function give() {
          try { values = <given> } catch (e) { seen = e.name }
        }
      }
      """
    When the method "give" of "root" is called
    Then the property "values" of "root" is <now>
    And the property "seen" of "root" is ""

    Examples:
      | list       | given    | now    |
      | list<int>  | []       | []     |
      | list<real> | ["", ""] | [0, 0] |

  # Found by ljos-fuzz (bindings), bucket value:list<real>:step-set(literal).
  @backlog
  Scenario: A script gives a read-only list a list through an alias of it
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        readonly property list<real> values: []
        property alias all: root.values
        function give() { all = [1, 2, 3] }
      }
      """
    When the method "give" of "root" is called
    Then the property "values" of "root" is [1, 2, 3]
