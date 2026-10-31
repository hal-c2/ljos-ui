# Sources:
#   docs/ARCHITECTURE.md (section 17, the models)
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html (the signals and get it shares)
#   https://doc.qt.io/qt-6/qabstractitemmodel.html#signals
# JsonListModel is a Ljós type: Qt has nothing to compare it with.

@ljos-only
Feature: JsonListModel
  A JsonListModel is a list model whose rows come from JSON: an array of
  objects, as text in `json` or in the file or URL `source` names. A script
  edits the rows with the methods of a ListModel; the edits stay in the model
  and are not written to the file.
  Its roles are the keys of the objects, in the order they are first seen.
  When the data changes the model says only what differs, so the delegates
  of the rows that stayed the same are kept. JSON it cannot use leaves the
  rows as they were and says why.

  Background:
    Given the file "Recorder.qml":
      """qml
      import QtQuick
      Item {
        id: rec
        property var model
        property string seen: ""
        function say(s) { seen = (seen ? seen + " " : "") + s }
        Connections {
          target: rec.model
          function onCountChanged() { rec.say("count" + rec.model.count) }
          function onDataChanged(tl, br, roles) { rec.say("data" + tl.row + "-" + br.row + JSON.stringify(roles)) }
          function onRowsInserted(p, f, l) { rec.say("+" + f + "-" + l) }
          function onRowsRemoved(p, f, l) { rec.say("x" + f + "-" + l) }
          function onModelReset() { rec.say("reset") }
          function onRowsMoved(sp, f, l, dp, row) { rec.say("m" + f + "-" + l + ">" + row) }
          function onRolesChanged() { rec.say("roles") }
        }
      }
      """

  @api-Ljos.Models.JsonListModel.json
  @api-Ljos.Models.JsonListModel.count
  @api-Ljos.Models.JsonListModel.roles
  Scenario: The objects of an array become rows, their keys the roles in first-seen order
    Given the QML:
      """qml
      import QtQuick
      import Ljos.Models
      Item {
        objectName: "o"
        property string names: m.roles.join(",")
        JsonListModel {
          id: m; objectName: "m"
          json: '[{"name": "a", "n": 1}, {"tag": "x", "name": "b"}]'
        }
      }
      """
    Then the property "count" of "m" is 2
    And the property "names" of "o" is "name,n,tag"
    And the log has no warnings

  @api-Ljos.Models.JsonListModel.status
  Scenario Outline: JsonListModel.<name> is <value>
    Given the QML:
      """qml
      import QtQuick
      import Ljos.Models
      Item { objectName: "o"; property var v: JsonListModel.<name> }
      """
    Then the property "v" of "o" is <value>

    Examples:
      | name    | value |
      | Null    | 0     |
      | Ready   | 1     |
      | Loading | 2     |
      | Error   | 3     |

  Scenario: A model given nothing is empty and has loaded nothing
    Given the QML:
      """qml
      import Ljos.Models
      JsonListModel { objectName: "m" }
      """
    Then the property "status" of "m" is 0
    And the property "count" of "m" is 0
    And the property "error" of "m" is ""

  @api-Ljos.Models.JsonListModel.get
  Scenario: get gives a row as a plain object, undefined beyond the rows
    Given the QML:
      """qml
      import QtQuick
      import Ljos.Models
      Item {
        objectName: "o"
        JsonListModel {
          id: m
          json: '[{"name": "a", "tags": ["x", "y"], "at": {"line": 3}}, {"name": "b", "gone": null}]'
        }
        property string first: m.count ? JSON.stringify(m.get(0)) : ""
        property string second: m.count ? JSON.stringify(m.get(1)) : ""
        property string beyond: m.count ? typeof m.get(2) : ""
        property string before: m.count ? typeof m.get(-1) : ""
      }
      """
    Then the property "first" of "o" is "{\"name\":\"a\",\"tags\":[\"x\",\"y\"],\"at\":{\"line\":3}}"
    And the property "second" of "o" is "{\"name\":\"b\",\"gone\":null}"
    And the property "beyond" of "o" is "undefined"
    And the property "before" of "o" is "undefined"

  Scenario: A key a row does not have is undefined, a JSON null is null
    Given the QML:
      """qml
      import QtQuick
      import Ljos.Models
      Item {
        id: root; objectName: "o"
        property string seen: ""
        JsonListModel {
          id: m
          json: '[{"a": 1, "b": null}, {"a": 2}]'
        }
        Repeater {
          model: m
          delegate: Item {
            required property var b
            Component.onCompleted: root.seen += (b === undefined ? "undefined" : b === null ? "null" : "other") + ";"
          }
        }
      }
      """
    Then the property "seen" of "o" is "null;undefined;"

  Scenario: Numbers, truth, text, arrays and objects reach a delegate as what they are
    Given the QML:
      """qml
      import QtQuick
      import Ljos.Models
      Item {
        id: root; objectName: "o"
        property string seen: ""
        JsonListModel {
          id: m
          json: '[{"i": 3, "r": 0.5, "t": true, "s": "é\\n", "l": [1, 2], "o": {"k": "v"}}]'
        }
        Repeater {
          model: m
          delegate: Item {
            required property var i; required property var r; required property var t
            required property var s; required property var l; required property var o
            Component.onCompleted: root.seen = [i + 1, r * 2, t === true, s.length, l[1], o.k].join(",")
          }
        }
      }
      """
    Then the property "seen" of "o" is "4,1,true,2,2,v"

  Scenario: A key said twice in an object is the last one said, as JSON.parse takes it
    Given the QML:
      """qml
      import QtQuick
      import Ljos.Models
      Item {
        objectName: "o"
        JsonListModel { id: m; json: '[{"a": 1, "a": 2}]' }
        property var a: m.count ? m.get(0).a : 0
      }
      """
    Then the property "a" of "o" is 2

  Scenario Outline: A <view> shows one delegate for each row
    Given the QML:
      """qml
      import QtQuick
      import Ljos.Models
      Item {
        id: root; objectName: "o"; width: 200; height: 200
        property string seen: ""
        JsonListModel { id: m; json: '[{"name": "a"}, {"name": "b"}]' }
        <view> {
          <size>
          model: m
          delegate: Item {
            width: 20; height: 20
            required property string name
            required property int index
            Component.onCompleted: root.seen += index + name + ";"
          }
        }
      }
      """
    Then the property "seen" of "o" is "0a;1b;"

    Examples:
      | view     | size                     |
      | Repeater |                          |
      | ListView | width: 100; height: 100  |
      | GridView | width: 100; height: 100  |

  Scenario: A model with one role gives its delegates modelData
    Given the QML:
      """qml
      import QtQuick
      import Ljos.Models
      Item {
        id: root; objectName: "o"
        property string seen: ""
        JsonListModel { id: m; json: '[{"name": "a"}]' }
        Repeater {
          model: m
          delegate: Item {
            required property var modelData
            Component.onCompleted: root.seen = modelData
          }
        }
      }
      """
    Then the property "seen" of "o" is "a"

  Scenario: A role named modelData is the delegates' modelData
    Given the QML:
      """qml
      import QtQuick
      import Ljos.Models
      Item {
        id: root; objectName: "o"
        property string seen: ""
        JsonListModel { id: m; json: '[{"name": "a", "modelData": "b"}]' }
        Repeater {
          model: m
          delegate: Item { Component.onCompleted: root.seen = name + modelData }
        }
      }
      """
    Then the property "seen" of "o" is "ab"

  Scenario: A TableView shows the rows in one column
    Given the QML:
      """qml
      import QtQuick
      import Ljos.Models
      Item {
        id: root; objectName: "o"; width: 200; height: 200
        property string seen: ""
        JsonListModel { id: m; json: '[{"name": "a"}, {"name": "b"}]' }
        TableView {
          objectName: "t"
          width: 100; height: 100
          model: m
          delegate: Item {
            implicitWidth: 20; implicitHeight: 20
            required property string name
            required property int row
            Component.onCompleted: root.seen += row + name + ";"
          }
        }
      }
      """
    When 100 milliseconds pass
    Then the property "seen" of "o" is "0a;1b;"
    And the property "rows" of "t" is 2
    And the property "columns" of "t" is 1

  Rule: A change says only what differs

    Scenario Outline: <what> is said as <seen>
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          JsonListModel {
            id: m; objectName: "m"
            json: '[{"n": 1, "s": "a"}, {"n": 2, "s": "b"}, {"n": 3, "s": "c"}]'
          }
          Recorder { id: rec; objectName: "rec"; model: m }
          function next() { rec.seen = ""; m.json = '<new>' }
          objectName: "o"
        }
        """
      When the method "next" of "o" is called
      Then the property "seen" of "rec" is "<seen>"

      Examples:
        | what                  | new                                                                                     | seen                |
        | The same data         | [{"n": 1, "s": "a"}, {"n": 2, "s": "b"}, {"n": 3, "s": "c"}]                            |                     |
        | The same data respaced| [ {"s":"a","n":1}, {"n":2,"s":"b"}, {"n":3,"s":"c"} ]                                   |                     |
        | One value of a row    | [{"n": 1, "s": "a"}, {"n": 2, "s": "B"}, {"n": 3, "s": "c"}]                            | data1-1[1]      |
        | A row added at the end| [{"n": 1, "s": "a"}, {"n": 2, "s": "b"}, {"n": 3, "s": "c"}, {"n": 4, "s": "d"}]        | +3-3 count4         |
        | A row added between   | [{"n": 1, "s": "a"}, {"n": 9, "s": "z"}, {"n": 2, "s": "b"}, {"n": 3, "s": "c"}]        | +1-1 count4         |
        | A row taken from the end | [{"n": 1, "s": "a"}, {"n": 2, "s": "b"}]                                             | x2-2 count2         |
        | A row taken from between | [{"n": 1, "s": "a"}, {"n": 3, "s": "c"}]                                             | x1-1 count2         |
        | The first and last row | [{"n": 9, "s": "a"}, {"n": 2, "s": "b"}, {"n": 3, "s": "z"}]                           | data0-0[0] data2-2[1] |
        | Two neighbours alike  | [{"n": 8, "s": "a"}, {"n": 9, "s": "b"}, {"n": 3, "s": "c"}]                            | data0-1[0]      |
        | Fewer and different   | [{"n": 9, "s": "z"}]                                                                    | data0-0[0,1] x1-2 count1 |
        | No rows at all        | []                                                                                      | x0-2 count0         |

    Scenario: The delegates of the rows that stayed are kept
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          id: root; objectName: "o"
          property int made: 0
          JsonListModel { id: m; json: '[{"s": "a"}, {"s": "b"}, {"s": "c"}]' }
          function next() { m.json = '[{"s": "a"}, {"s": "z"}, {"s": "c"}, {"s": "d"}]' }
          property string shown: ""
          Repeater {
            id: rep
            model: m
            delegate: Text {
              required property string s
              text: s
              Component.onCompleted: root.made += 1
            }
          }
          function read() { shown = rep.itemAt(1).text }
        }
        """
      Then the property "made" of "o" is 3
      When the method "next" of "o" is called
      And the method "read" of "o" is called
      Then the property "made" of "o" is 4
      And the property "shown" of "o" is "z"

    Scenario: A new key adds a role and says so, and the rows that have it change
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          objectName: "o"
          property string names: m.roles.join(",")
          JsonListModel { id: m; objectName: "m"; json: '[{"a": 1}, {"a": 2}]' }
          Recorder { id: rec; objectName: "rec"; model: m }
          function next() { rec.seen = ""; m.json = '[{"a": 1}, {"a": 2, "b": 3}]' }
        }
        """
      When the method "next" of "o" is called
      Then the property "seen" of "rec" is "roles data1-1[1]"
      And the property "names" of "o" is "a,b"

    Scenario: A key no row has any more is a reset, and the roles are read afresh
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          objectName: "o"
          property string names: m.roles.join(",")
          JsonListModel { id: m; objectName: "m"; json: '[{"a": 1, "b": 2}]' }
          Recorder { id: rec; objectName: "rec"; model: m }
          function next() { rec.seen = ""; m.json = '[{"b": 2}, {"c": 3}]' }
        }
        """
      When the method "next" of "o" is called
      Then the property "seen" of "rec" is "roles reset count2"
      And the property "names" of "o" is "b,c"

    Scenario: The first rows are said as rows that came, after the roles
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          JsonListModel { id: m; json: '[{"a": 1}]' }
          Recorder { objectName: "rec"; model: m }
        }
        """
      Then the property "seen" of "rec" is "roles +0-0 count1"

  Rule: JSON that cannot be used leaves the rows as they were

    @api-Ljos.Models.JsonListModel.error
    Scenario Outline: <what> keeps the rows and says why
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          objectName: "o"
          JsonListModel { id: m; objectName: "m"; json: '[{"a": 1}, {"a": 2}]' }
          Recorder { id: rec; objectName: "rec"; model: m }
          function next() { rec.seen = ""; m.json = '<bad>' }
          property var kept: m.count ? m.get(1).a : 0
          property bool says: m.error.indexOf("<says>") >= 0
        }
        """
      When the method "next" of "o" is called
      Then the property "status" of "m" is 3
      And the property "count" of "m" is 2
      And the property "kept" of "o" is 2
      And the property "says" of "o" is true
      And the property "seen" of "rec" is ""
      And the log has a warning matching "QML JsonListModel: .*<says>"

      Examples:
        | what                  | bad            | says                |
        | Text that is not JSON | [{"a": 1       | line 1              |
        | A top level no array  | {"a": 1}       | an array of objects |
        | A row no object       | [{"a": 1}, 2]  | row 1               |

    Scenario: Good JSON after bad takes the error away
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          objectName: "o"
          JsonListModel { id: m; objectName: "m"; json: '[{"a": 1}]' }
          function bad() { m.json = 'nope' }
          function good() { m.json = '[{"a": 1}, {"a": 2}]' }
        }
        """
      When the method "bad" of "o" is called
      And the method "good" of "o" is called
      Then the property "status" of "m" is 1
      And the property "error" of "m" is ""
      And the property "count" of "m" is 2

    Scenario: An empty text is no rows and no error
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          objectName: "o"
          JsonListModel { id: m; objectName: "m"; json: '[{"a": 1}]' }
          function clear() { m.json = "" }
        }
        """
      When the method "clear" of "o" is called
      Then the property "status" of "m" is 0
      And the property "count" of "m" is 0

  Rule: A source is read in the background

    @api-Ljos.Models.JsonListModel.source
    Scenario: A file beside the QML is read, Loading then Ready
      Given the text file "rows.json":
        """
        [{"name": "a"}, {"name": "b"}]
        """
      And the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          id: root; objectName: "o"
          property string seen: ""
          JsonListModel {
            id: m; objectName: "m"; source: "rows.json"
            onCountChanged: root.seen += "count " + count + ";"
            onStatusChanged: root.seen += "status " + status + ";"
          }
          Repeater {
            model: m
            delegate: Item {
              required property string name
              Component.onCompleted: root.seen += name + ";"
            }
          }
        }
        """
      Then the property "status" of "m" is 2
      When 500 milliseconds pass
      Then the property "status" of "m" is 1
      And the property "seen" of "o" is "status 2;a;b;count 2;status 1;"

    Scenario: A source wins over the text
      Given the text file "rows.json":
        """
        [{"name": "file"}]
        """
      And the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          objectName: "o"
          JsonListModel { id: m; objectName: "m"; source: "rows.json"; json: '[{"name": "text"}]' }
          property string first: m.count ? m.get(0).name : ""
        }
        """
      When 500 milliseconds pass
      Then the property "first" of "o" is "file"

    Scenario: A relative source in a component file is read beside that file
      Given the text file "members.json":
        """
        [{"name": "beside the window"}]
        """
      And the text file "team/members.json":
        """
        [{"name": "beside the component"}]
        """
      And the file "team/Members.qml":
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          property alias model: m
          JsonListModel { id: m; source: "members.json" }
        }
        """
      And the QML:
        """qml
        import QtQuick
        import "team"
        Item {
          objectName: "o"
          Members { id: t }
          property string first: t.model.count ? t.model.get(0).name : ""
        }
        """
      When 500 milliseconds pass
      Then the property "first" of "o" is "beside the component"

    Scenario: A relative source on the root of a component file is read beside the file that uses it
      Given the text file "members.json":
        """
        [{"name": "beside the window"}]
        """
      And the text file "team/members.json":
        """
        [{"name": "beside the component"}]
        """
      And the file "team/Members.qml":
        """qml
        import QtQuick
        import Ljos.Models
        JsonListModel {
          property var model: this
          source: "members.json"
        }
        """
      And the QML:
        """qml
        import QtQuick
        import "team"
        Item {
          objectName: "o"
          Members { id: t }
          property string first: t.model.count ? t.model.get(0).name : ""
        }
        """
      When 500 milliseconds pass
      Then the property "first" of "o" is "beside the window"

    Scenario: Qt.resolvedUrl on the root of a component file reads beside that file
      Given the text file "members.json":
        """
        [{"name": "beside the window"}]
        """
      And the text file "team/members.json":
        """
        [{"name": "beside the component"}]
        """
      And the file "team/Members.qml":
        """qml
        import QtQuick
        import Ljos.Models
        JsonListModel {
          property var model: this
          source: Qt.resolvedUrl("members.json")
        }
        """
      And the QML:
        """qml
        import QtQuick
        import "team"
        Item {
          objectName: "o"
          Members { id: t }
          property string first: t.model.count ? t.model.get(0).name : ""
        }
        """
      When 500 milliseconds pass
      Then the property "first" of "o" is "beside the component"

    Scenario: A file served over the network is read
      Given the web server serves the text file "rows.json":
        """
        [{"name": "a"}]
        """
      And the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          JsonListModel { objectName: "m"; source: "$SERVER/rows.json" }
        }
        """
      When 500 milliseconds pass
      Then the property "status" of "m" is 1
      And the property "count" of "m" is 1

    Scenario: A file that is not there is an error that names it
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          objectName: "o"
          JsonListModel { id: m; objectName: "m"; source: "missing.json" }
          property bool says: m.error.indexOf("missing.json") >= 0
        }
        """
      When 500 milliseconds pass
      Then the property "status" of "m" is 3
      And the property "count" of "m" is 0
      And the property "says" of "o" is true
      And the log has a warning matching "QML JsonListModel: .*missing.json"

    Scenario: A file of broken JSON says the file, the line and the column
      Given the text file "rows.json":
        """
        [{"name": "a"},
         {"name": }]
        """
      And the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          objectName: "o"
          JsonListModel { id: m; objectName: "m"; source: "rows.json" }
          property bool says: /rows\.json:\d+:\d+: /.test(m.error)
        }
        """
      When 500 milliseconds pass
      Then the property "status" of "m" is 3
      And the property "says" of "o" is true

    @api-Ljos.Models.JsonListModel.reload
    Scenario: reload reads the file again, and says nothing of rows that did not change
      Given the text file "rows.json":
        """
        [{"name": "a"}]
        """
      And the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          id: root; objectName: "o"
          property string seen: ""
          JsonListModel {
            id: m; objectName: "m"; source: "rows.json"
            onStatusChanged: root.seen += status + ";"
          }
          Recorder { objectName: "rec"; model: m }
        }
        """
      When 500 milliseconds pass
      And the method "reload" of "m" is called
      And 500 milliseconds pass
      Then the property "seen" of "o" is "2;1;2;1;"
      And the property "seen" of "rec" is "roles +0-0 count1"

    Scenario: A new source replaces the rows of the old one by what differs
      Given the text file "one.json":
        """
        [{"name": "a"}, {"name": "b"}]
        """
      And the text file "two.json":
        """
        [{"name": "a"}, {"name": "c"}, {"name": "d"}]
        """
      And the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          JsonListModel { id: m; objectName: "m"; source: "one.json" }
          Recorder { objectName: "rec"; model: m }
        }
        """
      When 500 milliseconds pass
      And the property "source" of "m" is set to "two.json"
      And 500 milliseconds pass
      Then the property "seen" of "rec" is "roles +0-1 count2 data1-1[0] +2-2 count3"

  Rule: A file watched with the QML is read again when saved

    Background:
      Given the file "app/rows.json":
        """
        [{"name": "a"}, {"name": "b"}]
        """
      And the file "app/Main.qml":
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          id: root; objectName: "main"
          property int made: 0
          property int taps: 0
          JsonListModel { id: m; objectName: "m"; source: "rows.json" }
          Repeater {
            model: m
            delegate: Item {
              required property string name
              Component.onCompleted: root.made += 1
            }
          }
          property string first: m.count ? m.get(0).name : ""
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      When the application watches the roots "app" for the entries "Main"
      And 500 milliseconds pass

    Scenario: A saved file changes only its rows, and nothing else is made again
      Then the property "made" of "main" is 2
      When the property "taps" of "main" is set to 3
      And the file "app/rows.json" is saved on disk as:
        """
        [{"name": "z"}, {"name": "b"}, {"name": "c"}]
        """
      And 500 milliseconds pass
      Then the property "first" of "main" is "z"
      And the property "count" of "m" is 3
      And the property "made" of "main" is 3
      And the property "taps" of "main" is 3

    Scenario: A file saved broken keeps the rows until it is saved whole
      When the file "app/rows.json" is saved on disk as:
        """
        [{"name":
        """
      And 500 milliseconds pass
      Then the property "status" of "m" is 3
      And the property "count" of "m" is 2
      And the log has a warning matching "QML JsonListModel: .*rows.json:\d+:\d+: "
      When the file "app/rows.json" is saved on disk as:
        """
        [{"name": "a"}]
        """
      And 500 milliseconds pass
      Then the property "status" of "m" is 1
      And the property "error" of "m" is ""
      And the property "count" of "m" is 1

    Scenario: A file removed keeps the rows, and is read again when it comes back
      When the file "app/rows.json" is removed from disk
      And 500 milliseconds pass
      Then the property "status" of "m" is 3
      And the property "count" of "m" is 2
      When the file "app/rows.json" is saved on disk as:
        """
        [{"name": "a"}, {"name": "b"}, {"name": "c"}]
        """
      And 500 milliseconds pass
      Then the property "status" of "m" is 1
      And the property "count" of "m" is 3

  Rule: A script edits the rows as it does those of a ListModel

    Scenario Outline: <call> is said as <seen>
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          objectName: "o"
          property string rows: ""
          JsonListModel {
            id: m; objectName: "m"
            json: '[{"n": 1, "s": "a"}, {"n": 2, "s": "b"}, {"n": 3, "s": "c"}]'
          }
          Recorder { id: rec; objectName: "rec"; model: m }
          function go() {
            rec.seen = ""
            <call>
            var out = []
            for (var i = 0; i < m.count; ++i) out.push(m.get(i).n)
            rows = out.join(",")
          }
        }
        """
      When the method "go" of "o" is called
      Then the property "seen" of "rec" is "<seen>"
      And the property "rows" of "o" is "<rows>"
      And the log has no warnings

      Examples:
        | call                                           | seen                | rows    |
        | m.append({n: 4, s: "d"})                       | +3-3 count4         | 1,2,3,4 |
        | m.append([{n: 4}, {n: 5}])                     | +3-4 count5         | 1,2,3,4,5 |
        | m.insert(1, {n: 9, s: "z"})                    | +1-1 count4         | 1,9,2,3 |
        | m.insert(3, {n: 9})                            | +3-3 count4         | 1,2,3,9 |
        | m.remove(1)                                    | x1-1 count2         | 1,3     |
        | m.remove(0, 2)                                 | x0-1 count1         | 3       |
        | m.move(0, 2, 1)                                | m0-0>3              | 2,3,1   |
        | m.move(2, 0, 1)                                | m2-2>0              | 3,1,2   |
        | m.clear()                                      | x0-2 count0         |         |
        | m.set(1, {n: 7})                               | data1-1[0]          | 1,7,3   |
        | m.set(3, {n: 4, s: "d"})                       | +3-3 count4         | 1,2,3,4 |
        | m.setProperty(2, "n", 8)                       | data2-2[0]          | 1,2,8   |
        | m.setProperty(2, "s", "c")                     | data2-2[1]          | 1,2,3   |

    Scenario: A key no row has is a new role, said before the row that has it
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          objectName: "o"
          property string names: m.roles.join(",")
          JsonListModel { id: m; objectName: "m"; json: '[{"a": 1}, {"a": 2}]' }
          Recorder { id: rec; objectName: "rec"; model: m }
          function go() { rec.seen = ""; m.setProperty(1, "b", "x"); m.append({a: 3, c: true}) }
        }
        """
      When the method "go" of "o" is called
      Then the property "names" of "o" is "a,b,c"
      And the property "seen" of "rec" is "roles data1-1[1] roles +2-2 count3"

    Scenario Outline: <call> is refused with a warning and changes nothing
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          objectName: "o"
          JsonListModel { id: m; objectName: "m"; json: '[{"n": 1}, {"n": 2}]' }
          Recorder { id: rec; objectName: "rec"; model: m }
          function go() { rec.seen = ""; <call> }
        }
        """
      When the method "go" of "o" is called
      Then the property "count" of "m" is 2
      And the property "seen" of "rec" is ""
      And the log has a warning matching "QML JsonListModel: <says>"

      Examples:
        | call                      | says                    |
        | m.append(5)               | append: value is not an object |
        | m.insert(3, {n: 1})       | insert: index 3 out of range |
        | m.insert(0, "x")          | insert: value is not an object |
        | m.remove(2)               | remove: indices .2 - 3. out of range .0 - 2. |
        | m.remove(0, 3)            | remove: indices .0 - 3. out of range .0 - 2. |
        | m.move(0, 2, 1)           | move: out of range      |
        | m.set(3, {n: 1})          | set: index 3 out of range |
        | m.set(0, 4)               | set: value is not an object |
        | m.setProperty(2, "n", 1)  | set: index 2 out of range |

    Scenario: A view follows the edits
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          id: root; objectName: "o"; width: 200; height: 200
          property string shown: ""
          JsonListModel { id: m; json: '[{"name": "a"}, {"name": "b"}]' }
          Repeater { id: rep; model: m; delegate: Text { required property string name; text: name } }
          function go() {
            m.append({name: "c"}); m.setProperty(0, "name", "z"); m.move(2, 0, 1); m.remove(1)
            var out = []
            for (var i = 0; i < rep.count; ++i) out.push(rep.itemAt(i).text)
            shown = out.join(",")
          }
        }
        """
      When the method "go" of "o" is called
      Then the property "shown" of "o" is "c,b"

    Scenario: A delegate writes a role of its row
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          objectName: "o"
          JsonListModel { id: m; objectName: "m"; json: '[{"title": "a"}, {"title": "b"}]' }
          Recorder { id: rec; objectName: "rec"; model: m }
          Repeater {
            id: rep; model: m
            delegate: Item { function rename() { model.title = model.title + "!" } }
          }
          property string second: ""
          function go() { rec.seen = ""; rep.itemAt(1).rename(); second = m.get(1).title }
        }
        """
      When the method "go" of "o" is called
      Then the property "second" of "o" is "b!"
      And the property "seen" of "rec" is "data1-1[0]"

    Scenario: Reading the source again replaces the edits
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Models
        Item {
          objectName: "o"
          JsonListModel { id: m; objectName: "m"; json: '[{"n": 1}, {"n": 2}]' }
          function edit() { m.setProperty(0, "n", 9); m.append({n: 3}) }
          function again() { m.reload() }
          property string rows: { var out = []; for (var i = 0; i < m.count; ++i) out.push(m.get(i).n); return out.join(",") }
        }
        """
      When the method "edit" of "o" is called
      Then the property "rows" of "o" is "9,2,3"
      When the method "again" of "o" is called
      Then the property "rows" of "o" is "1,2"
      And the property "count" of "m" is 2
