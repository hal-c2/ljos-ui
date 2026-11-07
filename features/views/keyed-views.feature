# Sources:
#   docs/DIALECT.md (§4.8 models and delegates, §4.11 declarative creation, §4.15 keyed views)
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (add, remove)
# A view's `key`, `enter` and `exit` are Ljós's own: Qt's views have none, and make every
# delegate again for a list that is not equal to the one they show
# (features/views/equal-model-keeps-delegates.feature). The ListView pair is
# features/views/list-transitions.feature, whose numbers the last rule holds.

@ljos-only
Feature: Keyed views
  A `Repeater` or a `ListView` given a `key` matches the rows of a new
  list to the rows it shows by that field. A row that stays keeps its
  delegate, with its state, and reads its new fields; a row of a new key
  gets a new delegate, and a row whose key is gone loses its own. The
  view's `enter` transition runs on a delegate of a new row and its
  `exit` on one whose row is gone, which stays until the transition ends.

  Rule: A row that stays keeps its delegate

    Background:
      Given the QML:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property unknown rows: [{ id: "a", n: 1 }, { id: "b", n: 2 }, { id: "c", n: 3 }]
          property int made: 0; property string seen
          Repeater {
            id: repeater; model: root.rows; key: "id"
            Item {
              objectName: "r-" + modelData.id; property int n: modelData.n
              property int mark: 0; Component.onCompleted: root.made += 1
            }
          }
          ListView {
            objectName: "list"; width: 200; height: 100; model: root.rows; key: "id"
            delegate: Item {
              objectName: "l-" + modelData.id; width: 200; height: 20
              property int mark: 0; Component.onCompleted: root.made += 1
            }
          }
          function look() {
            const s: string[] = []; for (let i = 0; i < repeater.count; i++) s.push(repeater.itemAt(i).objectName + repeater.itemAt(i).n)
            seen = s.join(" ")
          }
        }
        """

    Scenario: A new list keeps the delegates of the keys it names again, in its order
      When the property "mark" of "r-b" is set to 7
      And the property "mark" of "l-b" is set to 7
      And the property "rows" of "root" is set to [{"id": "b", "n": 20}, {"id": "d", "n": 4}, {"id": "a", "n": 1}]
      And the method "look" of "root" is called
      Then the property "seen" of "root" is "r-b20 r-d4 r-a1"
      And the property "made" of "root" is 8
      And the property "mark" of "r-b" is 7
      And the property "mark" of "l-b" is 7
      And the property "count" of "list" is 3
      And the property "y" of "l-b" is 0
      And the property "y" of "l-d" is 20
      And the property "y" of "l-a" is 40
      And the object "r-c" does not exist
      And the object "l-c" does not exist

    # A list equal to the one shown keeps its delegates without a key too
    # (features/views/equal-model-keeps-delegates.feature); one that differs in a field does not.
    Scenario: The same keys with a new field keep every delegate, which reads the field
      When the property "rows" of "root" is set to [{"id": "a", "n": 1}, {"id": "b", "n": 2}, {"id": "c", "n": 30}]
      Then the property "made" of "root" is 6
      And the property "n" of "r-c" is 30

    Scenario: Rows that share a key are matched in order, and each view says so once
      When the property "rows" of "root" is set to [{"id": "a", "n": 1}, {"id": "a", "n": 5}]
      And the method "look" of "root" is called
      Then the property "seen" of "root" is "r-a1 r-a5"
      And the property "made" of "root" is 8
      And the log has 2 warnings matching "rows share the key id a; they keep delegates in order: give each row a key of its own"

  Rule: Without a key a new list makes every delegate again

    Scenario: A Repeater without a key makes its items again
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property unknown rows: [{ id: "a" }, { id: "b" }]
          property int made: 0
          Repeater {
            model: root.rows
            Item { Component.onCompleted: root.made += 1 }
          }
        }
        """
      When the property "rows" of "root" is set to [{"id": "b"}, {"id": "a"}]
      Then the property "made" of "root" is 4

  # Blocked on U3: `Transition` is not in Ljos.Ui yet, so a Ljós file cannot say one.
  @blocked
  Rule: A keyed Repeater plays enter on a new row's item and exit on a gone row's

    Background:
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          width: 200; height: 200
          property unknown rows: [{ id: "a" }, { id: "b" }]
          Repeater {
            objectName: "repeater"
            model: root.rows; key: "id"
            enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 200 } }
            exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: 200 } }
            Item { objectName: modelData.id; width: 20; height: 20 }
          }
        }
        """

    Scenario: The items made first do not enter
      When 100 milliseconds pass
      Then the property "opacity" of "a" is 1

    Scenario: The item of a new row enters
      When 400 milliseconds pass
      And the property "rows" of "root" is set to [{"id": "a"}, {"id": "b"}, {"id": "c"}]
      And 120 milliseconds pass
      Then the property "opacity" of "c" is at least 0.2
      And the property "opacity" of "c" is at most 0.9
      And the property "opacity" of "a" is 1
      When 300 milliseconds pass
      Then the property "opacity" of "c" is 1

    Scenario: The item of a gone row stays until its exit has ended
      When 400 milliseconds pass
      And the property "rows" of "root" is set to [{"id": "b"}]
      And 120 milliseconds pass
      Then the object "a" exists
      And the property "opacity" of "a" is at most 0.8
      And the property "count" of "repeater" is 1
      When 300 milliseconds pass
      Then the object "a" does not exist
      And the property "opacity" of "b" is 1

  # Blocked on U3: `Transition` is not in Ljos.Ui yet, so a Ljós file cannot say one.
  @blocked
  Rule: A ListView's add and remove are its enter and exit

    Background:
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Models
        ListView {
          objectName: "list"
          width: 200; height: 300
          model: lm
          ListModel {
            id: lm
            objectName: "lm"
            ListElement { name: "a" }
            ListElement { name: "b" }
          }
          populate: Transition { NumberAnimation { property: "scale"; from: 0.5; to: 1; duration: 200 } }
          enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 200 } }
          exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: 200 } }
          delegate: Item {
            required property string name
            objectName: name; width: 200; height: 20
          }
        }
        """

    Scenario: In Ljós's API, an added row runs the enter transition
      When 400 milliseconds pass
      And the method "append" of "lm" is called with {"name": "c"}
      And 120 milliseconds pass
      Then the property "opacity" of "c" is at least 0.2
      And the property "opacity" of "c" is at most 0.9
      And the property "scale" of "c" is 1
      When 300 milliseconds pass
      Then the property "opacity" of "c" is 1

    Scenario: In Ljós's API, a removed row stays until its exit transition has ended
      When 400 milliseconds pass
      And the method "remove" of "lm" is called with 0
      And 120 milliseconds pass
      Then the object "a" exists
      And the property "opacity" of "a" is at most 0.8
      And the property "count" of "list" is 1
      When 300 milliseconds pass
      Then the object "a" does not exist
      And the property "y" of "b" is 0

  Rule: Qt's add and remove of a ListView are lowered to enter and exit

    # Blocked on U3: a file that makes a `Transition` is kept until Ljos.Ui has one.
    @blocked @headless
    Scenario: A ListView's add and remove are written enter and exit
      Given the file "Main.qml":
        """qml
        import QtQuick
        ListView {
          width: 200; height: 300
          model: ListModel { id: lm; ListElement { name: "a" } }
          add: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1 } }
          remove: Transition { NumberAnimation { property: "opacity"; to: 0 } }
          onAddChanged: console.log("add")
          delegate: Item { width: 200; height: 20; Component.onCompleted: if (index === 0) lm.remove(0) }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (4 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Models
        ListView {
          width: 200; height: 300
          model: ListModel { id: lm; ListElement { name: "a" } }
          enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1 } }
          exit: Transition { NumberAnimation { property: "opacity"; to: 0 } }
          onEnterChanged: console.log("add")
          delegate: Item { width: 200; height: 20; Component.onCompleted: if (index === 0) lm.remove(0) }
        }
        """

    @headless
    Scenario: A handler of add is written of enter, and a ListModel's remove() stays
      Given the file "Main.qml":
        """qml
        import QtQuick
        ListView {
          width: 200; height: 300
          model: ListModel { id: lm; ListElement { name: "a" } }
          onAddChanged: console.log("add")
          onRemoveChanged: console.log("remove")
          delegate: Item { width: 200; height: 20; Component.onCompleted: if (index === 0) lm.remove(0) }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (3 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Models
        ListView {
          width: 200; height: 300
          model: ListModel { id: lm; ListElement { name: "a" } }
          onEnterChanged: console.log("add")
          onExitChanged: console.log("remove")
          delegate: Item { width: 200; height: 20; Component.onCompleted: if (index === 0) lm.remove(0) }
        }
        """

    @ljos-only @headless
    Scenario: An Item's own remove() is no ListView's, and the file is lowered
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          ListModel { id: lm; ListElement { name: "a" } }
          function remove(i) { lm.remove(i) }
          Component.onCompleted: remove(0)
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (1 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Models
        Item {
          ListModel { id: lm; ListElement { name: "a" } }
          function remove(i) { lm.remove(i) }
          Component.onCompleted: remove(0)
        }
        """
