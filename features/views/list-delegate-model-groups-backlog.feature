# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-models-delegatemodel.html
#   https://doc.qt.io/qt-6/qml-qtqml-models-delegatemodelgroup.html
#   https://doc.qt.io/qt-6/qml-qtqml-models-package.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# The scenarios here are @backlog on Ljós: for every group of a DelegateModel Qt adds two
# attached properties named after that group to each delegate, and a delegate of a filtered
# DelegateModel still carries the index of its row in the model under it. Both need the
# engine to give an attached type and a row the names and number of one DelegateModel, which
# no type can do for itself yet. `rootIndex` waits for a tree model whose rows have children,
# and `parts` waits for the Package type, which Ljós does not have. A group's `get`, `create`
# and `resolve` hand out a row object or a delegate made outside a view, which the engine
# makes for itself and no type can ask it for yet.

Feature: The parts of DelegateModel groups Ljós does not have yet

  @backlog @qt-parity
  Scenario: A delegate carries an attached property for each group of its model
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        id: root; objectName: "root"; width: 200; height: 300
        function pick() { dm.items.addGroups(1, 1, ["selected"]) }
        ListModel { id: lm; ListElement { name: "a" } ListElement { name: "b" } ListElement { name: "c" } }
        DelegateModel {
          id: dm; objectName: "dm"; model: lm
          groups: [DelegateModelGroup { name: "selected" }]
          delegate: Item {
            required property string name
            objectName: "row" + name; width: 200; height: 20
            property bool inSel: DelegateModel.inSelected
            property int selIdx: DelegateModel.selectedIndex
          }
        }
        ListView { objectName: "list"; anchors.fill: parent; model: dm }
      }
      """
    When 100 milliseconds pass
    And the method "pick" of "root" is called
    And 100 milliseconds pass
    Then the property "inSel" of "rowb" is true
    And the property "selIdx" of "rowb" is 0
    And the property "inSel" of "rowa" is false
    And the property "selIdx" of "rowc" is 1

  @backlog @qt-parity
  Scenario: A delegate of a filtered model carries the index of its row in the model
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        id: root; objectName: "root"; width: 200; height: 300
        function pick() { dm.items.addGroups(1, 1, ["selected"]); dm.items.addGroups(3, 1, ["selected"]); dm.filterOnGroup = "selected" }
        ListModel { id: lm; ListElement { name: "a" } ListElement { name: "b" } ListElement { name: "c" } ListElement { name: "d" } }
        DelegateModel {
          id: dm; objectName: "dm"; model: lm
          groups: [DelegateModelGroup { name: "selected" }]
          delegate: Item {
            required property string name
            required property int index
            objectName: "row" + name; width: 200; height: 20
            property int seen: index
          }
        }
        ListView { objectName: "list"; anchors.fill: parent; model: dm }
      }
      """
    When 100 milliseconds pass
    And the method "pick" of "root" is called
    And 100 milliseconds pass
    Then the property "count" of "list" is 2
    And the property "seen" of "rowb" is 1
    And the property "seen" of "rowd" is 3

  @backlog @qt-parity @api-QtQml.Models.DelegateModelGroup.get
  Scenario: An item of a group can be read as an object of its own
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        id: root; objectName: "root"; width: 200; height: 300
        property string gotName: ""; property bool gotIn: false; property int gotIdx: -1
        function read() { var g = dm.items.get(1); root.gotName = g.model.name; root.gotIn = g.inItems; root.gotIdx = g.itemsIndex }
        ListModel { id: lm; ListElement { name: "a" } ListElement { name: "b" } }
        DelegateModel {
          id: dm; objectName: "dm"; model: lm
          delegate: Item { required property string name; objectName: "row" + name; width: 200; height: 20 }
        }
        ListView { objectName: "list"; anchors.fill: parent; model: dm }
      }
      """
    When 100 milliseconds pass
    And the method "read" of "root" is called
    Then the property "gotName" of "root" is "b"
    And the property "gotIn" of "root" is true
    And the property "gotIdx" of "root" is 1

  @backlog @qt-parity @api-QtQml.Models.DelegateModel.rootIndex
  Scenario: The rows under a root index are the rows of the model
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        id: root; objectName: "root"; width: 200; height: 300
        function toRoot() { dm.rootIndex = dm.modelIndex(0) }
        function back() { dm.rootIndex = dm.parentModelIndex() }
        ListModel { id: lm; ListElement { name: "a" } ListElement { name: "b" } }
        DelegateModel {
          id: dm; objectName: "dm"; model: lm
          delegate: Item { required property string name; objectName: "row" + name; width: 200; height: 20 }
        }
        ListView { objectName: "list"; anchors.fill: parent; model: dm }
      }
      """
    When 100 milliseconds pass
    And the method "toRoot" of "root" is called
    And 100 milliseconds pass
    Then the property "count" of "dm" is 0
    And the property "count" of "list" is 0
    When the method "back" of "root" is called
    And 100 milliseconds pass
    Then the property "count" of "dm" is 2
    And the property "count" of "list" is 2

  @backlog @qt-parity @api-QtQml.Models.DelegateModel.parts
  Scenario: A part of a package delegate is a model of its own
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        width: 200; height: 300
        DelegateModel {
          id: dm; objectName: "dm"; model: 2
          delegate: Package {
            Item { Package.name: "list"; objectName: "part" + index; width: 200; height: 20 }
          }
        }
        ListView { objectName: "list"; anchors.fill: parent; model: dm.parts.list }
      }
      """
    When 100 milliseconds pass
    Then the property "count" of "list" is 2
    And the property "contentHeight" of "list" is 40
    And the property "y" of "part1" is 20
