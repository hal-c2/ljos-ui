# Sources:
#   https://doc.qt.io/qt-6/qml-list.html
#   https://doc.qt.io/qt-6/qml-var.html
#   https://doc.qt.io/qt-6/qtqml-typesystem-sequencetypes.html
#
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a list property holds is its own, entries and all
  A list property is a sequence the object owns. Given an array, whether by a binding or by an
  assignment, it takes a copy of the array and of the arrays and objects in it, so that a change
  to an entry of one is not seen by the other. A var property given a list takes such a copy as
  well. A var property given another var's array or object holds that same one.

  Rule: A list given a var's array copies it

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var arr: [{ n: 1 }, [1, 2]]
          property list<var> bound: arr
          property list<var> assigned
          property string seen
          function j(x) { return JSON.stringify(x).replace(/"/g, "") }
          function byBinding() {
            bound[0].n = 2
            bound[1].push(3)
            seen = j(bound) + " " + j(arr)
          }
          function byAssignment() {
            assigned = arr
            assigned[0].n = 6
            seen = j(assigned) + " " + j(arr)
          }
        }
        """

    Scenario Outline: <case>
      When the method "<method>" of "root" is called
      Then the property "seen" of "root" is "<seen>"

      Examples:
        | case                     | method       | seen                          |
        | Bound to the array       | byBinding    | [{n:2},[1,2,3]] [{n:1},[1,2]] |
        | Assigned the array       | byAssignment | [{n:6},[1,2]] [{n:1},[1,2]]   |

  Rule: A var or a list given a list copies it

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property list<var> rows: [{ n: 1 }, [1, 2]]
          property var varBound: rows
          property list<var> listBound: rows
          property var varAssigned
          property list<var> listAssigned
          property string seen
          function j(x) { return JSON.stringify(x).replace(/"/g, "") }
          function varByBinding() { varBound[0].n = 3; varBound[1].push(4); seen = j(varBound) + " " + j(rows) }
          function listByBinding() { listBound[0].n = 4; seen = j(listBound) + " " + j(rows) }
          function varByAssignment() { varAssigned = rows; varAssigned[0].n = 7; seen = j(varAssigned) + " " + j(rows) }
          function listByAssignment() { listAssigned = rows; listAssigned[1].push(8); seen = j(listAssigned) + " " + j(rows) }
        }
        """

    Scenario Outline: <case>
      When the method "<method>" of "root" is called
      Then the property "seen" of "root" is "<seen>"

      Examples:
        | case                     | method           | seen                          |
        | A var bound to the list  | varByBinding     | [{n:3},[1,2,4]] [{n:1},[1,2]] |
        | A list bound to the list | listByBinding    | [{n:4},[1,2]] [{n:1},[1,2]]   |
        | A var assigned the list  | varByAssignment  | [{n:7},[1,2]] [{n:1},[1,2]]   |
        | A list assigned the list | listByAssignment | [{n:1},[1,2,8]] [{n:1},[1,2]] |

  Rule: A var's object, and a script's variable given a list, are not copies

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var rec: ({ inner: { n: 1 }, list: [1] })
          property var varBound: rec
          property list<var> rows: [{ n: 1 }, [1, 2]]
          property string seen
          function j(x) { return JSON.stringify(x).replace(/"/g, "") }
          function varOfVar() { varBound.inner.n = 5; seen = j(varBound) + " " + j(rec) }
          function variable() { var held = rows; held[0].n = 9; held[1].push(9); seen = j(rows) }
        }
        """

    Scenario Outline: <case>
      When the method "<method>" of "root" is called
      Then the property "seen" of "root" is "<seen>"

      Examples:
        | case                                    | method   | seen                                          |
        | A var bound to a var holds its object   | varOfVar | {inner:{n:5},list:[1]} {inner:{n:5},list:[1]} |
        | A script's variable changes the list    | variable | [{n:9},[1,2,9]]                               |
