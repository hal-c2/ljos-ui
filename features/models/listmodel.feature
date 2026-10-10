# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html
#   https://doc.qt.io/qt-6/qml-qtqml-models-listelement.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ListModel
  A ListModel holds rows of named roles, declared as ListElements and changed from JavaScript.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string seen: "?"
        property int counted: 0
        function show() {
          var s = ""
          for (var i = 0; i < lm.count; i++) s += lm.get(i).name + lm.get(i).cost + " "
          seen = s.trim()
        }
        function read(i, role) { var e = lm.get(i); seen = e ? String(e[role]) : "none" }
        function write(i, role, v) { lm.get(i)[role] = v; show() }
        function many() { lm.append([{name: "x", cost: 7}, {name: "y", cost: 8}]); show() }
        ListModel {
          id: lm
          objectName: "lm"
          onCountChanged: root.counted++
          ListElement { name: "a"; cost: 1.5; ok: true }
          ListElement { name: "b"; cost: 2 }
          ListElement { name: "c"; cost: 3 }
        }
      }
      """

  Scenario: ListElements become rows
    When the method "show" of "root" is called
    Then the property "count" of "lm" is 3
    And the property "seen" of "root" is "a1.5 b2 c3"

  Scenario Outline: get reads a role of a row
    When the method "read" of "root" is called with <row>, "<role>"
    Then the property "seen" of "root" is "<value>"

    Examples:
      | row | role | value     |
      | 0   | name | a         |
      | 0   | ok   | true      |
      | 1   | ok   | false     |
      | 2   | cost | 3         |
      | 1   | nope | undefined |
      | 3   | name | none      |

  Scenario: append adds a row at the end
    When the method "append" of "lm" is called with {"name": "d", "cost": 4}
    And the method "show" of "root" is called
    Then the property "count" of "lm" is 4
    And the property "seen" of "root" is "a1.5 b2 c3 d4"
    And the property "counted" of "root" is 1

  Scenario: append takes an array of rows
    When the method "many" of "root" is called
    Then the property "seen" of "root" is "a1.5 b2 c3 x7 y8"

  Scenario: insert adds a row at a position
    When the method "insert" of "lm" is called with 1, {"name": "d", "cost": 4}
    And the method "show" of "root" is called
    Then the property "seen" of "root" is "a1.5 d4 b2 c3"

  Scenario: remove takes rows out
    When the method "remove" of "lm" is called with 0, 2
    And the method "show" of "root" is called
    Then the property "count" of "lm" is 1
    And the property "seen" of "root" is "c3"

  Scenario: remove without a count takes one row out
    When the method "remove" of "lm" is called with 1
    And the method "show" of "root" is called
    Then the property "seen" of "root" is "a1.5 c3"

  Scenario Outline: move changes the order of rows
    When the method "move" of "lm" is called with <from>, <to>, <n>
    And the method "show" of "root" is called
    Then the property "seen" of "root" is "<seen>"

    Examples:
      | from | to | n | seen       |
      | 0    | 2  | 1 | b2 c3 a1.5 |
      | 2    | 0  | 1 | c3 a1.5 b2 |
      | 0    | 1  | 2 | c3 a1.5 b2 |
      | 1    | 0  | 2 | b2 c3 a1.5 |

  Scenario: set replaces the roles it names
    When the method "set" of "lm" is called with 1, {"cost": 9}
    And the method "show" of "root" is called
    Then the property "seen" of "root" is "a1.5 b9 c3"

  Scenario: set at the end appends a row
    When the method "set" of "lm" is called with 3, {"name": "d", "cost": 4}
    And the method "show" of "root" is called
    Then the property "seen" of "root" is "a1.5 b2 c3 d4"

  Scenario: setProperty changes one role
    When the method "setProperty" of "lm" is called with 2, "name", "z"
    And the method "show" of "root" is called
    Then the property "seen" of "root" is "a1.5 b2 z3"

  Scenario: The object returned by get is live
    When the method "write" of "root" is called with 0, "name", "w"
    Then the property "seen" of "root" is "w1.5 b2 c3"

  Scenario: clear removes every row
    When the method "clear" of "lm" is called
    And the method "show" of "root" is called
    Then the property "count" of "lm" is 0
    And the property "seen" of "root" is ""
    And the property "counted" of "root" is 1

  Scenario: A new role can be added by a later row
    When the method "append" of "lm" is called with {"name": "d", "cost": 4, "tag": "new"}
    And the method "read" of "root" is called with 3, "tag"
    Then the property "seen" of "root" is "new"
    When the method "read" of "root" is called with 0, "tag"
    Then the property "seen" of "root" is ""

  Scenario: A role keeps the type of its first value
    When the method "set" of "lm" is called with 0, {"name": 5}
    And the method "show" of "root" is called
    Then the property "seen" of "root" is "a1.5 b2 c3"
    And the log has a warning matching "Can't assign to existing role 'name' of different type \[Number -> String\]"

  Scenario Outline: Calls outside the rows are reported and change nothing
    When the method "<method>" of "lm" is called with <args>
    And the method "show" of "root" is called
    Then the property "seen" of "root" is "a1.5 b2 c3"
    And the log has a warning matching "<warning>"

    Examples:
      | method      | args             | warning                                                  |
      | remove      | 7, 2             | ListModel: remove: indices .7 - 9. out of range .0 - 3. |
      | move        | 0, 3, 1          | ListModel: move: out of range                            |
      | insert      | 9, {"name": "d"} | ListModel: insert: index 9 out of range                  |
      | set         | 6, {"name": "d"} | ListModel: set: index 6 out of range                     |
      | setProperty | 6, "name", "d"   | ListModel: set: index 6 out of range                     |
