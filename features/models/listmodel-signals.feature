# Sources:
#   https://doc.qt.io/qt-6/qabstractitemmodel.html#signals
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A ListModel says what changed in it
  A ListModel is an item model: whoever listens is told of the rows that
  come, go and move and of the values that change, with the rows and
  roles concerned, as a view of it is.

  Background:
    Given the file "Recorder.qml":
      """qml
      import QtQuick
      Item {
        id: rec
        property var model
        property string seen: ""
        function ix(i) { return i.valid ? i.row + ":" + i.column : "-" }
        function say(s) { seen = (seen ? seen + " " : "") + s }
        Connections {
          target: rec.model
          function onCountChanged() { rec.say("count" + rec.model.count) }
          function onDataChanged(tl, br, roles) { rec.say("data" + rec.ix(tl) + "," + rec.ix(br) + JSON.stringify(roles)) }
          function onRowsAboutToBeInserted(p, f, l) { rec.say("+?" + rec.ix(p) + f + "-" + l + "/" + rec.model.count) }
          function onRowsInserted(p, f, l) { rec.say("+" + rec.ix(p) + f + "-" + l + "/" + rec.model.count) }
          function onRowsAboutToBeRemoved(p, f, l) { rec.say("x?" + f + "-" + l + "/" + rec.model.count) }
          function onRowsRemoved(p, f, l) { rec.say("x" + rec.ix(p) + f + "-" + l + "/" + rec.model.count) }
          function onRowsAboutToBeMoved(sp, s, e, dp, d) { rec.say("m?" + s + "-" + e + ">" + d) }
          function onRowsMoved(sp, s, e, dp, d) { rec.say("m" + rec.ix(sp) + s + "-" + e + ">" + rec.ix(dp) + d) }
          function onModelReset() { rec.say("reset") }
          function onLayoutChanged() { rec.say("layout") }
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        function rename(i, to) { lm.get(i).name = to }
        function both() { lm.set(0, {name: "A", n: 9}) }
        function more() { lm.clear(); lm.append({zz: 1, yy: 2}) }
        Recorder { objectName: "rec"; model: lm }
        ListModel {
          id: lm
          objectName: "lm"
          ListElement { name: "a"; n: 1 }
          ListElement { name: "b"; n: 2 }
          ListElement { name: "c"; n: 3 }
        }
      }
      """

  Scenario Outline: Each change is said as Qt says it
    When the method "<method>" of "lm" is called with <arguments>
    Then the property "seen" of "rec" is "<seen>"
    And the log has no warnings

    Examples:
      | method      | arguments                    | seen                    |
      | setProperty | 1, "n", 5                    | data1:0,1:0[0]          |
      | setProperty | 1, "name", "z"               | data1:0,1:0[1]          |
      | setProperty | 2, "extra", true             | data2:0,2:0[2]          |
      | append      | {"name": "d"}                | +?-3-3/3 +-3-3/4 count4 |
      | append      | [{"name": "d"}, {"n": 7}]    | +?-3-4/3 +-3-4/5 count5 |
      | insert      | 1, {"name": "z"}             | +?-1-1/3 +-1-1/4 count4 |
      | remove      | 0, 2                         | x?0-1/3 x-0-1/1 count1  |
      | move        | 0, 2, 1                      | m?0-0>3 m-0-0>-3        |
      | move        | 2, 0, 1                      | m?2-2>0 m-2-2>-0        |
      | move        | 0, 1, 2                      | m?0-1>3 m-0-1>-3        |

  Scenario: Clearing says the rows went, and with no rows says nothing
    When the method "clear" of "lm" is called
    Then the property "seen" of "rec" is "x?0-2/3 x-0-2/0 count0"
    When the property "seen" of "rec" is set to ""
    And the method "clear" of "lm" is called
    Then the property "seen" of "rec" is ""

  Scenario: A row written through get() says its role changed
    When the method "rename" of "root" is called with 2, "C"
    Then the property "seen" of "rec" is "data2:0,2:0[1]"

  Scenario: A row given several roles says them in the order given
    When the method "both" of "root" is called
    Then the property "seen" of "rec" is "data0:0,0:0[1,0]"

  Scenario: A value of another type than its role's is refused and not said
    When the method "set" of "lm" is called with 0, {"n": "text"}
    Then the property "seen" of "rec" is ""
    And the log has a warning matching "Can't assign to existing role 'n' of different type"

  Scenario: Roles a script adds come after the model's, in the order given
    When the method "more" of "root" is called
    And the property "seen" of "rec" is set to ""
    And the method "setProperty" of "lm" is called with 0, "zz", 5
    And the method "setProperty" of "lm" is called with 0, "yy", 5
    Then the property "seen" of "rec" is "data0:0,0:0[2] data0:0,0:0[3]"
