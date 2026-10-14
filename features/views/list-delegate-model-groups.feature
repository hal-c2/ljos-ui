# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-models-delegatemodel.html
#   https://doc.qt.io/qt-6/qml-qtqml-models-delegatemodelgroup.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQml.Models.DelegateModel.items @api-QtQml.Models.DelegateModel.persistedItems @api-QtQml.Models.DelegateModel.groups @api-QtQml.Models.DelegateModel.filterOnGroup @api-QtQml.Models.DelegateModelGroup.count @api-QtQml.Models.DelegateModelGroup.includeByDefault @api-QtQml.Models.DelegateModelGroup.setGroups @api-QtQml.Models.DelegateModelGroup.addGroups @api-QtQml.Models.DelegateModelGroup.remove
Feature: The groups of a DelegateModel
  Every DelegateModel has an items group with a delegate item for each row of its model and a
  persistedItems group that starts empty. Further groups hold the items put in them, and the
  group named by filterOnGroup is the one a view shows.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        id: root; objectName: "root"; width: 200; height: 300
        property int itemsCount: dm.items.count; property int persistedCount: dm.persistedItems.count
        property int selCount: sel.count; property int allCount: allg.count
        property int groupsCount: dm.groups.length; property string filterGroup: dm.filterOnGroup
        function pick() { dm.items.addGroups(1, 1, ["selected"]); dm.items.addGroups(3, 1, ["selected"]) }
        function filterSel() { dm.filterOnGroup = "selected" }
        function filterItems() { dm.filterOnGroup = "items" }
        function moveA() { dm.items.setGroups(0, 1, ["selected"]) }
        function unpick() { sel.remove(0, 1) } function addE() { lm.append({"name": "e"}) } function addZ() { sel.insert({"name": "z"}) }
        ListModel { id: lm; ListElement { name: "a" } ListElement { name: "b" } ListElement { name: "c" } ListElement { name: "d" } }
        DelegateModel {
          id: dm; objectName: "dm"; model: lm
          groups: [DelegateModelGroup { id: sel; name: "selected" }, DelegateModelGroup { id: allg; name: "allg"; includeByDefault: true }]
          delegate: Item {
            required property string name; objectName: "row" + name; width: 200; height: 20
            property bool inItems: DelegateModel.inItems; property bool inPersisted: DelegateModel.inPersistedItems
            property int itemsIdx: DelegateModel.itemsIndex; property var grp: DelegateModel.groups
            property bool isDm: DelegateModel.model === dm; property bool unresolved: DelegateModel.isUnresolved
          }
        }
        ListView { id: list; objectName: "list"; anchors.fill: parent; model: dm }
      }
      """
    When 100 milliseconds pass

  Scenario: The default groups and a group that takes every new item
    Then the property "itemsCount" of "root" is 4
    And the property "persistedCount" of "root" is 0
    And the property "selCount" of "root" is 0
    And the property "allCount" of "root" is 4
    And the property "groupsCount" of "root" is 4
    And the property "filterGroup" of "root" is "items"
    And the property "count" of "dm" is 4
    And the property "count" of "list" is 4

  Scenario: A delegate says which groups its item is in
    Then the property "inItems" of "rowb" is true
    And the property "inPersisted" of "rowb" is false
    And the property "itemsIdx" of "rowb" is 1
    And the property "grp" of "rowb" is ["items", "allg"]
    And the property "grp" of "rowd" is ["items", "allg"]
    And the property "isDm" of "rowb" is true

  Scenario: Items put in a group are counted and named on their delegates
    When the method "pick" of "root" is called
    And 100 milliseconds pass
    Then the property "selCount" of "root" is 2
    And the property "itemsCount" of "root" is 4
    And the property "count" of "list" is 4
    And the property "grp" of "rowb" is ["items", "selected", "allg"]
    And the property "grp" of "rowa" is ["items", "allg"]

  Scenario: The view shows only the group it is filtered on
    When the method "pick" of "root" is called
    And the method "filterSel" of "root" is called
    And 100 milliseconds pass
    Then the property "count" of "list" is 2
    And the property "count" of "dm" is 2
    And the property "contentHeight" of "list" is 40
    And the property "itemsCount" of "root" is 4
    And the property "y" of "rowb" is 0
    And the property "y" of "rowd" is 20
    And the object "rowa" does not exist

  Scenario: Filtering back on the items group shows every row again
    When the method "pick" of "root" is called
    And the method "filterSel" of "root" is called
    And 100 milliseconds pass
    And the method "filterItems" of "root" is called
    And 100 milliseconds pass
    Then the property "count" of "list" is 4
    And the property "contentHeight" of "list" is 80
    And the property "y" of "rowa" is 0
    And the property "y" of "rowd" is 60

  Scenario: Setting an item's groups takes it out of the ones it was in
    When the method "moveA" of "root" is called
    And 100 milliseconds pass
    Then the property "itemsCount" of "root" is 3
    And the property "selCount" of "root" is 1
    And the property "allCount" of "root" is 3
    And the property "count" of "list" is 3
    And the property "y" of "rowb" is 0
    And the object "rowa" does not exist
    When the method "filterSel" of "root" is called
    And 100 milliseconds pass
    Then the property "count" of "list" is 1
    And the property "y" of "rowa" is 0
    And the property "inItems" of "rowa" is false
    And the property "grp" of "rowa" is ["selected"]

  Scenario: Removing an item from the filtered group takes it out of the view
    When the method "pick" of "root" is called
    And the method "filterSel" of "root" is called
    And 100 milliseconds pass
    And the method "unpick" of "root" is called
    And 100 milliseconds pass
    Then the property "selCount" of "root" is 1
    And the property "itemsCount" of "root" is 4
    And the property "count" of "list" is 1
    And the property "y" of "rowd" is 0
    And the object "rowb" does not exist

  Scenario: Data inserted into a group is shown where the view is filtered on that group
    When the method "pick" of "root" is called
    And the method "addZ" of "root" is called
    And the method "filterSel" of "root" is called
    And 100 milliseconds pass
    Then the property "selCount" of "root" is 3
    And the property "itemsCount" of "root" is 4
    And the property "count" of "list" is 3
    And the property "y" of "rowz" is 40
    And the property "grp" of "rowz" is ["selected"]
    And the property "unresolved" of "rowz" is true
    And the property "unresolved" of "rowb" is false

  Scenario: A row added to the model joins the groups that take new items
    When the method "addE" of "root" is called
    And 100 milliseconds pass
    Then the property "itemsCount" of "root" is 5
    And the property "allCount" of "root" is 5
    And the property "selCount" of "root" is 0
    And the property "count" of "list" is 5
    And the property "grp" of "rowe" is ["items", "allg"]
    And the property "y" of "rowe" is 80
