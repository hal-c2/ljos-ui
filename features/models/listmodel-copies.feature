# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html#append-method
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html#set-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQml.Models.ListModel.append
Feature: A ListModel keeps a copy of the object it is given
  `append`, `insert` and `set` take the values in the object they are given: an object or a
  date in it is copied, all the way down, so what the script does to its own afterwards does
  not reach the row. A row gives a new copy of such a value each time it is read, and a
  function in the object makes no role.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string seen
        ListModel { id: m; ListElement { k: 0 } }
        function given() {
          var o = { a: 1, in: { b: 1 } }, d = new Date(1000)
          m.append({ o: o }); m.insert(0, { o: o }); m.set(1, { o: o, d: d })
          m.append({ l: [{ o: o }] })
          o.a = 2; o.in.b = 2; d.setTime(5000)
          seen = [m.get(0).o.a, m.get(1).o.a, m.get(1).o.in.b, m.get(1).d.getTime(),
                  m.get(2).o.a, m.get(3).l.get(0).o.a].join(",")
        }
        function read() {
          m.append({ o: { a: 1 } }); var o = m.get(1).o; o.a = 7
          seen = [m.get(1).o.a, m.get(1).o === m.get(1).o, JSON.stringify(m.get(1).o)].join(",")
        }
        function method() { m.append({ f: function () { return 1 }, n: 1 }); seen = [m.get(1).f, m.get(1).n].join(",") }
      }
      """

  Scenario: What the script changes after giving an object does not reach the rows
    When the method "given" of "root" is called
    Then the property "seen" of "root" is "1,1,1,1000,1,1"

  Scenario: An object read from a row is a new copy each time
    When the method "read" of "root" is called
    Then the property "seen" of "root" is "1,false,{\"a\":1}"

  Scenario: A function in the object makes no role
    When the method "method" of "root" is called
    Then the property "seen" of "root" is ",1"
