# Sources:
#   https://doc.qt.io/qt-6.11/qml-qtqml-models-sortfilterproxymodel.html
#   https://doc.qt.io/qt-6.11/qml-qtqml-models-rolesorter.html
#   https://doc.qt.io/qt-6.11/qml-qtqml-models-valuefilter.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# In Qt 6.11 the source is set with "model"; the Qt 6.12 documentation
# calls the same property "sourceModel".

Feature: SortFilterProxyModel
  A SortFilterProxyModel shows the rows of another model, sorted by its
  sorters and with the rows its filters refuse left out.

  @qt-parity
  Rule: With no sorters and no filters, the proxy shows the source as it is

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          property int made: 0
          property string shown: ""
          function show() { var s = []; for (var i = 0; i < rep.count; ++i) { var o = rep.itemAt(i); s.push(o ? o.tag : "none") } shown = s.join(",") }
          ListModel {
            id: lm; objectName: "lm"
            ListElement { name: "c"; cost: 3; grp: "x" }
            ListElement { name: "a"; cost: 1; grp: "y" }
            ListElement { name: "b"; cost: 2; grp: "x" }
          }
          SortFilterProxyModel { id: pm; objectName: "pm"; model: lm }
          Repeater {
            id: rep; objectName: "rep"; model: pm
            Item { property string tag: name + cost; Component.onCompleted: root.made++ }
          }
        }
        """

    Scenario: The rows of the source appear unchanged and in order
      When the method "show" of "root" is called
      Then the property "shown" of "root" is "c3,a1,b2"
      And the property "count" of "rep" is 3
      And the property "made" of "root" is 3
      And the property "dynamicSortFilter" of "pm" is true
      And the property "recursiveFiltering" of "pm" is false
      And the property "autoAcceptChildRows" of "pm" is false

    Scenario: A row appended to the source appears at the end
      When the method "append" of "lm" is called with {"name": "d", "cost": 4, "grp": "x"}
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "c3,a1,b2,d4"
      And the property "count" of "rep" is 4
      And the property "made" of "root" is 4

    Scenario: A row removed from the source goes, and the others keep their delegates
      When the method "remove" of "lm" is called with 0
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "a1,b2"
      And the property "count" of "rep" is 2
      And the property "made" of "root" is 3

    Scenario: A changed role of a source row is read again by its delegate
      When the method "setProperty" of "lm" is called with 0, "name", "z"
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "z3,a1,b2"
      And the property "made" of "root" is 3

    # Qt makes the delegates again when the source moves rows, unlike
    # a move read straight from the source (qml6 probe, Qt 6.11.2).
    Scenario: Rows moved in the source move in the proxy, with new delegates
      When the method "move" of "lm" is called with 0, 2, 1
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "a1,b2,c3"
      And the property "made" of "root" is 6

  @qt-parity
  Rule: A RoleSorter orders the rows by one role

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          property int made: 0
          property string shown: ""
          function show() { var s = []; for (var i = 0; i < rep.count; ++i) { var o = rep.itemAt(i); s.push(o ? o.tag : "none") } shown = s.join(",") }
          ListModel {
            id: lm; objectName: "lm"
            ListElement { name: "c"; cost: 3 }
            ListElement { name: "a"; cost: 1 }
            ListElement { name: "b"; cost: 2 }
            ListElement { name: "d"; cost: 2 }
          }
          RoleSorter { id: rs; objectName: "rs"; roleName: "cost" }
          SortFilterProxyModel { id: pm; objectName: "pm"; model: lm; sorters: [rs] }
          Repeater {
            id: rep; objectName: "rep"; model: pm
            Item { property string tag: name + cost; Component.onCompleted: root.made++ }
          }
        }
        """

    Scenario: The rows are in ascending order of the role, equal rows in source order
      When the method "show" of "root" is called
      Then the property "shown" of "root" is "a1,b2,d2,c3"
      And the property "count" of "rep" is 4
      And the property "roleName" of "rs" is "cost"
      And the property "sortOrder" of "rs" is 0
      And the property "enabled" of "rs" is true

    Scenario: Descending order is the ascending order turned around
      When the property "sortOrder" of "rs" is set to 1
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "c3,d2,b2,a1"
      And the property "made" of "root" is 4

    Scenario: A row appended to the source lands where it sorts
      When the method "append" of "lm" is called with {"name": "e", "cost": 0}
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "e0,a1,b2,d2,c3"
      And the property "made" of "root" is 5

    Scenario: A change of the sorted role moves the row, keeping its delegate
      When the method "setProperty" of "lm" is called with 0, "cost", 0
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "c0,a1,b2,d2"
      And the property "made" of "root" is 4

    Scenario: A sorter that is not enabled leaves the source order
      When the property "enabled" of "rs" is set to false
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "c3,a1,b2,d2"
      And the property "made" of "root" is 4

  @qt-parity
  Rule: A ValueFilter shows only the rows whose role has its value

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          property string shown: ""
          function show() { var s = []; for (var i = 0; i < rep.count; ++i) { var o = rep.itemAt(i); s.push(o ? o.tag : "none") } shown = s.join(",") }
          ListModel {
            id: lm; objectName: "lm"
            ListElement { name: "c"; grp: "x" }
            ListElement { name: "a"; grp: "y" }
            ListElement { name: "b"; grp: "x" }
          }
          ValueFilter { id: vf; objectName: "vf"; roleName: "grp"; value: "x" }
          SortFilterProxyModel { id: pm; objectName: "pm"; model: lm; filters: [vf] }
          Repeater {
            id: rep; objectName: "rep"; model: pm
            Item { property string tag: name }
          }
        }
        """

    Scenario: Only the matching rows appear, and the count is theirs
      When the method "show" of "root" is called
      Then the property "shown" of "root" is "c,b"
      And the property "count" of "rep" is 2
      And the property "roleName" of "vf" is "grp"
      And the property "value" of "vf" is "x"
      And the property "inverted" of "vf" is false

    Scenario: A row appended to the source that does not match stays hidden
      When the method "append" of "lm" is called with {"name": "e", "grp": "y"}
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "c,b"
      And the property "count" of "rep" is 2

    Scenario: A row appended to the source that matches appears
      When the method "append" of "lm" is called with {"name": "f", "grp": "x"}
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "c,b,f"
      And the property "count" of "rep" is 3

    Scenario: A row whose role changes to match appears where it stands in the source
      When the method "setProperty" of "lm" is called with 1, "grp", "x"
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "c,a,b"
      And the property "count" of "rep" is 3

    Scenario: A row whose role changes away from the value goes
      When the method "setProperty" of "lm" is called with 0, "grp", "y"
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "b"
      And the property "count" of "rep" is 1

    Scenario: Changing the value of the filter changes which rows appear
      When the property "value" of "vf" is set to "y"
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "a"
      And the property "count" of "rep" is 1

    Scenario: An inverted filter shows the rows it would have hidden
      When the property "inverted" of "vf" is set to true
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "a"
      And the property "count" of "rep" is 1

  @qt-parity
  Rule: In a ListView, the rows stand in the proxy's order

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          width: 100; height: 100
          ListModel {
            id: lm; objectName: "lm"
            ListElement { name: "c"; cost: 3 }
            ListElement { name: "a"; cost: 1 }
            ListElement { name: "b"; cost: 2 }
          }
          SortFilterProxyModel {
            id: pm; objectName: "pm"; model: lm
            sorters: [ RoleSorter { roleName: "cost" } ]
          }
          ListView {
            id: lv; objectName: "lv"; anchors.fill: parent; model: pm
            delegate: Item { objectName: name; width: 100; height: 20 }
          }
        }
        """

    Scenario: The delegates stand sorted, top to bottom
      When 100 milliseconds pass
      Then the property "count" of "lv" is 3
      And the property "y" of "a" is 0
      And the property "y" of "b" is 20
      And the property "y" of "c" is 40

  @qt-parity
  Rule: A StringSorter orders text, and a proxy can feed a proxy

    Scenario: A StringSorter orders text, with its case sensitivity
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          property string shown: ""
          function show() { var s = []; for (var i = 0; i < rep.count; ++i) { var o = rep.itemAt(i); s.push(o ? o.tag : "none") } shown = s.join(",") }
          ListModel {
            id: lm; objectName: "lm"
            ListElement { name: "banana" }
            ListElement { name: "Apple" }
            ListElement { name: "cherry" }
          }
          StringSorter { id: ss; objectName: "ss"; roleName: "name" }
          SortFilterProxyModel { id: pm; objectName: "pm"; model: lm; sorters: [ss] }
          Repeater {
            id: rep; objectName: "rep"; model: pm
            Item { property string tag: name }
          }
        }
        """
      When the method "show" of "root" is called
      Then the property "shown" of "root" is "Apple,banana,cherry"
      When the property "caseSensitivity" of "ss" is set to 0
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "Apple,banana,cherry"

    Scenario: A proxy can be the source of another proxy
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          property string shown: ""
          function show() { var s = []; for (var i = 0; i < rep.count; ++i) { var o = rep.itemAt(i); s.push(o ? o.tag : "none") } shown = s.join(",") }
          ListModel {
            id: lm; objectName: "lm"
            ListElement { name: "c"; cost: 3; grp: "x" }
            ListElement { name: "a"; cost: 1; grp: "y" }
            ListElement { name: "b"; cost: 2; grp: "x" }
          }
          SortFilterProxyModel {
            id: inner; model: lm
            filters: [ ValueFilter { roleName: "grp"; value: "x" } ]
          }
          SortFilterProxyModel {
            id: outer; objectName: "outer"; model: inner
            sorters: [ RoleSorter { roleName: "cost" } ]
          }
          Repeater {
            id: rep; objectName: "rep"; model: outer
            Item { property string tag: name + cost }
          }
        }
        """
      When the method "show" of "root" is called
      Then the property "shown" of "root" is "b2,c3"
      And the property "count" of "rep" is 2

  # Not built yet: the rest of the Qt 6.11 surface. Each scenario below
  # says what it waits for. FunctionFilter and FunctionSorter are not
  # tagged @qt-parity: qml6 of Qt 6.11.2 dumps core on the documented
  # FunctionFilter example, so the oracle cannot verify them.
  Rule: The rest of the surface

    @backlog @qt-parity
    Scenario: Without dynamic sorting, the source order stands until the sorter is invalidated
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          property string shown: ""
          function show() { var s = []; for (var i = 0; i < rep.count; ++i) { var o = rep.itemAt(i); s.push(o ? o.tag : "none") } shown = s.join(",") }
          ListModel {
            id: lm; objectName: "lm"
            ListElement { name: "c"; cost: 3 }
            ListElement { name: "a"; cost: 1 }
            ListElement { name: "b"; cost: 2 }
          }
          SortFilterProxyModel {
            id: pm; objectName: "pm"; model: lm; dynamicSortFilter: false
            sorters: [ RoleSorter { roleName: "cost" } ]
          }
          Repeater {
            id: rep; objectName: "rep"; model: pm
            Item { property string tag: name + cost }
          }
        }
        """
      When the method "show" of "root" is called
      Then the property "shown" of "root" is "c3,a1,b2"
      When the method "invalidateSorter" of "pm" is called
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "a1,b2,c3"

    # A StringSorter is built, but only its case sensitivity changes
    # the order: locale collation, numeric mode and ignoring
    # punctuation wait for collation in the text stack.
    @backlog @qt-parity
    Scenario: A StringSorter in numeric mode orders the numbers in text by value
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          property string shown: ""
          function show() { var s = []; for (var i = 0; i < rep.count; ++i) { var o = rep.itemAt(i); s.push(o ? o.tag : "none") } shown = s.join(",") }
          ListModel {
            id: lm; objectName: "lm"
            ListElement { name: "a10" }
            ListElement { name: "a2" }
            ListElement { name: "a1" }
          }
          StringSorter { id: ss; objectName: "ss"; roleName: "name" }
          SortFilterProxyModel { id: pm; objectName: "pm"; model: lm; sorters: [ss] }
          Repeater {
            id: rep; objectName: "rep"; model: pm
            Item { property string tag: name }
          }
        }
        """
      When the method "show" of "root" is called
      Then the property "shown" of "root" is "a1,a10,a2"
      When the property "numericMode" of "ss" is set to true
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "a1,a2,a10"

    @backlog
    Scenario: A FunctionFilter decides from the row it is given
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          property string shown: ""
          function show() { var s = []; for (var i = 0; i < rep.count; ++i) { var o = rep.itemAt(i); s.push(o ? o.tag : "none") } shown = s.join(",") }
          ListModel {
            id: lm; objectName: "lm"
            ListElement { name: "c"; cost: 3 }
            ListElement { name: "a"; cost: 1 }
            ListElement { name: "b"; cost: 2 }
          }
          SortFilterProxyModel {
            id: pm; objectName: "pm"; model: lm
            filters: [ FunctionFilter {
              component RoleData: QtObject { property int cost }
              function filter(data: RoleData) : bool { return data.cost >= 2 }
            } ]
          }
          Repeater {
            id: rep; objectName: "rep"; model: pm
            Item { property string tag: name }
          }
        }
        """
      When the method "show" of "root" is called
      Then the property "shown" of "root" is "c,b"
      And the property "count" of "rep" is 2
